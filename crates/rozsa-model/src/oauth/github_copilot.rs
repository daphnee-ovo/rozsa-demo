use serde_json::Value;
use std::collections::HashMap;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::device_code::{self, DeviceCodePollResult};
use super::types::{OAuthCredentials, OAuthFlowEvent, OAuthLoginError};

const CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";
const DEFAULT_DOMAIN: &str = "github.com";
const USER_AGENT: &str = "rozsa/1.0";

const KNOWN_MODELS: &[&str] = &[
    "claude-3.5-sonnet",
    "claude-sonnet-4",
    "claude-3.7-sonnet",
    "o4-mini",
    "o3",
    "o3-mini",
    "gpt-4o",
    "gpt-4.1",
    "gemini-2.5-pro",
];

/// Execute the GitHub Copilot OAuth login flow (Device Code).
pub async fn login(
    event_tx: mpsc::UnboundedSender<OAuthFlowEvent>,
    mut response_rx: mpsc::UnboundedReceiver<Value>,
    cancel: CancellationToken,
) -> Result<OAuthCredentials, OAuthLoginError> {
    // 1. Prompt user for GitHub Enterprise domain (default: github.com)
    event_tx
        .send(OAuthFlowEvent::Prompt {
            message: "Enter GitHub domain (leave empty for github.com):".to_string(),
            placeholder: Some("github.com".to_string()),
        })
        .ok();

    // 2. Wait for user response
    let domain_response = response_rx.recv().await.ok_or(OAuthLoginError::Cancelled)?;
    let domain_input = domain_response
        .get("value")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let domain = if domain_input.is_empty() {
        DEFAULT_DOMAIN.to_string()
    } else {
        normalize_domain(&domain_input)
    };

    // 3. Request device code
    let device_code_response = request_device_code(&domain).await?;

    // 4. Send device code event to TS (show user_code + verification_uri)
    event_tx
        .send(OAuthFlowEvent::DeviceCode {
            user_code: device_code_response.user_code.clone(),
            verification_uri: device_code_response.verification_uri.clone(),
        })
        .ok();

    // 5. Send waiting event
    event_tx
        .send(OAuthFlowEvent::Waiting {
            message: "Waiting for authorization...".to_string(),
        })
        .ok();

    // 6. Poll for access token using device_code module
    let interval = device_code_response.interval.unwrap_or(5);
    let expires_in = device_code_response.expires_in;
    let device_code = device_code_response.device_code.clone();
    let poll_domain = domain.clone();

    let github_access_token = device_code::poll_device_code(
        interval,
        expires_in,
        || poll_github_token(&poll_domain, &device_code),
        cancel.clone(),
    )
    .await?;

    // 7. Exchange GitHub access token for Copilot token
    event_tx
        .send(OAuthFlowEvent::Progress {
            message: "Exchanging token for Copilot access...".to_string(),
        })
        .ok();

    let copilot_token = get_copilot_token(&domain, &github_access_token).await?;

    // 8. Enable models (best effort, don't fail on error)
    let _ = enable_known_models(&domain, &github_access_token).await;

    // 9. Build credentials
    let enterprise_url = if domain != DEFAULT_DOMAIN {
        Some(domain.clone())
    } else {
        None
    };

    let mut extra = HashMap::new();
    if let Some(url) = enterprise_url {
        extra.insert("enterpriseUrl".to_string(), serde_json::json!(url));
    }

    Ok(OAuthCredentials {
        access: copilot_token.token,
        refresh: github_access_token,
        expires: copilot_token.expires_at * 1000 - 5 * 60 * 1000,
        extra,
    })
}

#[derive(serde::Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    interval: Option<u64>,
    expires_in: u64,
}

async fn request_device_code(domain: &str) -> Result<DeviceCodeResponse, OAuthLoginError> {
    let url = format!("https://{domain}/login/device/code");
    let client = reqwest::Client::new();

    // CLIENT_ID and scope contain only safe URL characters, no encoding needed
    let body = format!("client_id={CLIENT_ID}&scope=read:user");

    let response = client
        .post(&url)
        .header("Accept", "application/json")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .map_err(|e| OAuthLoginError::Network(e.to_string()))?;

    if !response.status().is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(OAuthLoginError::Provider(format!(
            "device code request failed: {body}"
        )));
    }

    response
        .json()
        .await
        .map_err(|e| OAuthLoginError::Provider(e.to_string()))
}

async fn poll_github_token(domain: &str, device_code: &str) -> DeviceCodePollResult {
    let url = format!("https://{domain}/login/oauth/access_token");
    let client = reqwest::Client::new();

    // device_code is base64-like from GitHub, CLIENT_ID is safe, grant_type uses : which is allowed in form values
    let body = format!(
        "client_id={CLIENT_ID}&device_code={device_code}&grant_type=urn:ietf:params:oauth:grant-type:device_code"
    );

    let response = client
        .post(&url)
        .header("Accept", "application/json")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await;

    let Ok(resp) = response else {
        return DeviceCodePollResult::Failed {
            message: "network error".to_string(),
        };
    };

    let Ok(body) = resp.json::<Value>().await else {
        return DeviceCodePollResult::Failed {
            message: "invalid response".to_string(),
        };
    };

    if let Some(error) = body.get("error").and_then(Value::as_str) {
        return match error {
            "authorization_pending" => DeviceCodePollResult::Pending,
            "slow_down" => DeviceCodePollResult::SlowDown,
            _ => DeviceCodePollResult::Failed {
                message: body
                    .get("error_description")
                    .and_then(Value::as_str)
                    .unwrap_or(error)
                    .to_string(),
            },
        };
    }

    if let Some(token) = body.get("access_token").and_then(Value::as_str) {
        DeviceCodePollResult::Complete {
            access_token: token.to_string(),
        }
    } else {
        DeviceCodePollResult::Failed {
            message: "no access_token in response".to_string(),
        }
    }
}

#[derive(serde::Deserialize)]
struct CopilotTokenResponse {
    token: String,
    expires_at: i64, // Unix timestamp in seconds
}

async fn get_copilot_token(
    domain: &str,
    access_token: &str,
) -> Result<CopilotTokenResponse, OAuthLoginError> {
    let api_domain = if domain == DEFAULT_DOMAIN {
        "api.github.com".to_string()
    } else {
        format!("api.{domain}")
    };
    let url = format!("https://{api_domain}/copilot_internal/v2/token");
    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {access_token}"))
        .header("User-Agent", USER_AGENT)
        .send()
        .await
        .map_err(|e| OAuthLoginError::Network(e.to_string()))?;

    if !response.status().is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(OAuthLoginError::TokenExchange(format!(
            "copilot token exchange failed: {body}"
        )));
    }

    response
        .json()
        .await
        .map_err(|e| OAuthLoginError::TokenExchange(e.to_string()))
}

async fn enable_known_models(domain: &str, access_token: &str) -> Result<(), OAuthLoginError> {
    let api_domain = if domain == DEFAULT_DOMAIN {
        "api.github.com".to_string()
    } else {
        format!("api.{domain}")
    };
    let client = reqwest::Client::new();

    for model in KNOWN_MODELS {
        let url = format!("https://{api_domain}/models/{model}/policy");
        let _ = client
            .post(&url)
            .header("Authorization", format!("Bearer {access_token}"))
            .header("User-Agent", USER_AGENT)
            .json(&serde_json::json!({ "state": "enabled" }))
            .send()
            .await;
    }

    Ok(())
}

fn normalize_domain(input: &str) -> String {
    let domain = input
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/');
    domain.to_string()
}
