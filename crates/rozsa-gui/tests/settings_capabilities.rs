fn ordered_offsets(haystack: &str, needles: &[&str]) -> Vec<usize> {
    needles
        .iter()
        .map(|needle| {
            haystack
                .find(needle)
                .unwrap_or_else(|| panic!("missing settings contract: {needle}"))
        })
        .collect()
}

#[test]
fn settings_navigation_and_panes_match_the_product_order() {
    let index = include_str!("../frontend/index.html");
    let entry = include_str!("../frontend/runtime/entries/main.js");
    let settings = include_str!("../frontend/ui/scripts/features/settings.js");
    let shared_template = include_str!("../frontend/ui/templates/features/settings.html");
    let panes = [
        "data-settings-pane=\"skills\"",
        "data-settings-pane=\"tools\"",
        "data-settings-pane=\"extensions\"",
        "data-settings-pane=\"general\"",
        "data-settings-pane=\"appearance\"",
        "data-settings-pane=\"keyboard-shortcuts\"",
        "data-settings-pane=\"permissions\"",
        "data-settings-pane=\"dev-flow\"",
    ];

    let shared_offsets = ordered_offsets(shared_template, &panes);
    assert!(shared_offsets.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(index.contains("data-template-mount=\"settings\""));
    assert!(entry.contains("features/settings.html"));
    assert!(
        settings.contains("['general', 'skills', 'tools', 'keyboard-shortcuts', 'permissions']")
    );
    assert!(settings.contains("container.replaceChildren()"));
    assert!(settings.contains("container.dataset.uiSuspended = pane"));
    assert!(settings.contains("button.disabled = true"));
    assert!(settings.contains("button.setAttribute('aria-disabled', 'true')"));
    assert!(
        settings
            .contains("if (!tab.disabled && isSettingsPaneAvailable(tab.dataset.settingsPane))")
    );
}

#[test]
fn capability_backend_keeps_layered_updates_without_preloading_paused_settings() {
    let frontend = include_str!("../frontend/app.js");
    let shared_template = include_str!("../frontend/ui/templates/features/settings.html");
    let commands = include_str!("../src/commands.rs");
    let state = include_str!("../src/state.rs");
    let lib = include_str!("../src/lib.rs");

    let load_settings = frontend
        .split("async function loadSettings() {")
        .nth(1)
        .and_then(|body| body.split("\n}").next())
        .expect("loadSettings function must exist");
    assert!(!load_settings.contains("get_capability_settings"));
    assert!(!load_settings.contains("get_permission_settings"));
    assert!(shared_template.contains("Run <code>/reload</code>"));
    assert!(commands.contains("set_capability_override(scope, kind, &name, enabled)"));
    assert!(commands.contains(".shared\n        .registered_tool_metadata()"));
    assert!(commands.contains("agents_skills_dir"));
    assert!(commands.contains("SkillScope::Agents"));
    assert!(lib.contains("commands::get_capability_settings"));
    assert!(lib.contains("commands::update_capability_setting"));
    assert!(state.contains("Settings must remain inspectable before the first chat exists."));
}

#[test]
fn legacy_permission_rule_renderer_remains_available_for_t036_cleanup_gate() {
    let frontend = include_str!("../frontend/app.js");
    let css = include_str!("../frontend/styles/features/settings.css");
    let commands = include_str!("../src/commands.rs");

    assert!(commands.contains("name: \"Read-only Bash\""));
    assert!(commands.contains("default_allow_groups"));
    assert!(frontend.contains("permissionRuleGroups"));
    assert!(frontend.contains("title.textContent = group.name"));
    assert!(frontend.contains("document.createElement('details')"));
    assert!(frontend.contains("details.open = previousGroupOpen.get(group.name) === true"));
    assert!(frontend.contains("removePermissionRule(kind, rule)"));
    assert!(frontend.contains("wirePermissionRulePointerDrag(row, kind, rule)"));
    assert!(css.contains("permission-rule-group"));
    assert!(css.contains(".permission-rule-group-summary"));
}

#[test]
fn uncovered_settings_panes_are_cleared_and_blocked_before_interaction() {
    let settings = include_str!("../frontend/ui/scripts/features/settings.js");
    assert!(settings.contains("'general'"));
    assert!(settings.contains("'permissions'"));
    assert!(settings.contains("container.replaceChildren()"));
    assert!(settings.contains("button.disabled = true"));
    assert!(settings.contains("button.setAttribute('aria-disabled', 'true')"));
    assert!(
        settings
            .contains("if (!tab.disabled && isSettingsPaneAvailable(tab.dataset.settingsPane))")
    );
}
