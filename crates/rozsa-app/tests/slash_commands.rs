use rozsa_app::slash_commands::{
    AutocompleteEngine, SlashCommandInfo, SlashCommandSource, BUILTIN_SLASH_COMMANDS,
};

#[test]
fn builtin_registry_contains_compact() {
    assert!(BUILTIN_SLASH_COMMANDS.iter().any(|c| c.name == "compact"));
}

#[test]
fn complete_returns_builtins_for_prefix() {
    let engine = AutocompleteEngine::new();
    let items = engine.complete("/com", 4).expect("inside slash token");
    assert!(items.iter().any(|i| i.value == "compact"));
}

#[test]
fn complete_returns_empty_outside_slash_token() {
    let engine = AutocompleteEngine::new();
    assert!(engine.complete("hello", 5).is_none());
}

#[test]
fn complete_rejects_after_whitespace() {
    let engine = AutocompleteEngine::new();
    assert!(engine.complete("/help foo", 9).is_none());
}

#[test]
fn dynamic_commands_merge_with_builtins() {
    let dynamic = vec![SlashCommandInfo {
        name: "deploy".to_string(),
        description: Some("ship it".to_string()),
        source: SlashCommandSource::Extension,
    }];
    let engine = AutocompleteEngine::with_dynamic(dynamic);
    let items = engine.complete("/d", 2).expect("inside slash token");
    assert!(items.iter().any(|i| i.value == "deploy"));
}

#[test]
fn complete_empty_prefix_lists_all() {
    let engine = AutocompleteEngine::new();
    let items = engine.complete("/", 1).expect("inside slash token");
    assert_eq!(items.len(), BUILTIN_SLASH_COMMANDS.len());
}
