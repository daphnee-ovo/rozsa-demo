//! JSONL stdio bridge executable for the Rust model layer.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{Mutex, mpsc};
use tokio_util::sync::CancellationToken;

use rozsa_model::credentials::resolve_request_options;
use rozsa_model::protocol::{
    BridgeInput, BridgeMethod, BridgeOutput, bridge_error, event_to_bridge_output, parse_input_line,
    provider_request,
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
