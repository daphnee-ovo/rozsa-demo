//! Shared provider helpers for credentials, HTTP requests, usage, and errors.

use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::env_keys::get_env_api_key;
use crate::event_stream::EventStreamSender;
use crate::types::{
    Api, AssistantMessage, CacheRetention, Model, StopReason, StreamEvent, StreamOptions, Usage,
    UsageCost,
};

/// Error type returned by provider helper functions before stream events exist.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("No API key for provider: {provider}")]
    MissingApiKey { provider: String },
    #[error("Invalid provider URL `{url}`: {message}")]
    InvalidUrl { url: String, message: String },
    #[error("Provider HTTP error ({status}): {body}")]
    HttpStatus {
        status: reqwest::StatusCode,
        body: String,
    },
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Provider stream parse error: {0}")]
    Parse(String),
}

/// Convenient result alias for provider helper functions.
pub type ProviderResult<T> = Result<T, ProviderError>;

/// Return the current Unix timestamp in milliseconds for message metadata.
pub fn unix_timestamp_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_millis(0))
        .as_millis() as i64
}

/// Build a reqwest client with optional per-request timeout.
pub fn build_http_client(options: &StreamOptions) -> ProviderResult<reqwest::Client> {
    let mut builder = reqwest::Client::builder();
    if let Some(timeout_ms) = options.timeout_ms {
        builder = builder.timeout(Duration::from_millis(timeout_ms));
    }
    Ok(builder.build()?)
}

/// Resolve an API key from request options first, then known provider env vars.
pub fn resolve_api_key(model: &Model, options: &StreamOptions) -> ProviderResult<String> {
    if let Some(api_key) = options.api_key.as_ref().filter(|value| !value.is_empty()) {
        return Ok(api_key.clone());
    }
    get_env_api_key(&model.provider).ok_or_else(|| ProviderError::MissingApiKey {
        provider: provider_id(&model.provider),
    })
}

/// Merge model headers and request headers, with request headers taking priority.
pub fn merge_headers(
    model_headers: Option<&HashMap<String, String>>,
    option_headers: Option<&HashMap<String, String>>,
) -> HashMap<String, String> {
    let mut headers = HashMap::new();
    if let Some(model_headers) = model_headers {
        headers.extend(model_headers.clone());
    }
    if let Some(option_headers) = option_headers {
        headers.extend(option_headers.clone());
    }
    headers
}

/// Convert header map into a reqwest header map with clear validation errors.
pub fn to_header_map(
    headers: &HashMap<String, String>,
) -> ProviderResult<reqwest::header::HeaderMap> {
    let mut map = reqwest::header::HeaderMap::new();
    for (name, value) in headers {
        let header_name =
            reqwest::header::HeaderName::from_bytes(name.as_bytes()).map_err(|error| {
                ProviderError::Parse(format!("invalid header name `{name}`: {error}"))
            })?;
        let header_value = reqwest::header::HeaderValue::from_str(value).map_err(|error| {
            ProviderError::Parse(format!("invalid header value for `{name}`: {error}"))
        })?;
        map.insert(header_name, header_value);
    }
    Ok(map)
}

/// Resolve a provider base URL and append a path without duplicating slashes.
pub fn join_url(base_url: &str, path: &str) -> ProviderResult<String> {
    let trimmed = base_url.trim();
    if trimmed.is_empty() {
        return Err(ProviderError::InvalidUrl {
            url: base_url.to_string(),
            message: "base URL is empty".to_string(),
        });
    }
    let base = trimmed.trim_end_matches('/');
    let path = path.trim_start_matches('/');
    Ok(format!("{base}/{path}"))
}

/// Create the initial assistant message emitted by provider streams.
pub fn create_output(model: &Model, api: Api) -> AssistantMessage {
    AssistantMessage {
        content: Vec::new(),
        api,
        provider: model.provider.clone(),
        model: model.id.clone(),
        response_model: None,
        response_id: None,
        usage: empty_usage(),
        stop_reason: StopReason::Stop,
        error_message: None,
        timestamp: unix_timestamp_ms(),
    }
}

/// Return an empty usage record with zero cost.
pub fn empty_usage() -> Usage {
    Usage {
        input: 0,
        output: 0,
        cache_read: 0,
        cache_write: 0,
        total_tokens: 0,
        cost: UsageCost {
            input: 0.0,
            output: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
            total: 0.0,
        },
    }
}

