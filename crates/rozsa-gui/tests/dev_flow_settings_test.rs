// FrameworkTree
// dev_flow_settings_test.rs
// ├── manager()
// ├── dev_flow_settings_are_global_and_persist_typed_updates()
// ├── relative_custom_path_is_rejected_and_does_not_persist()
// ├── settings_pane_uses_shared_style_and_covers_overview_switches_and_install_hints()
// ├── stale_dev_flow_setting_responses_cannot_replace_the_latest_intent()
// ├── enabling_dev_flow_restores_dependent_controls_when_cli_is_available()
// ├── typed_ipc_commands_cover_every_dev_flow_setting_operation()
// ├── development_bundle_launches_with_the_repository_as_its_explicit_project()
// └── stable_notification_ids_follow_the_spec()

//! Dev-flow settings tests: typed global persistence, the settings pane
//! contract, typed IPC coverage, and stable error-ID conventions.

use std::path::PathBuf;
use std::process::Command;

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
    assert!(settings.dev_flow_settings().show_dashboard_button);
    assert_eq!(settings.dev_flow_settings().executable_path, None);

    settings.set_dev_flow_enabled(false).unwrap();
    settings.set_dev_flow_sidebar_status(false).unwrap();
    settings.set_dev_flow_dashboard_button(false).unwrap();
    let custom = PathBuf::from("/opt/bin/dow");
    settings
        .set_dev_flow_executable_path(Some(custom.clone()))
        .unwrap();

    let mut reloaded = manager(temp.path());
    assert!(!reloaded.dev_flow_settings().enabled);
    assert!(!reloaded.dev_flow_settings().show_sidebar_status);
    assert!(!reloaded.dev_flow_settings().show_dashboard_button);
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
fn settings_pane_uses_shared_style_and_covers_overview_switches_and_install_hints() {
    let index = include_str!("../frontend/index.html");
    let sidebar = include_str!("../frontend/sidebar.html");
    let app_js = include_str!("../frontend/app.js");

    for markup in [index, sidebar] {
        assert!(markup.contains("data-settings-pane=\"dev-flow\""));
    }
    assert!(index.contains("id=\"pane-dev-flow\""));
    assert!(index.contains("id=\"devFlowEnabled\""));
    assert!(index.contains("id=\"devFlowSidebarStatus\""));
    assert!(index.contains("id=\"devFlowDashboardButton\""));
    assert!(index.contains("id=\"devFlowVersion\""));
    assert!(index.contains("id=\"devFlowMissing\""));
    assert!(index.contains("id=\"devFlowDashboardAvailability\""));
    assert!(index.contains("id=\"devFlowDashboardAddress\""));
    assert!(index.contains("id=\"devFlowMemoryAmount\""));
    assert!(index.contains("id=\"devFlowMemoryUnit\""));
    assert!(index.contains("id=\"devFlowExecutablePath\""));
    assert!(index.contains("id=\"devFlowPickExecutable\""));
    assert!(!index.contains("id=\"devFlowUseAutomatic\""));
    assert!(index.contains("id=\"devFlowRescan\""));
    assert!(index.contains("class=\"dev-flow-overview\""));
    assert!(index.contains("class=\"dev-flow-section\""));
    assert!(index.contains("class=\"setting-note dev-flow-description\"><strong>Engineering discipline for coding agents.</strong> Connect this project"));
    assert!(!index.contains("class=\"dev-flow-tagline\""));
    assert!(index.contains("class=\"dev-flow-overview-link\" id=\"devFlowDashboardAddress\""));
    assert!(index.contains("class=\"dev-flow-overview-icon\""));
    assert!(
        index.contains(
            "class=\"dev-flow-icon-button\" type=\"button\" id=\"devFlowPickExecutable\""
        )
    );
    assert!(!index.contains("id=\"devFlowExecutablePath\" type=\"text\" aria-label=\"Resolved dow executable path\" readonly"));
    assert!(index.contains("Engineering discipline for coding agents."));
    assert!(!index.contains("dev-flow-diagnostics"));
    assert!(!index.contains("id=\"devFlowSourceAuto\""));
    assert!(!index.contains("id=\"devFlowSourceCustom\""));

    // The pane shows the official Homebrew, npm, and Cargo install commands
    // as static hints without any execution control.
    assert!(index.contains("brew install daphnee-ovo/tap/dev-flow"));
    assert!(index.contains("npm install -g @xin_yue/dev-flow"));
    assert!(index.contains("cargo install dev-flow"));

    assert!(app_js.contains("invoke('get_dev_flow_settings')"));
    assert!(app_js.contains("'set_dev_flow_enabled'"));
    assert!(app_js.contains("'set_dev_flow_sidebar_status'"));
    assert!(app_js.contains("'set_dev_flow_dashboard_button'"));
    assert!(app_js.contains("'set_dev_flow_executable_path'"));
    assert!(app_js.contains("'rescan_dev_flow'"));
    assert!(app_js.contains("invoke('pick_dev_flow_executable')"));
    assert!(app_js.contains("invoke('open_dev_flow_dashboard')"));
    assert!(app_js.contains("pathInput.addEventListener('blur'"));
    assert!(app_js.contains("set_dev_flow_executable_path"));
    assert!(app_js.contains("function setDevFlowDependentControlDisabled"));
    assert!(app_js.contains("const dependentDisabled = devFlowDependentControlsDisabled(s);"));
    assert!(app_js.contains("setDevFlowDependentControlDisabled(enabled, false);"));
    assert!(app_js.contains("let devFlowSettingsRevision = 0;"));
    assert!(app_js.contains("revision !== devFlowSettingsRevision"));
    assert!(app_js.contains("{ ...devFlowSettings, ...optimistic }"));
    assert!(app_js.contains("function showDevFlowSettingsError"));
    assert!(!app_js.contains("showError('Failed to update Dev Flow"));

    for diagnostic in [
        "Dashboard",
        "devFlowDashboardAddressText",
        "devFlowMemoryAmount",
    ] {
        assert!(
            index.contains(diagnostic),
            "missing {diagnostic} diagnostic"
        );
    }

    let overview_start = index
        .find("<div class=\"dev-flow-module dev-flow-overview-module\">")
        .unwrap();
    let settings_start = index
        .find("<div class=\"dev-flow-module dev-flow-settings-rows\">")
        .unwrap();
    let path_position = index.find("id=\"devFlowExecutablePath\"").unwrap();
    assert!(settings_start > overview_start);
    assert!(path_position > settings_start);
}

