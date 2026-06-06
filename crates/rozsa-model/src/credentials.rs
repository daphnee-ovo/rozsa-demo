//! Request credential and header resolution for Rust-owned model execution.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::providers::common::provider_id;
use crate::types::{Model, SimpleStreamOptions};

/// Resolve `models.json` request auth and headers into stream options before provider execution.
pub async fn resolve_request_options(
    model: &Model,
    options: &SimpleStreamOptions,
    models_json_path: Option<&str>,
    auth_json_path: Option<&str>,
) -> Result<SimpleStreamOptions, String> {
    let mut resolved = options.clone();
    let provider_name = provider_id(&model.provider);

    if resolved
        .base
        .api_key
        .as_ref()
        .is_none_or(|value| value.is_empty())
    {
        if let Some(path) = auth_json_path {
            resolved.base.api_key = resolve_auth_json_api_key(path, &provider_name).await?;
        }
    }

    let Some(path) = models_json_path else {
        return Ok(resolved);
    };
    let input = match fs::read_to_string(path) {
        Ok(input) => input,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(resolved),
        Err(error) => return Err(format!("Failed to read models.json `{path}`: {error}")),
    };
    let config: ModelsConfig = serde_json::from_str(&strip_json_comments(&input))
        .map_err(|error| format!("Failed to parse models.json `{path}`: {error}"))?;
    let Some(provider) = config.providers.get(&provider_name) else {
        return Ok(resolved);
    };

    if resolved
        .base
        .api_key
        .as_ref()
        .is_none_or(|value| value.is_empty())
    {
        if let Some(api_key) = provider.api_key.as_ref() {
            resolved.base.api_key = Some(resolve_config_value_or_throw(
                api_key,
                &format!("API key for provider `{provider_name}`"),
            )?);
        }
    }

    if let Some(headers) = provider.headers.as_ref() {
        let mut merged = resolve_headers_or_throw(headers, &format!("provider `{provider_name}`"))?;
        if let Some(existing) = resolved.base.headers.take() {
            merged.extend(existing);
        }
        resolved.base.headers = (!merged.is_empty()).then_some(merged);
    }

    if provider.auth_header.unwrap_or(false) {
        let Some(api_key) = resolved
            .base
            .api_key
            .as_ref()
            .filter(|value| !value.is_empty())
        else {
            return Err(format!("No API key found for `{provider_name}`"));
        };
        let mut headers = resolved.base.headers.take().unwrap_or_default();
        headers.insert("Authorization".to_string(), format!("Bearer {api_key}"));
        resolved.base.headers = Some(headers);
    }

    Ok(resolved)
}

#[derive(Debug, Deserialize)]
struct ModelsConfig {
    providers: HashMap<String, ProviderConfig>,
}

#[derive(Debug, Deserialize)]
struct ProviderConfig {
    #[serde(rename = "apiKey")]
    api_key: Option<String>,
    headers: Option<HashMap<String, String>>,
    #[serde(rename = "authHeader")]
    auth_header: Option<bool>,
}

async fn resolve_auth_json_api_key(path: &str, provider: &str) -> Result<Option<String>, String> {
    let input = match fs::read_to_string(path) {
        Ok(input) => input,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Failed to read auth.json `{path}`: {error}")),
    };
    let mut auth: Map<String, Value> = serde_json::from_str(&input)
        .map_err(|error| format!("Failed to parse auth.json `{path}`: {error}"))?;
    let Some(credential) = auth.get(provider).cloned() else {
        return Ok(None);
    };

    let credential_type = credential
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Invalid auth.json credential for `{provider}`: missing type"))?;

    if credential_type == "api_key" {
        let key = credential
            .get("key")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("Invalid auth.json API key credential for `{provider}`"))?;
        return Ok(Some(resolve_config_value_or_throw(
            key,
            &format!("stored API key for provider `{provider}`"),
        )?));
    }

    if credential_type == "oauth" {
        let mut oauth = parse_oauth_credential(provider, credential)?;
        if now_ms() >= oauth.expires {
            oauth = refresh_oauth_credential(provider, &oauth).await?;
            auth.insert(provider.to_string(), oauth_to_value(&oauth));
            write_auth_json(path, &auth)?;
        }
        return Ok(Some(oauth.access));
    }

    Err(format!(
        "Invalid auth.json credential for `{provider}`: unsupported type `{credential_type}`"
    ))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_millis(0))
        .as_millis() as i64
}

#[derive(Debug, Clone)]
struct OAuthCredential {
    access: String,
    refresh: String,
    expires: i64,
    extra: Map<String, Value>,
}

#[derive(Debug, Deserialize)]
struct RefreshTokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: i64,
}

#[derive(Debug, Deserialize)]
struct CopilotTokenResponse {
    token: String,
    expires_at: i64,
}

