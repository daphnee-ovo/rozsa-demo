//! JSONL stdio bridge executable for the Rust model layer.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{Mutex, mpsc};
use tokio_util::sync::CancellationToken;

use rozsa_model::credentials::{resolve_request_options, store_oauth_credentials};
use rozsa_model::oauth::types::OAuthFlowEvent;
use rozsa_model::protocol::{
    BridgeInput, BridgeMethod, BridgeOutput, bridge_error, event_to_bridge_output, oauth_event,
    parse_input_line, provider_request,
};
use rozsa_model::providers::register_builtin_providers;
use rozsa_model::registry::get_provider;

/// Run the JSONL bridge loop until stdin closes, handling concurrent requests.
#[tokio::main]
async fn main() {
    register_builtin_providers();
    let stdin = BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();

    // Channel for serializing stdout writes from concurrent tasks
    let (output_tx, mut output_rx) = mpsc::unbounded_channel::<BridgeOutput>();

    // Map of active request cancellation tokens
    let active_requests = Arc::new(Mutex::new(HashMap::<String, CancellationToken>::new()));

    // Map of active OAuth sessions: id -> channel to send user responses
    let oauth_responses: Arc<Mutex<HashMap<String, mpsc::UnboundedSender<Value>>>> =
        Arc::new(Mutex::new(HashMap::new()));

    // Spawn stdout writer task
    let writer_handle = tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        while let Some(output) = output_rx.recv().await {
            write_bridge_output(&mut stdout, output).await;
        }
    });

    // Main input loop: read lines and spawn concurrent request handlers
    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() {
            continue;
        }

        let input = match parse_input_line(&line) {
            Ok(input) => input,
            Err(error) => {
                let _ = output_tx.send(bridge_error("unknown", error, "parse_error"));
                continue;
            }
        };

        match input {
            BridgeInput::Request { id, method, model, context, options } => {
                let cancel_token = CancellationToken::new();
                active_requests.lock().await.insert(id.clone(), cancel_token.clone());

                let output_tx_clone = output_tx.clone();
                let active_requests_clone = active_requests.clone();

                tokio::spawn(async move {
                    handle_request(
                        id.clone(),
                        method,
                        model,
                        context,
                        options,
                        cancel_token,
                        output_tx_clone.clone(),
                    )
                    .await;
                    // Remove token after completion
                    active_requests_clone.lock().await.remove(&id);
                });
            }
            BridgeInput::Cancel { id } => {
                if let Some(token) = active_requests.lock().await.remove(&id) {
                    token.cancel();
                }
            }
            BridgeInput::OAuthLogin { id, provider, options } => {
                let auth_json_path = options
                    .get("authJsonPath")
                    .and_then(Value::as_str)
                    .unwrap_or("~/.rozsa/auth.json")
                    .to_string();

                // Check if it's a known built-in provider
                let is_builtin = matches!(
                    provider.as_str(),
                    "anthropic" | "github-copilot" | "openai-codex"
                );

                if !is_builtin {
                    // Extension providers: signal TS to handle login in JS
                    let _ = output_tx.send(oauth_event(&id, json!({ "type": "delegate" })));
                    continue;
                }

                let cancel_token = CancellationToken::new();
                active_requests.lock().await.insert(id.clone(), cancel_token.clone());

                // Create response channel for this OAuth session
                let (resp_tx, resp_rx) = mpsc::unbounded_channel::<Value>();
                oauth_responses.lock().await.insert(id.clone(), resp_tx);

                let output_tx_clone = output_tx.clone();
                let active_requests_clone = active_requests.clone();
                let oauth_responses_clone = oauth_responses.clone();

                tokio::spawn(async move {
                    handle_oauth_login(
                        id.clone(),
                        provider,
                        auth_json_path,
                        resp_rx,
                        cancel_token,
                        output_tx_clone,
                    )
                    .await;
                    active_requests_clone.lock().await.remove(&id);
                    oauth_responses_clone.lock().await.remove(&id);
                });
            }
            BridgeInput::OAuthResponse { id, response } => {
                if let Some(tx) = oauth_responses.lock().await.get(&id) {
                    let _ = tx.send(response);
                }
            }
        }
    }

    // stdin closed: cancel all active requests
    let tokens: Vec<CancellationToken> = {
        let mut map = active_requests.lock().await;
        map.drain().map(|(_, token)| token).collect()
    };
    for token in tokens {
        token.cancel();
    }

    // Close output channel and wait for writer to finish
    drop(output_tx);
    let _ = writer_handle.await;
}

