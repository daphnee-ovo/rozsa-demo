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
    let sidebar = include_str!("../frontend/sidebar.html");
    let panes = [
        "data-settings-pane=\"skills\"",
        "data-settings-pane=\"tools\"",
        "data-settings-pane=\"extensions\"",
        "data-settings-pane=\"general\"",
        "data-settings-pane=\"appearance\"",
        "data-settings-pane=\"keyboard-shortcuts\"",
        "data-settings-pane=\"permissions\"",
    ];

    for markup in [index, sidebar] {
        let offsets = ordered_offsets(markup, &panes);
        assert!(offsets.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(!markup.contains("data-settings-pane=\"models\""));
    }
    assert!(index.contains("id=\"pane-skills\""));
    assert!(index.contains("id=\"pane-extensions\""));
    assert!(index.contains("id=\"settingsModelSelect\""));
    assert!(index.contains("id=\"settingsPermMode\""));
    assert!(index.contains("id=\"pane-permissions\""));
}

#[test]
fn capability_controls_use_layer_aware_backend_commands() {
    let frontend = include_str!("../frontend/app.js");
    let index = include_str!("../frontend/index.html");
    let commands = include_str!("../src/commands.rs");
    let state = include_str!("../src/state.rs");

    assert!(frontend.contains("invoke('get_capability_settings')"));
    assert!(frontend.contains("invoke('update_capability_setting'"));
    assert!(frontend.contains("toggle.setAttribute('role', 'switch')"));
    assert!(frontend.contains("enabled: !item.effective"));
    assert!(frontend.contains("Restore global inheritance"));
    assert!(index.contains("Run <code>/reload</code>"));
    assert!(commands.contains("set_capability_override(scope, kind, &name, enabled)"));
    assert!(commands.contains(".shared\n        .registered_tool_metadata()"));
    assert!(state.contains("Settings must remain inspectable before the first chat exists."));
}

#[test]
fn general_controls_are_connected_to_real_setting_keys() {
    let index = include_str!("../frontend/index.html");
    let frontend = include_str!("../frontend/app.js");
    for id in [
        "settingsCompactionThreshold",
        "settingsCompactionTarget",
        "settingsRetryTimeout",
        "settingsHideThinking",
    ] {
        assert!(index.contains(id), "missing control {id}");
        assert!(frontend.contains(id), "unwired control {id}");
    }
    for retired in [
        "settingsPermissionPatterns",
        "settingsAllowedTools",
        "settingsBlockedCommands",
    ] {
        assert!(!index.contains(retired));
        assert!(!frontend.contains(retired));
    }
    assert!(frontend.contains("invoke('get_permission_settings')"));
    assert!(frontend.contains("invoke('update_permission_rules'"));
    assert!(index.contains("Add rule"));
    assert!(index.contains("id=\"permissionSettingsError\" role=\"alert\""));
    assert!(frontend.contains("setPermissionSettingsError(String(error))"));
    assert!(!index.contains("id=\"settingsPermissionDeny\""));
}
