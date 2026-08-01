//! Request credential and header resolution for Rust-owned model execution.

use std::collections::HashMap;
use std::fs;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::providers::common::provider_id;
use crate::types::{Model, SimpleStreamOptions};

const ROZSA_CONFIG_DIR_ENV: &str = "ROZSA_CONFIG_DIR";
const PRIVATE_ENV_FILE_NAME: &str = ".env";

static PRIVATE_ENV_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Return the private environment file used by Rózsa.
pub fn private_env_path() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os(ROZSA_CONFIG_DIR_ENV) {
        if path.is_empty() {
            return Err(format!("{ROZSA_CONFIG_DIR_ENV} must not be empty"));
        }
        return Ok(PathBuf::from(path).join(PRIVATE_ENV_FILE_NAME));
    }

    home_dir()
        .map(|home| home.join(".rozsa").join(PRIVATE_ENV_FILE_NAME))
        .ok_or_else(|| "Cannot determine the Rózsa home directory for ~/.rozsa/.env".to_string())
}

/// Validate a value used as an API key or header configuration value.
///
/// Values beginning with `$` are environment-variable references. Values
/// beginning with `!` are rejected rather than interpreted as commands.
pub fn validate_config_value(config: &str, description: &str) -> Result<(), String> {
    if config.is_empty() {
        return Err(format!("{description} must not be empty"));
    }
    if config.starts_with('!') {
        return Err(format!(
            "Shell command credential references are disabled for {description}; use `$NAME` or a literal value"
        ));
    }
    if let Some(name) = config.strip_prefix('$') {
        validate_environment_name(name, description)?;
    }
    Ok(())
}

/// Resolve a model configuration value using the process environment and the
/// private Rózsa environment file.
pub fn resolve_config_value(config: &str) -> Result<String, String> {
    validate_config_value(config, "configuration value")?;
    if !config.starts_with('$') {
        return Ok(config.to_string());
    }

    let env_path = private_env_path()?;
    resolve_config_value_from_env_file(config, &env_path)
}

/// Resolve a model configuration value against an explicit private env file.
/// This is also useful for isolated callers and tests.
pub fn resolve_config_value_from_env_file(config: &str, env_path: &Path) -> Result<String, String> {
    validate_config_value(config, "configuration value")?;
    let Some(name) = config.strip_prefix('$') else {
        return Ok(config.to_string());
    };

    resolve_environment_variable_from_env_file(name, env_path)?.ok_or_else(|| {
        format!(
            "Environment variable `{name}` is not set in the process environment or `{}`",
            env_path.display()
        )
    })
}

/// Resolve an environment variable without exporting private values to the
/// process environment. The process environment takes precedence so callers
/// can explicitly override a private value.
pub fn resolve_environment_variable(name: &str) -> Result<Option<String>, String> {
    validate_environment_name(name, "environment variable")?;
    if let Some(value) = non_empty_process_env(name) {
        return Ok(Some(value));
    }

    let env_path = private_env_path()?;
    resolve_environment_variable_from_env_file(name, &env_path)
}

/// Ensure a private environment variable exists in an explicit env file.
/// Existing values are never overwritten silently.
pub fn ensure_private_env_value(name: &str, value: &str) -> Result<(), String> {
    let env_path = private_env_path()?;
    ensure_private_env_value_at(&env_path, name, value)
}

/// Ensure a private environment variable exists in an explicit env file.
/// Existing values are never overwritten silently.
pub fn ensure_private_env_value_at(env_path: &Path, name: &str, value: &str) -> Result<(), String> {
    validate_environment_name(name, "private environment variable")?;
    if value.is_empty() {
        return Err(format!(
            "private environment variable `{name}` must not be empty"
        ));
    }

    let _lock = PRIVATE_ENV_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "private environment file lock is poisoned".to_string())?;
    let (existing, existed) = match fs::read_to_string(env_path) {
        Ok(content) => (content, true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (String::new(), false),
        Err(error) => {
            return Err(format!(
                "Failed to read private environment file `{}`: {error}",
                env_path.display()
            ));
        }
    };
    if existed {
        restrict_private_env_permissions(env_path)?;
    }
    let values = parse_private_env(&existing, env_path)?;
    if let Some(current) = values.get(name) {
        if current == value {
            return Ok(());
        }
        return Err(format!(
            "Private environment variable `{name}` already has a different value in `{}`",
            env_path.display()
        ));
    }

    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(name);
    updated.push('=');
    updated.push_str(
        &serde_json::to_string(value)
            .map_err(|error| format!("Failed to encode private environment value: {error}"))?,
    );
    updated.push('\n');
    write_private_env_atomically(env_path, &updated)
}

