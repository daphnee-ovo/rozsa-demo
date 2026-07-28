use std::fs;

use rozsa_app::config_paths::ConfigRoots;
use rozsa_gui::key_bindings::{
    KeyBindingAction, key_bindings_path, load_key_bindings, reset_key_binding, update_key_binding,
};
use tempfile::tempdir;

#[test]
fn defaults_are_complete_without_creating_a_file() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("key_bindings.json");

    let bindings = load_key_bindings(&path).unwrap();

    assert_eq!(bindings.len(), KeyBindingAction::ALL.len());
    assert_eq!(
        bindings
            .iter()
            .find(|item| item.action == KeyBindingAction::OpenSettings)
            .unwrap()
            .binding,
        "Ctrl+,"
    );
    assert!(!path.exists());
}

#[test]
fn update_persists_only_the_override_and_reset_restores_the_default() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("nested/key_bindings.json");

    let updated = update_key_binding(&path, KeyBindingAction::NewSession, "Ctrl+Shift+N").unwrap();
    assert_eq!(
        updated
            .iter()
            .find(|item| item.action == KeyBindingAction::NewSession)
            .unwrap()
            .binding,
        "Ctrl+Shift+N"
    );
    let contents = fs::read_to_string(&path).unwrap();
    assert!(contents.contains("\"newSession\": \"Ctrl+Shift+N\""));
    assert!(!contents.contains("toggleThinking"));

    let reset = reset_key_binding(&path, KeyBindingAction::NewSession).unwrap();
    assert_eq!(
        reset
            .iter()
            .find(|item| item.action == KeyBindingAction::NewSession)
            .unwrap()
            .binding,
        "Ctrl+N"
    );
}

#[test]
fn rejects_conflicts_and_malformed_files_instead_of_hiding_them() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("key_bindings.json");

    let conflict = update_key_binding(&path, KeyBindingAction::NewSession, "Ctrl+T")
        .expect_err("a duplicate binding must be rejected");
    assert!(conflict.contains("already assigned"));

    fs::write(&path, "{not-json").unwrap();
    let malformed = load_key_bindings(&path).expect_err("invalid JSON must be visible");
    assert!(malformed.contains("Invalid"));
}

#[test]
fn path_is_owned_by_the_global_gui_configuration_root() {
    let roots = ConfigRoots::from_roots("/global".into(), "/project".into());
    assert_eq!(
        key_bindings_path(&roots),
        std::path::PathBuf::from("/global/key_bindings.json")
    );
}

#[test]
fn frontend_routes_supported_actions_through_the_effective_registry() {
    let html = include_str!("../frontend/index.html");
    let sidebar = include_str!("../frontend/sidebar.html");
    let script = include_str!("../frontend/app.js");

    assert!(html.contains("id=\"pane-keyboard-shortcuts\""));
    assert!(html.contains("id=\"keyBindingList\""));
    assert!(sidebar.contains("data-settings-pane=\"keyboard-shortcuts\""));
    assert!(script.contains("matchesKeyBinding(e, 'newSession')"));
    assert!(script.contains("matchesKeyBinding(e, 'sendMessage')"));
    assert!(script.contains("invoke('update_key_binding', { action, binding })"));
    assert!(!script.contains("if (e.ctrlKey && e.key === 'n')"));
}
