// FrameworkTree
// provider_display_name.rs
// ├── built_in_providers_have_stable_display_names()
// ├── codex_oauth_has_a_dedicated_display_name()
// ├── ordinary_custom_provider_uses_its_raw_name()
// ├── custom_provider_conflicting_with_builtin_identifier_is_qualified()
// ├── custom_provider_conflicting_with_builtin_display_name_is_qualified()
// └── presentation_does_not_change_routing_identifiers()

use rozsa_model::types::Provider;

#[test]
fn built_in_providers_have_stable_display_names() {
    let cases = [
        (Provider::Anthropic, "Anthropic"),
        (Provider::OpenAI, "OpenAI"),
        (Provider::AmazonBedrock, "AmazonBedrock"),
        (Provider::Google, "Google"),
        (Provider::GoogleVertex, "GoogleVertex"),
        (Provider::DeepSeek, "DeepSeek"),
        (Provider::OpenRouter, "OpenRouter"),
        (Provider::XAI, "XAI"),
        (Provider::Groq, "Groq"),
        (Provider::Cerebras, "Cerebras"),
        (Provider::Mistral, "Mistral"),
        (Provider::Nvidia, "Nvidia"),
        (Provider::Zai, "Zai"),
        (Provider::Together, "Together"),
        (Provider::MoonshotAI, "MoonshotAI"),
        (Provider::MoonshotAICn, "MoonshotAICn"),
        (Provider::HuggingFace, "HuggingFace"),
        (Provider::CloudflareWorkersAI, "CloudflareWorkersAI"),
        (Provider::CloudflareAIGateway, "CloudflareAIGateway"),
        (Provider::Xiaomi, "Xiaomi"),
        (Provider::XiaomiTokenPlanCn, "XiaomiTokenPlanCn"),
        (Provider::XiaomiTokenPlanAms, "XiaomiTokenPlanAms"),
        (Provider::XiaomiTokenPlanSgp, "XiaomiTokenPlanSgp"),
    ];

    for (provider, expected) in cases {
        assert_eq!(provider.display_name(), expected);
    }
}

#[test]
fn codex_oauth_has_a_dedicated_display_name() {
    assert_eq!(
        Provider::Custom("codex-oauth".to_string()).display_name(),
        "CodexOauth"
    );
}

#[test]
fn ordinary_custom_provider_uses_its_raw_name() {
    assert_eq!(
        Provider::Custom("custom-name".to_string()).display_name(),
        "custom-name"
    );
}

#[test]
fn custom_provider_conflicting_with_builtin_identifier_is_qualified() {
    assert_eq!(
        Provider::Custom("AMAZON-BEDROCK".to_string()).display_name(),
        "Custom:AMAZON-BEDROCK"
    );
}

#[test]
fn custom_provider_conflicting_with_builtin_display_name_is_qualified() {
    assert_eq!(
        Provider::Custom("amazonbedrock".to_string()).display_name(),
        "Custom:amazonbedrock"
    );
    assert_eq!(
        Provider::Custom("CodexOauth".to_string()).display_name(),
        "Custom:CodexOauth"
    );
    assert_eq!(
        Provider::Custom("CODEX-OAUTH".to_string()).display_name(),
        "Custom:CODEX-OAUTH"
    );
}

#[test]
fn presentation_does_not_change_routing_identifiers() {
    let built_in = Provider::OpenAI;
    let custom = Provider::Custom("codex-oauth".to_string());

    assert_eq!(built_in.as_str(), "openai");
    assert_eq!(built_in.to_string(), "openai");
    assert_eq!(custom.as_str(), "codex-oauth");
    assert_eq!(custom.to_string(), "codex-oauth");
}