#[test]
fn stale_dev_flow_setting_responses_cannot_replace_the_latest_intent() {
    let app_js = include_str!("../frontend/app.js");
    let start = app_js
        .find("function acceptDevFlowSettingsSnapshot")
        .unwrap();
    let end = app_js[start..]
        .find("\n\nasync function loadDevFlowSettings")
        .unwrap()
        + start;
    let function = &app_js[start..end];
    let script = format!(
        "let devFlowSettings={{enabled:true}};let devFlowSettingsRevision=2;let renders=0;function renderDevFlowSettings(){{renders++;}}\n{function}\nif(acceptDevFlowSettingsSnapshot(1,{{enabled:false}})!==false)process.exit(1);if(!devFlowSettings.enabled||renders!==0)process.exit(2);if(acceptDevFlowSettingsSnapshot(2,{{enabled:false}})!==true)process.exit(3);if(devFlowSettings.enabled||renders!==1)process.exit(4);"
    );
    let status = Command::new("node").arg("-e").arg(script).status().unwrap();
    assert!(status.success());
}

#[test]
fn enabling_dev_flow_restores_dependent_controls_when_cli_is_available() {
    let app_js = include_str!("../frontend/app.js");
    let start = app_js
        .find("function devFlowDependentControlsDisabled")
        .unwrap();
    let end = app_js[start..]
        .find("\n\nfunction renderDevFlowSettings")
        .unwrap()
        + start;
    let function = &app_js[start..end];
    let script = format!(
        "{function}\nif(!devFlowDependentControlsDisabled({{enabled:false,cli:{{available:true}}}}))process.exit(1);if(devFlowDependentControlsDisabled({{enabled:true,cli:{{available:true}}}}))process.exit(2);if(!devFlowDependentControlsDisabled({{enabled:true,cli:{{available:false}}}}))process.exit(3);"
    );
    let status = Command::new("node").arg("-e").arg(script).status().unwrap();
    assert!(status.success());
}

#[test]
fn typed_ipc_commands_cover_every_dev_flow_setting_operation() {
    let commands = include_str!("../src/commands.rs");
    for command in [
        "get_dev_flow_settings",
        "set_dev_flow_enabled",
        "set_dev_flow_sidebar_status",
        "set_dev_flow_dashboard_button",
        "set_dev_flow_executable_path",
        "rescan_dev_flow",
        "pick_dev_flow_executable",
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
        "commands::set_dev_flow_dashboard_button",
        "commands::set_dev_flow_executable_path",
        "commands::rescan_dev_flow",
        "commands::pick_dev_flow_executable",
    ] {
        assert!(lib.contains(command));
    }
    assert!(commands.contains("state.dev_flow_settings_update.lock().await"));
    assert!(commands.contains("crate::events::emit_sidebar_state(&app, state.inner()).await?"));
    assert!(commands.contains("Result<crate::dev_flow::DevFlowSettingsSnapshot, String>"));
}

#[test]
fn development_bundle_launches_with_the_repository_as_its_explicit_project() {
    let run_script = include_str!("../../../run.sh");
    assert!(run_script.contains("open -n \"$app_bundle\" --args \"$project_dir\""));
}

#[test]
fn stable_notification_ids_follow_the_spec() {
    let dev_flow = include_str!("../src/dev_flow.rs");
    assert!(dev_flow.contains("pub const CLI_ERROR_ID: &str = \"dev-flow.cli\""));
    assert!(dev_flow.contains("DASHBOARD_START_PREFIX"));
    assert!(dev_flow.contains("CONNECTION_PREFIX"));
}
