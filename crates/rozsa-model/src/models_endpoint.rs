// FrameworkTree
// models_endpoint.rs
// ├── refresh_models_if_needed()
// ├── is_cache_fresh()
// ├── fetch_remote_models()
// ├── convert_to_registry_format()
// ├── write_config()
// ├── enum ModelRefreshError
// ├── struct ModelsPayload
// ├── struct RemoteModel
// ├── mod tests
// ├── remote_model()
// └── convert_to_registry_format_keeps_only_visible_api_models()

// Auto-refresh model list for codex-oauth from the ChatGPT backend API.
//
// Internal Framework:
// models_endpoint.rs
// ├── CachedModelConfig          — on-disk format with last_refreshed timestamp
// ├── refresh_models_if_needed() — check cache age, fetch if stale
// ├── fetch_remote_models()      — HTTP GET to /wham/models
// └── convert_to_registry_format() — remote response → models.json format
//
// Reference:
// - codex-rs model-provider/src/models_endpoint.rs
// - codex-rs models-manager/models.json (bundled format)

use serde::Deserialize;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const CACHE_DURATION_SECS: u64 = 24 * 60 * 60; // 24 hours
const MODELS_ENDPOINT: &str = "/wham/models";
const DEFAULT_BASE_URL: &str = "https://chatgpt.com/backend-api";
const CODEX_MODELS_CLIENT_VERSION: &str = "0.146.0";

/// Check if the cached model config needs refresh and update if so.
/// Returns Ok(true) if refreshed, Ok(false) if cache is fresh.
pub async fn refresh_models_if_needed(
    config_path: &Path,
    access_token: &str,
    account_id: &str,
    force: bool,
) -> Result<bool, ModelRefreshError> {
    if !force && is_cache_fresh(config_path) {
        return Ok(false);
    }

    let models = fetch_remote_models(access_token, account_id).await?;
    let config = convert_to_registry_format(&models);
    write_config(config_path, &config)?;
    Ok(true)
}

/// Check if the config file cache is still within the 24h window.
fn is_cache_fresh(path: &Path) -> bool {
    let Ok(content) = std::fs::read_to_string(path) else {
        return false;
    };
    let Ok(value): Result<serde_json::Value, _> = serde_json::from_str(&content) else {
        return false;
    };
    let Some(timestamp) = value.get("_last_refreshed").and_then(|v| v.as_u64()) else {
        return false;
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    now.saturating_sub(timestamp) < CACHE_DURATION_SECS
}

/// Fetch models from the ChatGPT backend /wham/models endpoint.
async fn fetch_remote_models(
    access_token: &str,
    account_id: &str,
) -> Result<Vec<RemoteModel>, ModelRefreshError> {
    let url = format!("{}{}", DEFAULT_BASE_URL, MODELS_ENDPOINT);
    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .query(&[("client_version", CODEX_MODELS_CLIENT_VERSION)])
        .header("Authorization", format!("Bearer {access_token}"))
        .header("ChatGPT-Account-ID", account_id)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(ModelRefreshError::Network)?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(ModelRefreshError::HttpStatus { status, body });
    }

    let payload: ModelsPayload = response
        .json()
        .await
        .map_err(|e| ModelRefreshError::Parse(e.to_string()))?;

    Ok(payload.models)
}

/// Convert remote models to the registry JSON format.
fn convert_to_registry_format(models: &[RemoteModel]) -> serde_json::Value {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let model_entries: Vec<serde_json::Value> = models
        .iter()
        .filter(|m| {
            m.supported_in_api.unwrap_or(false)
                && m.visibility.as_deref().unwrap_or("list") == "list"
        })
        .map(|m| {
            let reasoning = m.default_reasoning_level.is_some();
            let input: Vec<&str> = m.input_modalities.iter().map(|s| s.as_str()).collect();
            serde_json::json!({
                "id": m.slug,
                "name": m.display_name,
                "contextWindow": m.context_window.unwrap_or(128000),
                "maxTokens": m.context_window.unwrap_or(128000) / 2,
                "reasoning": reasoning,
                "input": input,
                "cost": { "input": 0.0, "output": 0.0, "cacheRead": 0.0, "cacheWrite": 0.0 }
            })
        })
        .collect();

    serde_json::json!({
        "_last_refreshed": now,
        "providers": {
            "codex-oauth": {
                "baseUrl": "https://chatgpt.com/backend-api/codex",
                "api": "openai-responses",
                "authHeader": true,
                "models": model_entries
            }
        }
    })
}

fn write_config(path: &Path, config: &serde_json::Value) -> Result<(), ModelRefreshError> {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let content = serde_json::to_string_pretty(config)
        .map_err(|e| ModelRefreshError::Parse(e.to_string()))?;
    std::fs::write(path, content).map_err(|e| ModelRefreshError::Io(e.to_string()))?;
    Ok(())
}

// -- Types --

#[derive(Debug, thiserror::Error)]
pub enum ModelRefreshError {
    #[error("network error: {0}")]
    Network(reqwest::Error),
    #[error("HTTP {status}: {body}")]
    HttpStatus {
        status: reqwest::StatusCode,
        body: String,
    },
    #[error("parse error: {0}")]
    Parse(String),
    #[error("IO error: {0}")]
    Io(String),
}

#[derive(Debug, Deserialize)]
struct ModelsPayload {
    models: Vec<RemoteModel>,
}

#[derive(Debug, Deserialize)]
struct RemoteModel {
    slug: String,
    display_name: String,
    context_window: Option<i64>,
    supported_in_api: Option<bool>,
    visibility: Option<String>,
    default_reasoning_level: Option<String>,
    #[serde(default)]
    input_modalities: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remote_model(slug: &str, visibility: Option<&str>, supported_in_api: bool) -> RemoteModel {
        RemoteModel {
            slug: slug.to_string(),
            display_name: slug.to_string(),
            context_window: Some(272_000),
            supported_in_api: Some(supported_in_api),
            visibility: visibility.map(str::to_string),
            default_reasoning_level: Some("medium".to_string()),
            input_modalities: vec!["text".to_string(), "image".to_string()],
        }
    }

    #[test]
    fn convert_to_registry_format_keeps_only_visible_api_models() {
        let config = convert_to_registry_format(&[
            remote_model("gpt-5.5", Some("list"), true),
            remote_model("codex-auto-review", Some("hide"), true),
            remote_model("internal-disabled", Some("list"), false),
            remote_model("unclassified-visible", None, true),
        ]);

        let models = config
            .get("providers")
            .and_then(|providers| providers.get("codex-oauth"))
            .and_then(|provider| provider.get("models"))
            .and_then(|models| models.as_array())
            .expect("models should be present");
        let base_url = config
            .get("providers")
            .and_then(|providers| providers.get("codex-oauth"))
            .and_then(|provider| provider.get("baseUrl"))
            .and_then(|base_url| base_url.as_str());
        let ids: Vec<&str> = models
            .iter()
            .filter_map(|model| model.get("id").and_then(|id| id.as_str()))
            .collect();

        assert_eq!(base_url, Some("https://chatgpt.com/backend-api/codex"));
        assert_eq!(ids, vec!["gpt-5.5", "unclassified-visible"]);
    }
}
