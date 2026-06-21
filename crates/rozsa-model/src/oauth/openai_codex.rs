use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::callback_server;
use super::pkce;
use super::types::{OAuthCredentials, OAuthFlowEvent, OAuthLoginError};

const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const AUTHORIZE_URL: &str = "https://auth.openai.com/oauth/authorize";
const TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const CALLBACK_PORT: u16 = 1455;
const REDIRECT_URI: &str = "http://localhost:1455/auth/callback";
const SCOPE: &str = "openid profile email offline_access";

/// Execute the OpenAI Codex OAuth login flow.
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

    // 2. Generate random state (16 bytes hex)
    let state = generate_random_state();

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

fn build_auth_url(challenge: &str, state: &str) -> String {
    let scope_encoded = SCOPE.replace(' ', "%20");
    format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method=S256&id_token_add_organizations=true&codex_cli_simplified_flow=true&originator=pi",
        AUTHORIZE_URL,
        CLIENT_ID,
        url_encode_component(REDIRECT_URI),
        scope_encoded,
        state,
        challenge,
    )
}

fn url_encode_component(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            ':' => "%3A".to_string(),
            '/' => "%2F".to_string(),
            _ => c.to_string(),
        })
        .collect()
}

fn generate_random_state() -> String {
    let mut bytes = [0u8; 16];
    rand::rng().fill(&mut bytes);
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Parse user input as either a full URL, query string, or raw code.
/// Returns (code, state).
fn parse_authorization_input(
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
                    return Err(OAuthLoginError::CallbackServer("state mismatch".to_string()));
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
                return Err(OAuthLoginError::CallbackServer("state mismatch".to_string()));
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
                return Err(OAuthLoginError::CallbackServer("state mismatch".to_string()));
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

async fn exchange_code(
    code: &str,
    verifier: &str,
) -> Result<OAuthCredentials, OAuthLoginError> {
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

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(OAuthLoginError::TokenExchange(format!(
            "HTTP {}: {}",
            status,
            body
        )));
    }

    let body: TokenResponse = response
        .json()
        .await
        .map_err(|e| OAuthLoginError::TokenExchange(e.to_string()))?;

    // Extract accountId from JWT
    let account_id = extract_account_id_from_jwt(&body.access_token);

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    let mut extra = HashMap::new();
    if let Some(account_id) = account_id {
        extra.insert("accountId".to_string(), serde_json::json!(account_id));
    }

    // Note: OpenAI Codex does NOT subtract 5-minute buffer from expires
    Ok(OAuthCredentials {
        access: body.access_token,
        refresh: body.refresh_token,
        expires: now_ms + (body.expires_in as i64) * 1000,
        extra,
    })
}

/// Extract accountId from JWT without verification.
/// OpenAI JWT payload contains "https://api.openai.com/auth.chatgpt_account_id".
fn extract_account_id_from_jwt(token: &str) -> Option<String> {
    // JWT format: header.payload.signature
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }

    // Decode payload (base64url, may need padding)
    let payload_bytes = URL_SAFE_NO_PAD.decode(parts[1]).ok()?;
    let payload: serde_json::Value = serde_json::from_slice(&payload_bytes).ok()?;

    payload
        .get("https://api.openai.com/auth.chatgpt_account_id")
        .and_then(|v| v.as_str())
        .map(String::from)
}

#[derive(Debug, Deserialize, Serialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_auth_url() {
        let challenge = "test_challenge";
        let state = "test_state";
        let url = build_auth_url(challenge, state);
        assert!(url.contains("code_challenge=test_challenge"));
        assert!(url.contains("state=test_state"));
        assert!(url.contains("client_id="));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("id_token_add_organizations=true"));
        assert!(url.contains("codex_cli_simplified_flow=true"));
        assert!(url.contains("originator=pi"));
    }

    #[test]
    fn test_parse_authorization_input_raw_code() {
        let result = parse_authorization_input("abc123", "expected_state");
        assert!(result.is_ok());
        let (code, state) = result.unwrap();
        assert_eq!(code, "abc123");
        assert_eq!(state, "expected_state");
    }

    #[test]
    fn test_parse_authorization_input_query_string() {
        let result = parse_authorization_input("code=abc123&state=test_state", "test_state");
        assert!(result.is_ok());
        let (code, state) = result.unwrap();
        assert_eq!(code, "abc123");
        assert_eq!(state, "test_state");
    }

    #[test]
    fn test_parse_authorization_input_hash_format() {
        let result = parse_authorization_input("abc123#test_state", "test_state");
        assert!(result.is_ok());
        let (code, state) = result.unwrap();
        assert_eq!(code, "abc123");
        assert_eq!(state, "test_state");
    }

    #[test]
    fn test_parse_authorization_input_url() {
        let result = parse_authorization_input(
            "http://localhost:1455/auth/callback?code=abc123&state=test_state",
            "test_state",
        );
        assert!(result.is_ok());
        let (code, state) = result.unwrap();
        assert_eq!(code, "abc123");
        assert_eq!(state, "test_state");
    }

    #[test]
    fn test_parse_authorization_input_state_mismatch() {
        let result = parse_authorization_input("code=abc123&state=wrong", "expected");
        assert!(result.is_err());
        match result.unwrap_err() {
            OAuthLoginError::CallbackServer(msg) => assert!(msg.contains("state mismatch")),
            _ => panic!("expected CallbackServer error"),
        }
    }

    #[test]
    fn test_extract_account_id_from_jwt() {
        // Valid JWT with accountId
        let payload = r#"{"https://api.openai.com/auth.chatgpt_account_id":"test-account-123"}"#;
        let payload_b64 = URL_SAFE_NO_PAD.encode(payload.as_bytes());
        let token = format!("header.{}.signature", payload_b64);

        let account_id = extract_account_id_from_jwt(&token);
        assert_eq!(account_id, Some("test-account-123".to_string()));
    }

    #[test]
    fn test_extract_account_id_missing_field() {
        // JWT without accountId field
        let payload = r#"{"sub":"user123"}"#;
        let payload_b64 = URL_SAFE_NO_PAD.encode(payload.as_bytes());
        let token = format!("header.{}.signature", payload_b64);

        let account_id = extract_account_id_from_jwt(&token);
        assert_eq!(account_id, None);
    }

    #[test]
    fn test_extract_account_id_invalid_jwt() {
        let account_id = extract_account_id_from_jwt("not.a.valid.jwt");
        assert_eq!(account_id, None);
    }

    #[test]
    fn test_generate_random_state() {
        let state1 = generate_random_state();
        let state2 = generate_random_state();
        assert_eq!(state1.len(), 32); // 16 bytes -> 32 hex chars
        assert_ne!(state1, state2);
    }
}