/// Calculate usage cost in-place from model price metadata.
pub fn calculate_cost(model: &Model, usage: &mut Usage) {
    usage.cost.input = (model.cost.input / 1_000_000.0) * usage.input as f64;
    usage.cost.output = (model.cost.output / 1_000_000.0) * usage.output as f64;
    usage.cost.cache_read = (model.cost.cache_read / 1_000_000.0) * usage.cache_read as f64;
    usage.cost.cache_write = (model.cost.cache_write / 1_000_000.0) * usage.cache_write as f64;
    usage.cost.total =
        usage.cost.input + usage.cost.output + usage.cost.cache_read + usage.cost.cache_write;
}

/// Emit a normalized stream error and close the provider task naturally.
pub fn emit_error(
    sender: &EventStreamSender<StreamEvent>,
    mut output: AssistantMessage,
    error: impl ToString,
) {
    output.stop_reason = StopReason::Error;
    output.error_message = Some(error.to_string());
    sender.push(StreamEvent::Error {
        reason: StopReason::Error,
        error: output,
    });
}

/// Convert a provider finish reason into the normalized stop reason.
pub fn map_finish_reason(reason: Option<&str>) -> (StopReason, Option<String>) {
    match reason {
        None | Some("stop" | "end") => (StopReason::Stop, None),
        Some("length") => (StopReason::Length, None),
        Some("function_call" | "tool_calls") => (StopReason::ToolUse, None),
        Some("content_filter") => (
            StopReason::Error,
            Some("Provider finish_reason: content_filter".to_string()),
        ),
        Some("network_error") => (
            StopReason::Error,
            Some("Provider finish_reason: network_error".to_string()),
        ),
        Some(other) => (
            StopReason::Error,
            Some(format!("Provider finish_reason: {other}")),
        ),
    }
}

/// Return the provider ID used in error messages and env-key reporting.
pub fn provider_id(provider: &crate::types::Provider) -> String {
    match provider {
        crate::types::Provider::Anthropic => "anthropic".to_string(),
        crate::types::Provider::OpenAI => "openai".to_string(),
        crate::types::Provider::AmazonBedrock => "amazon-bedrock".to_string(),
        crate::types::Provider::Google => "google".to_string(),
        crate::types::Provider::GoogleVertex => "google-vertex".to_string(),
        crate::types::Provider::DeepSeek => "deepseek".to_string(),
        crate::types::Provider::OpenRouter => "openrouter".to_string(),
        crate::types::Provider::XAI => "xai".to_string(),
        crate::types::Provider::Groq => "groq".to_string(),
        crate::types::Provider::Cerebras => "cerebras".to_string(),
        crate::types::Provider::Mistral => "mistral".to_string(),
        crate::types::Provider::Nvidia => "nvidia".to_string(),
        crate::types::Provider::Zai => "zai".to_string(),
        crate::types::Provider::Together => "together".to_string(),
        crate::types::Provider::MoonshotAI => "moonshotai".to_string(),
        crate::types::Provider::MoonshotAICn => "moonshotai-cn".to_string(),
        crate::types::Provider::HuggingFace => "huggingface".to_string(),
        crate::types::Provider::CloudflareWorkersAI => "cloudflare-workers-ai".to_string(),
        crate::types::Provider::CloudflareAIGateway => "cloudflare-ai-gateway".to_string(),
        crate::types::Provider::Xiaomi => "xiaomi".to_string(),
        crate::types::Provider::XiaomiTokenPlanCn => "xiaomi-token-plan-cn".to_string(),
        crate::types::Provider::XiaomiTokenPlanAms => "xiaomi-token-plan-ams".to_string(),
        crate::types::Provider::XiaomiTokenPlanSgp => "xiaomi-token-plan-sgp".to_string(),
        crate::types::Provider::Custom(value) => value.clone(),
    }
}

/// Resolve the cache-retention default used by OpenAI-compatible providers.
pub fn resolve_cache_retention(options: &StreamOptions) -> CacheRetention {
    if options.cache_retention == CacheRetention::Long {
        CacheRetention::Long
    } else if std::env::var("ROZSA_CACHE_RETENTION").ok().as_deref() == Some("long") {
        CacheRetention::Long
    } else {
        options.cache_retention
    }
}
