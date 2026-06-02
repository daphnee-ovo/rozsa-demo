//! Environment-variable credential lookup for built-in providers.

use crate::types::Provider;

/// Return the configured API key for a provider when it has a known env var.
pub fn get_env_api_key(provider: &Provider) -> Option<String> {
    let env_var = match provider {
        Provider::Anthropic => "ANTHROPIC_API_KEY",
        Provider::OpenAI => "OPENAI_API_KEY",
        Provider::DeepSeek => "DEEPSEEK_API_KEY",
        Provider::Google => "GEMINI_API_KEY",
        Provider::GoogleVertex => "GOOGLE_CLOUD_API_KEY",
        Provider::XAI => "XAI_API_KEY",
        Provider::Groq => "GROQ_API_KEY",
        Provider::Mistral => "MISTRAL_API_KEY",
        Provider::OpenRouter => "OPENROUTER_API_KEY",
        Provider::AmazonBedrock => return std::env::var("AWS_ACCESS_KEY_ID").ok(),
        Provider::Cerebras => "CEREBRAS_API_KEY",
        Provider::Zai => "ZAI_API_KEY",
        Provider::Together => "TOGETHER_API_KEY",
        Provider::MoonshotAI | Provider::MoonshotAICn => "MOONSHOT_API_KEY",
        Provider::HuggingFace => "HF_TOKEN",
        Provider::CloudflareWorkersAI | Provider::CloudflareAIGateway => "CLOUDFLARE_API_KEY",
        Provider::Xiaomi => "XIAOMI_API_KEY",
        Provider::XiaomiTokenPlanCn => "XIAOMI_TOKEN_PLAN_CN_API_KEY",
        Provider::XiaomiTokenPlanAms => "XIAOMI_TOKEN_PLAN_AMS_API_KEY",
        Provider::XiaomiTokenPlanSgp => "XIAOMI_TOKEN_PLAN_SGP_API_KEY",
        Provider::Custom(_) => return None,
    };
    std::env::var(env_var).ok()
}