fn parse_oauth_credential(provider: &str, value: Value) -> Result<OAuthCredential, String> {
    let mut object = value
        .as_object()
        .cloned()
        .ok_or_else(|| format!("Invalid auth.json OAuth credential for `{provider}`"))?;
    object.remove("type");
    let access = take_string(&mut object, "access", provider)?;
    let refresh = take_string(&mut object, "refresh", provider)?;
    let expires = object
        .remove("expires")
        .and_then(|value| value.as_i64())
        .ok_or_else(|| {
            format!("Invalid auth.json OAuth credential for `{provider}`: missing expires")
        })?;
    Ok(OAuthCredential {
        access,
        refresh,
        expires,
        extra: object,
    })
}

fn take_string(
    object: &mut Map<String, Value>,
    key: &str,
    provider: &str,
) -> Result<String, String> {
    object
        .remove(key)
        .and_then(|value| value.as_str().map(ToString::to_string))
        .ok_or_else(|| {
            format!("Invalid auth.json OAuth credential for `{provider}`: missing {key}")
        })
}

fn oauth_to_value(credential: &OAuthCredential) -> Value {
    let mut object = credential.extra.clone();
    object.insert("type".to_string(), Value::String("oauth".to_string()));
    object.insert(
        "access".to_string(),
        Value::String(credential.access.clone()),
    );
    object.insert(
        "refresh".to_string(),
        Value::String(credential.refresh.clone()),
    );
    object.insert("expires".to_string(), json!(credential.expires));
    Value::Object(object)
}

async fn refresh_oauth_credential(
    provider: &str,
    credential: &OAuthCredential,
) -> Result<OAuthCredential, String> {
    match provider {
        "anthropic" => refresh_anthropic_oauth(credential).await,
        "openai-codex" => refresh_openai_codex_oauth(credential).await,
        "github-copilot" => refresh_github_copilot_oauth(credential).await,
        _ => Err(format!(
            "OAuth credential for `{provider}` is expired and Rust refresh is not supported for this provider"
        )),
    }
}

