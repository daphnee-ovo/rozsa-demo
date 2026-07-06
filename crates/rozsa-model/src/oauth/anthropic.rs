use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::callback_server;
use super::pkce;
use super::types::{OAuthCredentials, OAuthFlowEvent, OAuthLoginError};

const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const AUTHORIZE_URL: &str = "https://claude.ai/oauth/authorize";
const TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
const CALLBACK_PORT: u16 = 53692;
const REDIRECT_URI: &str = "http://127.0.0.1:53692/callback";
const SCOPE: &str = "org:create_api_key user:profile user:inference user:sessions:claude_code user:mcp_servers user:file_upload";

/// Execute the Anthropic OAuth login flow.
/// Emits OAuthFlowEvents through `event_tx` for the bridge to forward to TS.
/// Receives user input (e.g., manual code paste) through `response_rx`.
pub async fn login(
    event_tx: mpsc::UnboundedSender<OAuthFlowEvent>,
    mut response_rx: mpsc::UnboundedReceiver<serde_json::Value>,
    cancel: CancellationToken,
) -> Result<OAuthCredentials, OAuthLoginError> {
    // 1. Generate PKCE verifier + challenge
    let verifier = pkce::generate_verifier();
    let challenge = pkce::generate_challenge(&verifier);

    // 2. Use verifier as state (same as TS implementation)
    let state = verifier.clone();

    // 3. Build authorization URL
    let auth_url = build_auth_url(&challenge, &state);

    // 4. Send auth_url event to TS (so it opens the browser)
    let _ = event_tx.send(OAuthFlowEvent::AuthUrl {
        url: auth_url,
        instructions: Some(
            "Complete login in your browser. If the browser is on another machine, paste the final redirect URL here."
                .to_string(),
        ),
    });

    // 5. Race: wait for callback server OR manual code input
    let code_and_state = tokio::select! {
        // Wait for callback server
        callback_result = callback_server::wait_for_callback(CALLBACK_PORT, &state, cancel.clone()) => {
            match callback_result {
                Ok(result) => (result.code, result.state),
                Err(e) => return Err(e),
            }
        }
        // Wait for manual code input
        user_response = response_rx.recv() => {
            match user_response {
                Some(value) => {
                    let input = value
                        .get("response")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            OAuthLoginError::Provider("missing response field in user input".to_string())
                        })?;
                    parse_authorization_input(input, &state)?
                }
                None => {
                    return Err(OAuthLoginError::Cancelled);
                }
            }
        }
        // Wait for cancellation
        _ = cancel.cancelled() => {
            return Err(OAuthLoginError::Cancelled);
        }
    };

    let code = code_and_state.0;
    let _state = code_and_state.1;

    // 6. Send progress event
    let _ = event_tx.send(OAuthFlowEvent::Progress {
        message: "Exchanging authorization code for tokens...".to_string(),
    });

    // 7. Exchange authorization code for tokens
    exchange_code(&code, &verifier).await
}

#[doc(hidden)]
pub fn build_auth_url(challenge: &str, state: &str) -> String {
    let scope_encoded = SCOPE.replace(' ', "%20");
    format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method=S256",
        AUTHORIZE_URL, CLIENT_ID, REDIRECT_URI, scope_encoded, state, challenge,
    )
}

/// Parse user input as either a full URL, query string, or raw code.
/// Returns (code, state).
#[doc(hidden)]
pub fn parse_authorization_input(
    input: &str,
    expected_state: &str,
) -> Result<(String, String), OAuthLoginError> {
    let value = input.trim();
    if value.is_empty() {
        return Err(OAuthLoginError::Provider("empty input".to_string()));
    }

    // Try parsing as URL (extract query string after '?')
    if value.starts_with("http://") || value.starts_with("https://") {
        if let Some(query_start) = value.find('?') {
            let query = &value[query_start + 1..];
            let params = parse_query_string(query);
            if let Some(code) = params.get("code") {
                let state = params
                    .get("state")
                    .cloned()
                    .unwrap_or_else(|| expected_state.to_string());
                if state != expected_state {
                    return Err(OAuthLoginError::CallbackServer(
                        "state mismatch".to_string(),
                    ));
                }
                return Ok((code.clone(), state));
            }
        }
    }

    // Try parsing as "code#state"
    if value.contains('#') {
        let parts: Vec<&str> = value.splitn(2, '#').collect();
        if parts.len() == 2 {
            let code = parts[0].to_string();
            let state = parts[1].to_string();
            if state != expected_state {
                return Err(OAuthLoginError::CallbackServer(
                    "state mismatch".to_string(),
                ));
            }
            return Ok((code, state));
        }
    }

    // Try parsing as query string
    if value.contains("code=") {
        let params = parse_query_string(value);
        if let Some(code) = params.get("code") {
            let state = params
                .get("state")
                .cloned()
                .unwrap_or_else(|| expected_state.to_string());
            if state != expected_state {
                return Err(OAuthLoginError::CallbackServer(
                    "state mismatch".to_string(),
                ));
            }
            return Ok((code.clone(), state));
        }
    }

    // Treat as raw code
    Ok((value.to_string(), expected_state.to_string()))
}

fn parse_query_string(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter_map(|pair| {
            let mut split = pair.splitn(2, '=');
            let key = split.next()?.to_string();
            let value = split.next()?.to_string();
            Some((key, value))
        })
        .collect()
}

async fn exchange_code(code: &str, verifier: &str) -> Result<OAuthCredentials, OAuthLoginError> {
    let client = reqwest::Client::new();

    let response = client
        .post(TOKEN_URL)
        .json(&serde_json::json!({
            "grant_type": "authorization_code",
            "client_id": CLIENT_ID,
            "code": code,
            "redirect_uri": REDIRECT_URI,
            "code_verifier": verifier,
        }))
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| OAuthLoginError::Network(e.to_string()))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(OAuthLoginError::TokenExchange(format!(
            "HTTP {}: {}",
            status, body
        )));
    }

    let body: TokenResponse = response
        .json()
        .await
        .map_err(|e| OAuthLoginError::TokenExchange(e.to_string()))?;

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    Ok(OAuthCredentials {
        access: body.access_token,
        refresh: body.refresh_token,
        expires: now_ms + (body.expires_in as i64) * 1000 - 5 * 60 * 1000,
        extra: Default::default(),
    })
}

#[derive(Debug, Deserialize, Serialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: u64,
}
