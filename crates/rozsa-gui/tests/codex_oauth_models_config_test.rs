#[test]
fn codex_oauth_fallback_matches_bundled_codex_gpt_models() {
    let commands = include_str!("../src/commands.rs");

    let expected_models = [
        ("gpt-5.6-sol", "GPT-5.6-Sol", "372_000"),
        ("gpt-5.6-terra", "GPT-5.6-Terra", "372_000"),
        ("gpt-5.6-luna", "GPT-5.6-Luna", "372_000"),
        ("gpt-5.5", "GPT-5.5", "272_000"),
        ("gpt-5.4", "GPT-5.4", "272_000"),
        ("gpt-5.4-mini", "GPT-5.4-Mini", "272_000"),
    ];

    let mut previous_position = 0;
    for (id, name, context_window) in expected_models {
        let entry = format!("codex_oauth_model(\"{id}\", \"{name}\", {context_window})");
        let position = commands
            .find(&entry)
            .unwrap_or_else(|| panic!("missing codex-oauth fallback model: {entry}"));
        assert!(
            position >= previous_position,
            "codex-oauth fallback model is out of order: {id}"
        );
        previous_position = position;
    }

    assert!(commands.contains("\"_fallback_version\": 4"));
    assert!(!commands.contains("codex_oauth_model(\"gpt-5.3-codex\""));
    assert!(!commands.contains("codex_oauth_model(\"gpt-5.2\""));
    assert!(commands.contains("\"maxTokens\": context_window / 2"));
}

#[test]
fn codex_oauth_fallback_upgrade_preserves_unmanaged_configs() {
    let commands = include_str!("../src/commands.rs");
    let function_start = commands
        .find("fn ensure_codex_oauth_models_config")
        .expect("missing codex-oauth fallback writer");
    let function = &commands[function_start..];

    assert!(function.contains("let is_managed_fallback = existing_config"));
    assert!(function.contains("== Some(\"codex-rs/models-manager/models.json\")"));
    assert!(function.contains("let fallback_version = existing_config"));
    assert!(function.contains("if !is_managed_fallback || fallback_version >= 4"));
}
