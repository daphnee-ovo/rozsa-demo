use rozsa_app::model_registry::{ImageModelRegistry, ModelRegistry, RegistryModelCost};
use rozsa_model::providers::openai_completions::DiscoveredModel;

fn generated_json() -> &'static str {
    r#"{
        "openai": {
            "gpt-test": {
                "id": "gpt-test",
                "name": "GPT Test",
                "api": "openai-completions",
                "provider": "openai",
                "baseUrl": "https://api.openai.com/v1",
                "reasoning": true,
                "input": ["text", "image"],
                "cost": { "input": 1, "output": 2, "cacheRead": 0.1, "cacheWrite": 0.2 },
                "contextWindow": 128000,
                "maxTokens": 4096,
                "compat": { "supportsStore": true, "openRouterRouting": { "allow_fallbacks": true } }
            }
        }
    }"#
}

fn generated_image_json() -> &'static str {
    r#"{
        "openrouter": {
            "image-test": {
                "id": "image-test",
                "name": "Image Test",
                "api": "openrouter-images",
                "provider": "openrouter",
                "baseUrl": "https://openrouter.ai/api/v1",
                "input": ["text", "image"],
                "output": ["image"],
                "cost": { "input": 1, "output": 2, "cacheRead": 0.1, "cacheWrite": 0.2 }
            }
        }
    }"#
}

#[test]
fn loads_generated_model_metadata() {
    let registry = ModelRegistry::from_generated_json(generated_json()).unwrap();
    let model = registry.find("openai", "gpt-test").unwrap();

    assert_eq!(model.name, "GPT Test");
    assert_eq!(model.api, rozsa_model::types::Api::OpenAICompletions);
    assert_eq!(model.base_url, "https://api.openai.com/v1");
    assert_eq!(
        model.input_modalities,
        vec![
            rozsa_model::types::InputModality::Text,
            rozsa_model::types::InputModality::Image
        ]
    );
    assert_eq!(model.cost.input, 1.0);
    assert_eq!(model.cost.output, 2.0);
    assert_eq!(model.cost.cache_read, 0.1);
    assert_eq!(model.cost.cache_write, 0.2);
}

#[test]
fn loads_generated_image_model_metadata() {
    let registry = ImageModelRegistry::from_generated_json(generated_image_json()).unwrap();
    let model = registry.find("openrouter", "image-test").unwrap();

    assert_eq!(model.name, "Image Test");
    assert_eq!(model.api, "openrouter-images");
    assert_eq!(model.base_url, "https://openrouter.ai/api/v1");
    assert_eq!(model.input, vec!["text", "image"]);
    assert_eq!(model.output, vec!["image"]);
    assert_eq!(
        model.cost,
        RegistryModelCost {
            input: 1.0,
            output: 2.0,
            cache_read: 0.1,
            cache_write: 0.2,
        }
    );
}

#[test]
fn loads_image_models_from_json() {
    let registry = ImageModelRegistry::from_generated_json(generated_image_json()).unwrap();

    assert!(!registry.all().is_empty());
    assert!(registry.find("openrouter", "image-test").is_some());
}

#[test]
fn reports_image_provider_auth_from_env() {
    let original = std::env::var("OPENROUTER_API_KEY").ok();
    unsafe {
        std::env::set_var("OPENROUTER_API_KEY", "test-key");
    }

    let registry = ImageModelRegistry::from_generated_json(generated_image_json()).unwrap();
    let available = registry.provider_available();

    assert_eq!(available.get("openrouter").unwrap().configured, true);
    assert_eq!(
        available.get("openrouter").unwrap().source.as_deref(),
        Some("environment")
    );

    unsafe {
        if let Some(value) = original {
            std::env::set_var("OPENROUTER_API_KEY", value);
        } else {
            std::env::remove_var("OPENROUTER_API_KEY");
        }
    }
}

#[test]
fn load_from_nonexistent_dir_returns_empty() {
    let registry =
        ModelRegistry::load_from_dir(std::path::Path::new("/tmp/rozsa-test-nonexistent-dir"))
            .unwrap();

    assert!(registry.all().is_empty());
}

