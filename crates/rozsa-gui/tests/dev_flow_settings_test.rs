//! Dev-flow settings tests: typed global persistence, the settings pane
//! contract, typed IPC coverage, and stable error-ID conventions.

use std::path::PathBuf;

use rozsa_app::settings::SettingsManager;

fn manager(path: &std::path::Path) -> SettingsManager {
    SettingsManager::load(path.join("settings.json"), None, None).unwrap()
}

#[test]
fn dev_flow_settings_are_global_and_persist_typed_updates() {
    let temp = tempfile::tempdir().unwrap();
    let mut settings = manager(temp.path());

    assert!(settings.dev_flow_settings().enabled);
    assert!(settings.dev_flow_settings().show_sidebar_status);
    assert_eq!(settings.dev_flow_settings().executable_path, None);

    settings.set_dev_flow_enabled(false).unwrap();
    settings.set_dev_flow_sidebar_status(false).unwrap();
    let custom = PathBuf::from("/opt/bin/dow");
    settings
        .set_dev_flow_executable_path(Some(custom.clone()))
        .unwrap();

    let mut reloaded = manager(temp.path());
    assert!(!reloaded.dev_flow_settings().enabled);
    assert!(!reloaded.dev_flow_settings().show_sidebar_status);
    assert_eq!(
        reloaded.dev_flow_settings().executable_path.as_ref(),
        Some(&custom)
    );

    // Selecting Auto again removes the custom path.
    reloaded.set_dev_flow_executable_path(None).unwrap();
    let reloaded_again = manager(temp.path());
    assert_eq!(reloaded_again.dev_flow_settings().executable_path, None);
}

#[test]
fn relative_custom_path_is_rejected_and_does_not_persist() {
    let temp = tempfile::tempdir().unwrap();
    let mut settings = manager(temp.path());
    assert!(
        settings
            .set_dev_flow_executable_path(Some(PathBuf::from("relative/dow")))
            .is_err()
    );
    let reloaded = manager(temp.path());
    assert_eq!(reloaded.dev_flow_settings().executable_path, None);
}

#[test]
fn settings_pane_contract_covers_diagnostics_switches_path_rescan_and_install_hints() {
    let index = include_str!("../frontend/index.html");
    let sidebar = include_str!("../frontend/sidebar.html");
    let app_js = include_str!("../frontend/app.js");

    for markup in [index, sidebar] {
        assert!(markup.contains("data-settings-pane=\"dev-flow\""));
    }
    assert!(index.contains("id=\"pane-dev-flow\""));
    assert!(index.contains("id=\"devFlowEnabled\""));
    assert!(index.contains("id=\"devFlowSidebarStatus\""));
    assert!(index.contains("id=\"devFlowCliDiagnostics\""));
    assert!(index.contains("id=\"devFlowProjectDiagnostics\""));
    assert!(index.contains("id=\"devFlowSourceAuto\""));
    assert!(index.contains("id=\"devFlowSourceCustom\""));
    assert!(index.contains("id=\"devFlowExecutablePath\""));
    assert!(index.contains("id=\"devFlowRescan\""));

    // The pane shows the official Homebrew, npm, and Cargo install commands
    // as static hints without any execution control.
    assert!(index.contains("brew install daphnee-ovo/tap/dev-flow"));
    assert!(index.contains("npm install -g @xin_yue/dev-flow"));
    assert!(index.contains("cargo install dev-flow"));

    assert!(app_js.contains("invoke('get_dev_flow_settings')"));
    assert!(app_js.contains("invoke('set_dev_flow_enabled'"));
    assert!(app_js.contains("invoke('set_dev_flow_sidebar_status'"));
    assert!(app_js.contains("invoke('set_dev_flow_executable_path'"));
    assert!(app_js.contains("invoke('rescan_dev_flow'"));
}

#[test]
fn typed_ipc_commands_cover_every_dev_flow_setting_operation() {
    let commands = include_str!("../src/commands.rs");
    for command in [
        "get_dev_flow_settings",
        "set_dev_flow_enabled",
        "set_dev_flow_sidebar_status",
        "set_dev_flow_executable_path",
        "rescan_dev_flow",
    ] {
        assert!(
            commands.contains(&format!("#[tauri::command]\npub async fn {command}")),
            "{command}"
        );
    }
    let lib = include_str!("../src/lib.rs");
    for command in [
        "commands::get_dev_flow_settings",
        "commands::set_dev_flow_enabled",
        "commands::set_dev_flow_sidebar_status",
        "commands::set_dev_flow_executable_path",
        "commands::rescan_dev_flow",
    ] {
        assert!(lib.contains(command));
    }
}

#[test]
fn stable_notification_ids_follow_the_spec() {
    let dev_flow = include_str!("../src/dev_flow.rs");
    assert!(dev_flow.contains("pub const CLI_ERROR_ID: &str = \"dev-flow.cli\""));
    assert!(dev_flow.contains("DASHBOARD_START_PREFIX"));
    assert!(dev_flow.contains("CONNECTION_PREFIX"));
}