fn resolve_environment_variable_from_env_file(
    name: &str,
    env_path: &Path,
) -> Result<Option<String>, String> {
    validate_environment_name(name, "environment variable")?;
    if let Some(value) = non_empty_process_env(name) {
        return Ok(Some(value));
    }

    let values = match fs::read_to_string(env_path) {
        Ok(content) => parse_private_env(&content, env_path)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => HashMap::new(),
        Err(error) => {
            return Err(format!(
                "Failed to read private environment file `{}`: {error}",
                env_path.display()
            ));
        }
    };
    Ok(values.get(name).filter(|value| !value.is_empty()).cloned())
}

fn parse_private_env(content: &str, path: &Path) -> Result<HashMap<String, String>, String> {
    let mut values = HashMap::new();
    for (line_index, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let assignment = trimmed.strip_prefix("export ").unwrap_or(trimmed);
        let Some((name, raw_value)) = assignment.split_once('=') else {
            return Err(format!(
                "Invalid private environment entry at {}:{}: expected NAME=VALUE",
                path.display(),
                line_index + 1
            ));
        };
        let name = name.trim();
        validate_environment_name(
            name,
            &format!(
                "private environment entry at {}:{}",
                path.display(),
                line_index + 1
            ),
        )?;
        values.insert(
            name.to_string(),
            parse_private_env_value(raw_value.trim(), path, line_index + 1)?,
        );
    }
    Ok(values)
}

fn parse_private_env_value(raw: &str, path: &Path, line: usize) -> Result<String, String> {
    if raw.starts_with('"') {
        return serde_json::from_str(raw).map_err(|error| {
            format!(
                "Invalid quoted private environment value at {}:{line}: {error}",
                path.display()
            )
        });
    }
    if raw.starts_with('\'') {
        if raw.len() >= 2 && raw.ends_with('\'') {
            return Ok(raw[1..raw.len() - 1].to_string());
        }
        return Err(format!(
            "Invalid quoted private environment value at {}:{line}",
            path.display()
        ));
    }
    Ok(raw.to_string())
}

fn write_private_env_atomically(path: &Path, content: &str) -> Result<(), String> {
    let parent = path.parent().ok_or_else(|| {
        format!(
            "Cannot determine parent directory for private environment file `{}`",
            path.display()
        )
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "Failed to create private environment directory `{}`: {error}",
            parent.display()
        )
    })?;

    let temporary = path.with_file_name(format!(
        ".{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("rozsa-env")
    ));
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary).map_err(|error| {
        format!(
            "Failed to open temporary private environment file `{}`: {error}",
            temporary.display()
        )
    })?;
    std::io::Write::write_all(&mut file, content.as_bytes()).map_err(|error| {
        format!(
            "Failed to write temporary private environment file `{}`: {error}",
            temporary.display()
        )
    })?;
    file.sync_all().map_err(|error| {
        format!(
            "Failed to flush temporary private environment file `{}`: {error}",
            temporary.display()
        )
    })?;
    #[cfg(unix)]
    std::fs::set_permissions(
        &temporary,
        std::os::unix::fs::PermissionsExt::from_mode(0o600),
    )
    .map_err(|error| {
        format!(
            "Failed to secure temporary private environment file `{}`: {error}",
            temporary.display()
        )
    })?;
    fs::rename(&temporary, path).map_err(|error| {
        format!(
            "Failed to replace private environment file `{}`: {error}",
            path.display()
        )
    })
}

fn restrict_private_env_permissions(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        std::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o600))
            .map_err(|error| {
                format!(
                    "Failed to secure private environment file `{}`: {error}",
                    path.display()
                )
            })?;
    }
    Ok(())
}

fn validate_environment_name(name: &str, description: &str) -> Result<(), String> {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return Err(format!(
            "Invalid {description}: `$` must be followed by an environment variable name"
        ));
    };
    if !(first == '_' || first.is_ascii_alphabetic())
        || !chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
    {
        return Err(format!(
            "Invalid {description}: environment variable names must match [A-Za-z_][A-Za-z0-9_]*"
        ));
    }
    Ok(())
}

fn non_empty_process_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

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

/// Public wrapper for resolving API key from auth.json.
pub async fn resolve_auth_json_api_key_pub(
    path: &str,
    provider: &str,
) -> Result<Option<String>, String> {
    resolve_auth_json_api_key(path, provider).await
}

/// Read the ChatGPT account ID from an OAuth credential in auth.json.
pub fn read_account_id(path: &str, provider: &str) -> Option<String> {
    let input = std::fs::read_to_string(path).ok()?;
    let auth: Map<String, Value> = serde_json::from_str(&input).ok()?;
    let credential = auth.get(provider)?;
    read_account_id_from_credential(credential)
}

