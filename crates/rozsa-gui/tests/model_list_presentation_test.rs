// FrameworkTree
// model_list_presentation_test.rs
// ├── list_models_hides_unconfigured_bedrock_without_filtering_other_providers()
// └── list_models_uses_the_provider_presentation_name()

#[test]
fn list_models_hides_unconfigured_bedrock_without_filtering_other_providers() {
    let commands = include_str!("../src/commands.rs");
    let list_models_start = commands
        .find("pub async fn list_models")
        .expect("list_models command must exist");
    let switch_model_start = commands[list_models_start..]
        .find("pub async fn switch_model")
        .map(|offset| list_models_start + offset)
        .expect("switch_model command must follow list_models");
    let implementation = &commands[list_models_start..switch_model_start];
    let compact: String = implementation
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();

    assert!(compact.contains("letprovider_available=registry.provider_available();"));
    assert!(compact.contains("model.provider!=Provider::AmazonBedrock||provider_available"));
    assert!(compact.contains(".get(model.provider.as_str())"));
    assert!(compact.contains(".is_some_and(|availability|availability.configured)"));
}

#[test]
fn list_models_uses_the_provider_presentation_name() {
    let commands = include_str!("../src/commands.rs");
    let list_models_start = commands
        .find("pub async fn list_models")
        .expect("list_models command must exist");
    let switch_model_start = commands[list_models_start..]
        .find("pub async fn switch_model")
        .map(|offset| list_models_start + offset)
        .expect("switch_model command must follow list_models");
    let implementation = &commands[list_models_start..switch_model_start];
    let compact: String = implementation
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();

    assert!(compact.contains("provider:m.provider.display_name()"));
    assert!(!compact.contains("provider:format!(\"{:?}\",m.provider)"));
}