#[test]
fn merges_models_json_overrides_and_custom_models() {
    let mut registry = ModelRegistry::from_generated_json(generated_json()).unwrap();

    registry
        .apply_models_config_json(
            r#"{
                "providers": {
                    "openai": {
                        "baseUrl": "https://proxy.example.com/v1",
                        "compat": {
                            "supportsDeveloperRole": false,
                            "openRouterRouting": { "zdr": true }
                        },
                        "modelOverrides": {
                            "gpt-test": {
                                "name": "GPT Test Override",
                                "cost": { "input": 3 },
                                "maxTokens": 8192
                            }
                        },
                        "models": [
                            {
                                "id": "gpt-custom",
                                "name": "GPT Custom",
                                "reasoning": false,
                                "input": ["text"],
                                "contextWindow": 64000,
                                "maxTokens": 2048
                            }
                        ]
                    },
                    "custom-provider": {
                        "baseUrl": "https://custom.example.com/v1",
                        "apiKey": "CUSTOM_API_KEY",
                        "api": "openai-completions",
                        "models": [{ "id": "custom-model" }]
                    }
                }
            }"#,
        )
        .unwrap();

    let overridden = registry.find("openai", "gpt-test").unwrap();
    assert_eq!(overridden.name, "GPT Test Override");
    assert_eq!(overridden.base_url, "https://proxy.example.com/v1");
    assert_eq!(overridden.cost.input, 3.0);
    assert_eq!(overridden.cost.output, 2.0);
    assert_eq!(overridden.max_tokens, 8192);
    assert_eq!(
        overridden
            .compat
            .as_ref()
            .unwrap()
            .pointer("/openRouterRouting/allow_fallbacks")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        overridden
            .compat
            .as_ref()
            .unwrap()
            .pointer("/openRouterRouting/zdr")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );

    assert!(registry.is_user_configured("openai", "gpt-custom"));
    assert_eq!(
        registry.find("openai", "gpt-custom").unwrap().base_url,
        "https://proxy.example.com/v1"
    );
    assert_eq!(
        registry
            .find("custom-provider", "custom-model")
            .unwrap()
            .max_tokens,
        16_384
    );
}

#[test]
fn keeps_provider_api_keys_across_multiple_config_files() {
    let mut registry = ModelRegistry::from_generated_json(generated_json()).unwrap();

    registry
        .apply_models_config_json(
            r#"{
                "providers": {
                    "custom-provider": {
                        "baseUrl": "https://custom.example.com/v1",
                        "apiKey": "CUSTOM_API_KEY",
                        "api": "openai-completions",
                        "models": [{ "id": "custom-model" }]
                    }
                }
            }"#,
        )
        .unwrap();

    registry
        .apply_models_config_json(
            r#"{
                "providers": {
                    "amazon-bedrock": {
                        "baseUrl": "https://bedrock-runtime.us-east-1.amazonaws.com",
                        "api": "bedrock-converse-stream",
                        "models": [{ "id": "amazon.nova-lite-v1:0" }]
                    }
                }
            }"#,
        )
        .unwrap();

    assert_eq!(registry.first_available().unwrap().id, "custom-model");
    assert!(registry.is_user_configured("custom-provider", "custom-model"));
}

#[test]
fn accepts_auth_header_provider_without_api_key() {
    let mut registry = ModelRegistry::from_generated_json(generated_json()).unwrap();

    registry
        .apply_models_config_json(
            r#"{
                "providers": {
                    "codex-oauth": {
                        "baseUrl": "https://api.openai.com/v1",
                        "api": "openai-responses",
                        "authHeader": true,
                        "models": [{ "id": "gpt-4o" }]
                    }
                }
            }"#,
        )
        .unwrap();

    assert!(registry.find("codex-oauth", "gpt-4o").is_some());
}

#[test]
fn models_json_allows_line_comments_and_trailing_commas() {
    let mut registry = ModelRegistry::from_generated_json(generated_json()).unwrap();

    registry
        .apply_models_config_json(
            r#"{
                "providers": {
                    // Existing TypeScript registry accepts comments.
                    "openai": {
                        "baseUrl": "https://proxy.example.com/v1",
                        "models": [
                            {
                                "id": "commented-model",
                            },
                        ],
                    },
                },
            }"#,
        )
        .unwrap();

    assert_eq!(
        registry.find("openai", "commented-model").unwrap().base_url,
        "https://proxy.example.com/v1"
    );
}

#[test]
fn merges_discovered_nvidia_models() {
    let mut registry = ModelRegistry::from_generated_json(generated_json()).unwrap();

    registry.merge_openai_compatible_discovered_models(
        "nvidia",
        "https://integrate.api.nvidia.com/v1",
        vec![DiscoveredModel {
            id: "nvidia/nemotron".to_string(),
            name: "nvidia/nemotron".to_string(),
            context_window: 131_072,
            max_tokens: 4_096,
        }],
    );

    let model = registry.find("nvidia", "nvidia/nemotron").unwrap();
    assert_eq!(model.api, rozsa_model::types::Api::OpenAICompletions);
    assert_eq!(model.base_url, "https://integrate.api.nvidia.com/v1");
    assert_eq!(
        model
            .compat
            .as_ref()
            .unwrap()
            .get("maxTokensField")
            .and_then(serde_json::Value::as_str),
        Some("max_tokens")
    );
}
