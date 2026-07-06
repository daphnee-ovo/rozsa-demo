// Rate limit query for ChatGPT subscription accounts.
//
// Internal Framework:
// rate_limit.rs
// ├── RateLimitSnapshot        — top-level result
// ├── RateLimitWindow          — single window (primary=5h, secondary=weekly)
// ├── RateLimitError           — error type
// └── fetch_rate_limits()      — HTTP GET to /wham/usage
//
// Reference:
// - codex-rs backend-client/src/client.rs:284-302

use serde::Deserialize;

/// Rate limit snapshot for a ChatGPT subscription account.
#[derive(Debug, Clone)]
pub struct RateLimitSnapshot {
    pub plan_type: Option<String>,
    pub allowed: bool,
    pub limit_reached: bool,
    /// Five-hour window usage.
    pub primary: Option<RateLimitWindow>,
    /// Weekly window usage.
    pub secondary: Option<RateLimitWindow>,
}

/// A single rate limit window (5-hour or weekly).
#[derive(Debug, Clone)]
pub struct RateLimitWindow {
    /// Percentage of quota used (0-100).
    pub used_percent: i32,
    /// Window duration in seconds.
    pub window_duration_secs: i32,
    /// Seconds until the window resets.
    pub reset_after_secs: i32,
    /// Unix timestamp when the window resets.
    pub reset_at: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum RateLimitError {
    #[error("HTTP request failed: {0}")]
    Network(#[from] reqwest::Error),
    #[error("HTTP {status}: {body}")]
    HttpStatus {
        status: reqwest::StatusCode,
        body: String,
    },
    #[error("Failed to parse response: {0}")]
    Parse(String),
    #[error("Missing credentials: {0}")]
    MissingCredentials(String),
}

/// Fetch rate limits from the ChatGPT backend API.
///
/// Uses the /wham/usage endpoint with the ChatGPT OAuth access token.
pub async fn fetch_rate_limits(
    access_token: &str,
    account_id: &str,
    base_url: Option<&str>,
) -> Result<RateLimitSnapshot, RateLimitError> {
    let base = base_url.unwrap_or("https://chatgpt.com/backend-api");
    let url = format!("{}/wham/usage", base.trim_end_matches('/'));

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {access_token}"))
        .header("ChatGPT-Account-ID", account_id)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(RateLimitError::HttpStatus { status, body });
    }

    let payload: RateLimitPayload = response
        .json()
        .await
        .map_err(|e| RateLimitError::Parse(e.to_string()))?;

    Ok(parse_payload(payload))
}

/// Fetch rate limits using credentials from auth.json.
pub async fn fetch_rate_limits_from_auth(
    auth_json_path: &str,
) -> Result<RateLimitSnapshot, RateLimitError> {
    use crate::credentials::{read_account_id, resolve_auth_json_api_key_pub};

    let access_token = resolve_auth_json_api_key_pub(auth_json_path, "codex-oauth")
        .await
        .map_err(RateLimitError::MissingCredentials)?
        .ok_or_else(|| {
            RateLimitError::MissingCredentials("No codex-oauth credential in auth.json".to_string())
        })?;

    let account_id = read_account_id(auth_json_path, "codex-oauth").ok_or_else(|| {
        RateLimitError::MissingCredentials("No accountId in codex-oauth credential".to_string())
    })?;

    fetch_rate_limits(&access_token, &account_id, None).await
}

// -- Internal deserialization types --

#[derive(Debug, Deserialize)]
struct RateLimitPayload {
    plan_type: Option<String>,
    rate_limit: Option<RateLimitDetails>,
}

#[derive(Debug, Deserialize)]
struct RateLimitDetails {
    allowed: Option<bool>,
    limit_reached: Option<bool>,
    primary_window: Option<RateLimitWindowRaw>,
    secondary_window: Option<RateLimitWindowRaw>,
}

#[derive(Debug, Deserialize)]
struct RateLimitWindowRaw {
    used_percent: Option<i32>,
    limit_window_seconds: Option<i32>,
    reset_after_seconds: Option<i32>,
    reset_at: Option<i64>,
}

fn parse_payload(payload: RateLimitPayload) -> RateLimitSnapshot {
    let (allowed, limit_reached, primary, secondary) = match payload.rate_limit {
        Some(details) => (
            details.allowed.unwrap_or(true),
            details.limit_reached.unwrap_or(false),
            details.primary_window.map(parse_window),
            details.secondary_window.map(parse_window),
        ),
        None => (true, false, None, None),
    };

    RateLimitSnapshot {
        plan_type: payload.plan_type,
        allowed,
        limit_reached,
        primary,
        secondary,
    }
}

fn parse_window(raw: RateLimitWindowRaw) -> RateLimitWindow {
    RateLimitWindow {
        used_percent: raw.used_percent.unwrap_or(0),
        window_duration_secs: raw.limit_window_seconds.unwrap_or(0),
        reset_after_secs: raw.reset_after_seconds.unwrap_or(0),
        reset_at: raw.reset_at.unwrap_or(0),
    }
}
