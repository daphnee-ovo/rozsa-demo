//! Environment-variable credential lookup for built-in providers.
//!
//! Resolve provider API keys, including Bedrock and Vertex multi-source detection.

use crate::types::Provider;

/// Return the configured API key (or sentinel) for a provider.
///
/// For providers with multiple credential sources (AWS Bedrock, Google Vertex),
/// returns `Some("<authenticated>")` when any valid credential is detected.
pub fn get_env_api_key(provider: &Provider) -> Option<String> {
    match provider {
        Provider::Anthropic => env_first(&["ANTHROPIC_OAUTH_TOKEN", "ANTHROPIC_API_KEY"]),
        Provider::OpenAI => env("OPENAI_API_KEY"),
        Provider::DeepSeek => env("DEEPSEEK_API_KEY"),
        Provider::Google => env("GEMINI_API_KEY"),
        Provider::GoogleVertex => get_vertex_credential(),
        Provider::XAI => env("XAI_API_KEY"),
        Provider::Groq => env("GROQ_API_KEY"),
        Provider::Mistral => env("MISTRAL_API_KEY"),
        Provider::Nvidia => env("NVIDIA_API_KEY"),
        Provider::OpenRouter => env("OPENROUTER_API_KEY"),
        Provider::AmazonBedrock => get_bedrock_credential(),
        Provider::Cerebras => env("CEREBRAS_API_KEY"),
        Provider::Zai => env("ZAI_API_KEY"),
        Provider::Together => env("TOGETHER_API_KEY"),
        Provider::MoonshotAI | Provider::MoonshotAICn => env("MOONSHOT_API_KEY"),
        Provider::HuggingFace => env("HF_TOKEN"),
        Provider::CloudflareWorkersAI | Provider::CloudflareAIGateway => env("CLOUDFLARE_API_KEY"),
        Provider::Xiaomi => env("XIAOMI_API_KEY"),
        Provider::XiaomiTokenPlanCn => env("XIAOMI_TOKEN_PLAN_CN_API_KEY"),
        Provider::XiaomiTokenPlanAms => env("XIAOMI_TOKEN_PLAN_AMS_API_KEY"),
        Provider::XiaomiTokenPlanSgp => env("XIAOMI_TOKEN_PLAN_SGP_API_KEY"),
        Provider::Custom(_) => None,
    }
}

fn env(key: &str) -> Option<String> {
    crate::credentials::resolve_environment_variable(key)
        .ok()
        .flatten()
}

fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()
        .map(std::path::PathBuf::from)
}

fn env_first(keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| env(k))
}

/// Amazon Bedrock 多源凭证检测：
/// 1. AWS_PROFILE — named profile
/// 2. AWS_ACCESS_KEY_ID + AWS_SECRET_ACCESS_KEY — static IAM keys
/// 3. AWS_BEARER_TOKEN_BEDROCK — bearer token auth
/// 4. AWS_CONTAINER_CREDENTIALS_RELATIVE_URI — ECS task role
/// 5. AWS_CONTAINER_CREDENTIALS_FULL_URI — ECS task role (full)
/// 6. AWS_WEB_IDENTITY_TOKEN_FILE — IRSA / OIDC
/// 7. ~/.aws/credentials 默认 profile 存在
fn get_bedrock_credential() -> Option<String> {
    if env("AWS_PROFILE").is_some() {
        return Some("<authenticated>".to_string());
    }
    if env("AWS_ACCESS_KEY_ID").is_some() && env("AWS_SECRET_ACCESS_KEY").is_some() {
        return Some("<authenticated>".to_string());
    }
    if env("AWS_BEARER_TOKEN_BEDROCK").is_some() {
        return Some("<authenticated>".to_string());
    }
    if env("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI").is_some() {
        return Some("<authenticated>".to_string());
    }
    if env("AWS_CONTAINER_CREDENTIALS_FULL_URI").is_some() {
        return Some("<authenticated>".to_string());
    }
    if env("AWS_WEB_IDENTITY_TOKEN_FILE").is_some() {
        return Some("<authenticated>".to_string());
    }
    // 检查 ~/.aws/credentials 默认 profile
    if let Some(home) = home_dir() {
        let creds_file = home.join(".aws").join("credentials");
        if creds_file.exists() {
            return Some("<authenticated>".to_string());
        }
    }
    None
}

/// Google Vertex AI 多源凭证检测：
/// 1. GOOGLE_CLOUD_API_KEY — 直接 API key
/// 2. GOOGLE_APPLICATION_CREDENTIALS — service account JSON
/// 3. ~/.config/gcloud/application_default_credentials.json (ADC) + project + location
fn get_vertex_credential() -> Option<String> {
    if env("GOOGLE_CLOUD_API_KEY").is_some() {
        return Some("<authenticated>".to_string());
    }
    // Service account JSON
    if let Some(gac_path) = env("GOOGLE_APPLICATION_CREDENTIALS") {
        if std::path::Path::new(&gac_path).exists() {
            return Some("<authenticated>".to_string());
        }
    }
    // Application Default Credentials + project + location
    let has_adc = if let Some(home) = home_dir() {
        home.join(".config/gcloud/application_default_credentials.json")
            .exists()
    } else {
        false
    };
    let has_project = env("GOOGLE_CLOUD_PROJECT").is_some() || env("GCLOUD_PROJECT").is_some();
    let has_location = env("GOOGLE_CLOUD_LOCATION").is_some();
    if has_adc && has_project && has_location {
        return Some("<authenticated>".to_string());
    }
    None
}