fn read_account_id_from_credential(credential: &Value) -> Option<String> {
    credential
        .get("accountId")
        .or_else(|| credential.get("account_id"))
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| {
            credential
                .get("idToken")
                .or_else(|| credential.get("id_token"))
                .and_then(|v| v.as_str())
                .and_then(crate::oauth::openai_codex::extract_account_id_from_jwt)
        })
        .or_else(|| {
            credential
                .get("access")
                .and_then(|v| v.as_str())
                .and_then(crate::oauth::openai_codex::extract_account_id_from_jwt)
        })
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
    id_token: Option<String>,
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
        "codex-oauth" => refresh_openai_codex_oauth(credential).await,
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
    let mut extra = credential.extra.clone();
    if let Some(id_token) = parsed.id_token.as_ref() {
        extra.insert("idToken".to_string(), Value::String(id_token.clone()));
        if let Some(account_id) = crate::oauth::openai_codex::extract_account_id_from_jwt(id_token)
        {
            extra.insert("accountId".to_string(), Value::String(account_id));
        }
    }
    Ok(OAuthCredential {
        access: parsed.access_token,
        refresh: parsed
            .refresh_token
            .unwrap_or_else(|| credential.refresh.clone()),
        expires: now_ms() + parsed.expires_in * 1000,
        extra,
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

/// Store OAuth credentials for a provider into auth.json (creating the file if needed).
pub fn store_oauth_credentials(
    path: &str,
    provider: &str,
    credentials: &crate::oauth::types::OAuthCredentials,
) -> Result<(), String> {
    let mut auth: Map<String, Value> = match fs::read_to_string(path) {
        Ok(input) => {
            let cleaned = strip_json_comments(&input);
            serde_json::from_str(&cleaned)
                .map_err(|e| format!("Failed to parse auth.json `{path}`: {e}"))?
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if let Some(parent) = Path::new(path).parent() {
                let _ = fs::create_dir_all(parent);
            }
            Map::new()
        }
        Err(e) => return Err(format!("Failed to read auth.json `{path}`: {e}")),
    };

    let mut value = serde_json::Map::new();
    value.insert("type".to_string(), json!("oauth"));
    value.insert("access".to_string(), json!(credentials.access));
    value.insert("refresh".to_string(), json!(credentials.refresh));
    value.insert("expires".to_string(), json!(credentials.expires));
    for (key, val) in &credentials.extra {
        value.insert(key.clone(), val.clone());
    }

    auth.insert(provider.to_string(), Value::Object(value));
    write_auth_json(path, &auth)
}

/// Remove stored credentials for a provider from auth.json.
pub fn remove_stored_credentials(path: &str, provider: &str) -> Result<bool, String> {
    let mut auth: Map<String, Value> = match fs::read_to_string(path) {
        Ok(input) => {
            let cleaned = strip_json_comments(&input);
            serde_json::from_str(&cleaned)
                .map_err(|e| format!("Failed to parse auth.json `{path}`: {e}"))?
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(format!("Failed to read auth.json `{path}`: {e}")),
    };

    let removed = auth.remove(provider).is_some();
    if removed {
        write_auth_json(path, &auth)?;
    }
    Ok(removed)
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
    validate_config_value(config, description)?;
    if let Some(name) = config.strip_prefix('$') {
        return resolve_environment_variable(name)
            .map_err(|error| format!("Failed to resolve {description}: {error}"))?
            .ok_or_else(|| format!("Failed to resolve {description}"));
    }
    Ok(config.to_string())
}

pub(crate) fn strip_json_comments(input: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::read_account_id_from_credential;
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use serde_json::json;

    fn jwt_with_payload(payload: &str) -> String {
        let payload_b64 = URL_SAFE_NO_PAD.encode(payload.as_bytes());
        format!("header.{}.signature", payload_b64)
    }

    #[test]
    fn read_account_id_prefers_stored_camel_case_value() {
        let credential = json!({
            "type": "oauth",
            "accountId": "stored-account",
            "idToken": jwt_with_payload(
                r#"{"https://api.openai.com/auth":{"chatgpt_account_id":"token-account"}}"#
            )
        });

        assert_eq!(
            read_account_id_from_credential(&credential),
            Some("stored-account".to_string())
        );
    }

    #[test]
    fn read_account_id_accepts_stored_snake_case_value() {
        let credential = json!({
            "type": "oauth",
            "account_id": "stored-account"
        });

        assert_eq!(
            read_account_id_from_credential(&credential),
            Some("stored-account".to_string())
        );
    }

    #[test]
    fn read_account_id_derives_from_id_token() {
        let credential = json!({
            "type": "oauth",
            "idToken": jwt_with_payload(
                r#"{"https://api.openai.com/auth":{"chatgpt_account_id":"token-account"}}"#
            )
        });

        assert_eq!(
            read_account_id_from_credential(&credential),
            Some("token-account".to_string())
        );
    }
}