async fn refresh_anthropic_oauth(credential: &OAuthCredential) -> Result<OAuthCredential, String> {
    let response = reqwest::Client::new()
        .post("https://platform.claude.com/v1/oauth/token")
        .json(&json!({
            "grant_type": "refresh_token",
            "client_id": "9d1c250a-e61b-44d9-88ed-5944d1962f5e",
            "refresh_token": credential.refresh,
        }))
        .send()
        .await
        .map_err(|error| format!("Anthropic token refresh request failed: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("Anthropic token refresh body read failed: {error}"))?;
    if !status.is_success() {
        return Err(format!("Anthropic token refresh failed ({status}): {body}"));
    }
    let parsed: RefreshTokenResponse = serde_json::from_str(&body).map_err(|error| {
        format!("Anthropic token refresh returned invalid JSON: {error}; body={body}")
    })?;
    Ok(OAuthCredential {
        access: parsed.access_token,
        refresh: parsed
            .refresh_token
            .unwrap_or_else(|| credential.refresh.clone()),
        expires: now_ms() + parsed.expires_in * 1000 - 5 * 60 * 1000,
        extra: credential.extra.clone(),
    })
}

async fn refresh_openai_codex_oauth(
    credential: &OAuthCredential,
) -> Result<OAuthCredential, String> {
    let response = reqwest::Client::new()
        .post("https://auth.openai.com/oauth/token")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", credential.refresh.as_str()),
            ("client_id", "app_EMoamEEZ73f0CkXaXp7hrann"),
        ])
        .send()
        .await
        .map_err(|error| format!("OpenAI Codex token refresh request failed: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("OpenAI Codex token refresh body read failed: {error}"))?;
    if !status.is_success() {
        return Err(format!(
            "OpenAI Codex token refresh failed ({status}): {body}"
        ));
    }
    let parsed: RefreshTokenResponse = serde_json::from_str(&body).map_err(|error| {
        format!("OpenAI Codex token refresh returned invalid JSON: {error}; body={body}")
    })?;
    Ok(OAuthCredential {
        access: parsed.access_token,
        refresh: parsed
            .refresh_token
            .unwrap_or_else(|| credential.refresh.clone()),
        expires: now_ms() + parsed.expires_in * 1000,
        extra: credential.extra.clone(),
    })
}

async fn refresh_github_copilot_oauth(
    credential: &OAuthCredential,
) -> Result<OAuthCredential, String> {
    let domain = credential
        .extra
        .get("enterpriseUrl")
        .and_then(Value::as_str)
        .map(normalize_domain)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "github.com".to_string());
    let response = reqwest::Client::new()
        .get(format!("https://api.{domain}/copilot_internal/v2/token"))
        .header("Accept", "application/json")
        .header("Authorization", format!("Bearer {}", credential.refresh))
        .header("User-Agent", "GitHubCopilotChat/0.35.0")
        .header("Editor-Version", "vscode/1.107.0")
        .header("Editor-Plugin-Version", "copilot-chat/0.35.0")
        .header("Copilot-Integration-Id", "vscode-chat")
        .send()
        .await
        .map_err(|error| format!("GitHub Copilot token refresh request failed: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("GitHub Copilot token refresh body read failed: {error}"))?;
    if !status.is_success() {
        return Err(format!(
            "GitHub Copilot token refresh failed ({status}): {body}"
        ));
    }
    let parsed: CopilotTokenResponse = serde_json::from_str(&body).map_err(|error| {
        format!("GitHub Copilot token refresh returned invalid JSON: {error}; body={body}")
    })?;
    Ok(OAuthCredential {
        access: parsed.token,
        refresh: credential.refresh.clone(),
        expires: parsed.expires_at * 1000 - 5 * 60 * 1000,
        extra: credential.extra.clone(),
    })
}

fn normalize_domain(raw: &str) -> String {
    let trimmed = raw.trim();
    let without_scheme = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .unwrap_or(trimmed);
    without_scheme
        .split('/')
        .next()
        .unwrap_or(without_scheme)
        .trim()
        .to_string()
}

fn auth_lock_path(path: &str) -> String {
    format!("{}.lock", Path::new(path).display())
}

fn acquire_auth_lock(path: &str) -> Result<Option<fs::File>, String> {
    let lock_path = auth_lock_path(path);
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
    {
        Ok(file) => Ok(Some(file)),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(None),
        Err(error) => Err(format!(
            "Failed to create auth.json lock `{lock_path}`: {error}"
        )),
    }
}

fn release_auth_lock(path: &str) -> Result<(), String> {
    let lock_path = auth_lock_path(path);
    match fs::remove_file(&lock_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "Failed to release auth.json lock `{lock_path}`: {error}"
        )),
    }
}

fn write_auth_json(path: &str, auth: &Map<String, Value>) -> Result<(), String> {
    let _lock = acquire_auth_lock(path)?
        .ok_or_else(|| format!("auth.json `{path}` is locked by another process"))?;
    let serialized = serde_json::to_string_pretty(auth)
        .map_err(|error| format!("Failed to serialize auth.json `{path}`: {error}"))?;
    let result = fs::write(path, serialized)
        .map_err(|error| format!("Failed to write auth.json `{path}`: {error}"));
    let release = release_auth_lock(path);
    result.and(release)
}

fn resolve_headers_or_throw(
    headers: &HashMap<String, String>,
    description: &str,
) -> Result<HashMap<String, String>, String> {
    let mut resolved = HashMap::new();
    for (name, value) in headers {
        resolved.insert(
            name.clone(),
            resolve_config_value_or_throw(value, &format!("{description} header `{name}`"))?,
        );
    }
    Ok(resolved)
}

fn resolve_config_value_or_throw(config: &str, description: &str) -> Result<String, String> {
    if let Some(value) = resolve_config_value(config) {
        return Ok(value);
    }
    if let Some(command) = config.strip_prefix('!') {
        return Err(format!(
            "Failed to resolve {description} from shell command: {command}"
        ));
    }
    Err(format!("Failed to resolve {description}"))
}

fn resolve_config_value(config: &str) -> Option<String> {
    if let Some(command) = config.strip_prefix('!') {
        return execute_command(command);
    }
    std::env::var(config)
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| Some(config.to_string()))
}

fn execute_command(command: &str) -> Option<String> {
    let output = if cfg!(windows) {
        Command::new("cmd").args(["/C", command]).output().ok()?
    } else {
        Command::new("sh").args(["-c", command]).output().ok()?
    };
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn strip_json_comments(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        if in_string {
            output.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        if ch == '"' {
            in_string = true;
            output.push(ch);
            continue;
        }

        if ch == '/' && chars.peek() == Some(&'/') {
            chars.next();
            for comment_ch in chars.by_ref() {
                if comment_ch == '\n' {
                    output.push('\n');
                    break;
                }
            }
            continue;
        }

        output.push(ch);
    }

    strip_trailing_commas(&output)
}

fn strip_trailing_commas(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        if in_string {
            output.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        if ch == '"' {
            in_string = true;
            output.push(ch);
            continue;
        }

        if ch == ',' {
            let mut lookahead = chars.clone();
            while matches!(lookahead.peek(), Some(next) if next.is_whitespace()) {
                lookahead.next();
            }
            if matches!(lookahead.peek(), Some('}' | ']')) {
                continue;
            }
        }

        output.push(ch);
    }

    output
}