/// Handle one request: resolve credentials, stream events, check for cancellation.
async fn handle_request(
    id: String,
    method: BridgeMethod,
    model: serde_json::Value,
    context: serde_json::Value,
    options: serde_json::Value,
    cancel_token: CancellationToken,
    output_tx: mpsc::UnboundedSender<BridgeOutput>,
) {
    let request = match provider_request(BridgeInput::Request {
        id: id.clone(),
        method,
        model,
        context,
        options: options.clone(),
    }) {
        Ok(Some(request)) => request,
        Ok(None) => return,
        Err(error) => {
            let _ = output_tx.send(bridge_error(&id, error, "request_error"));
            return;
        }
    };

    let options_resolved = match resolve_request_options(
        &request.model,
        &request.options,
        request.models_json_path.as_deref(),
        request.auth_json_path.as_deref(),
    )
    .await
    {
        Ok(options) => options,
        Err(error) => {
            let _ = output_tx.send(bridge_error(&id, error, "credential_error"));
            return;
        }
    };

    // Get stream from provider — must not hold provider guard across await points
    let mut stream = {
        let Some(provider) = get_provider(&request.model.api) else {
            let _ = output_tx.send(bridge_error(
                &id,
                format!("No provider registered for api: {:?}", request.model.api),
                "unsupported_api",
            ));
            return;
        };

        match request.method {
            BridgeMethod::Stream | BridgeMethod::StreamSimple => {
                provider.stream_simple(&request.model, &request.context, &options_resolved)
            }
        }
    };

    // Stream events until completion or cancellation
    loop {
        tokio::select! {
            _ = cancel_token.cancelled() => {
                let _ = output_tx.send(bridge_error(&id, "Request cancelled", "aborted"));
                break;
            }
            event_opt = stream.next() => {
                match event_opt {
                    Some(event) => {
                        let _ = output_tx.send(event_to_bridge_output(&id, event));
                    }
                    None => {
                        // Stream ended naturally
                        break;
                    }
                }
            }
        }
    }
}

/// Handle an OAuth login request: dispatch to provider, forward events, store credentials.
async fn handle_oauth_login(
    id: String,
    provider: String,
    auth_json_path: String,
    response_rx: mpsc::UnboundedReceiver<Value>,
    cancel_token: CancellationToken,
    output_tx: mpsc::UnboundedSender<BridgeOutput>,
) {
    // Create event forwarding channel: login fn sends OAuthFlowEvent, we convert to bridge output
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<OAuthFlowEvent>();

    let id_clone = id.clone();
    let output_tx_clone = output_tx.clone();

    // Spawn event forwarder
    let forwarder = tokio::spawn(async move {
        while let Some(flow_event) = event_rx.recv().await {
            let bridge_event = flow_event_to_value(flow_event);
            let _ = output_tx_clone.send(oauth_event(&id_clone, bridge_event));
        }
    });

    // Dispatch to provider-specific login
    let result = match provider.as_str() {
        "anthropic" => {
            rozsa_model::oauth::anthropic::login(event_tx, response_rx, cancel_token).await
        }
        "github-copilot" => {
            rozsa_model::oauth::github_copilot::login(event_tx, response_rx, cancel_token).await
        }
        "openai-codex" => {
            rozsa_model::oauth::openai_codex::login(event_tx, response_rx, cancel_token).await
        }
        _ => unreachable!(),
    };

    // Wait for forwarder to drain
    let _ = forwarder.await;

    // Handle result
    match result {
        Ok(credentials) => {
            // Resolve auth.json path (expand ~)
            let resolved_path = resolve_tilde(&auth_json_path);

            // Write credentials to auth.json
            if let Err(e) = store_oauth_credentials(&resolved_path, &provider, &credentials) {
                let _ = output_tx.send(oauth_event(
                    &id,
                    json!({ "type": "error", "message": format!("Failed to store credentials: {e}") }),
                ));
                return;
            }

            // Send complete event with credentials
            let _ = output_tx.send(oauth_event(
                &id,
                json!({
                    "type": "complete",
                    "credentials": {
                        "access": credentials.access,
                        "refresh": credentials.refresh,
                        "expires": credentials.expires,
                    }
                }),
            ));
        }
        Err(e) => {
            let _ = output_tx.send(oauth_event(
                &id,
                json!({ "type": "error", "message": e.to_string() }),
            ));
        }
    }
}

fn flow_event_to_value(event: OAuthFlowEvent) -> Value {
    match event {
        OAuthFlowEvent::AuthUrl { url, instructions } => {
            let mut v = json!({ "type": "auth_url", "url": url });
            if let Some(instr) = instructions {
                v["instructions"] = json!(instr);
            }
            v
        }
        OAuthFlowEvent::DeviceCode { user_code, verification_uri } => {
            json!({ "type": "device_code", "userCode": user_code, "verificationUri": verification_uri })
        }
        OAuthFlowEvent::Prompt { message, placeholder } => {
            let mut v = json!({ "type": "prompt", "message": message });
            if let Some(ph) = placeholder {
                v["placeholder"] = json!(ph);
            }
            v
        }
        OAuthFlowEvent::Select { message, options } => {
            json!({ "type": "select", "message": message, "options": options })
        }
        OAuthFlowEvent::Progress { message } => {
            json!({ "type": "progress", "message": message })
        }
        OAuthFlowEvent::Waiting { message } => {
            json!({ "type": "waiting", "message": message })
        }
    }
}

fn resolve_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{home}/{rest}");
        }
    }
    path.to_string()
}

/// Serialize one bridge output and flush it to stdout immediately.
async fn write_bridge_output(stdout: &mut tokio::io::Stdout, output: BridgeOutput) {
    match serde_json::to_string(&output) {
        Ok(serialized) => {
            let _ = stdout.write_all(serialized.as_bytes()).await;
            let _ = stdout.write_all(b"\n").await;
            let _ = stdout.flush().await;
        }
        Err(error) => {
            let fallback = bridge_error("unknown", error, "serialize_error");
            if let Ok(serialized) = serde_json::to_string(&fallback) {
                let _ = stdout.write_all(serialized.as_bytes()).await;
                let _ = stdout.write_all(b"\n").await;
                let _ = stdout.flush().await;
            }
        }
    }
}
