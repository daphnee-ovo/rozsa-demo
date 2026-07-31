// FrameworkTree
// settings_test.rs
// ├── automatic_session_naming_defaults_on_and_can_be_disabled()
// ├── dev_flow_defaults_are_enabled_without_a_custom_executable()
// ├── dev_flow_settings_are_global_and_ignore_project_and_local_values()
// ├── typed_dev_flow_updates_preserve_unrelated_global_fields()
// └── relative_dev_flow_executable_is_rejected_before_persistence()

use std::path::PathBuf;

use rozsa_app::settings::{DevFlowSettings, Settings, SettingsManager};

#[test]
fn automatic_session_naming_defaults_on_and_can_be_disabled() {
    assert!(Settings::default().auto_session_naming);
    assert!(Settings::default().small_model.is_none());

    let temp = tempfile::tempdir().unwrap();
    let settings_path = temp.path().join("settings.json");
    std::fs::write(
        &settings_path,
        r#"{"autoSessionNaming":false,"smallModel":"gpt-cheap"}"#,
    )
    .unwrap();
    let settings = SettingsManager::load(settings_path, None, None).unwrap();

    assert!(!settings.resolved().auto_session_naming);
    assert_eq!(
        settings.resolved().small_model.as_deref(),
        Some("gpt-cheap")
    );
}

#[test]
fn dev_flow_defaults_are_enabled_without_a_custom_executable() {
    assert_eq!(Settings::default().dev_flow, DevFlowSettings::default());
    assert!(Settings::default().dev_flow.enabled);
    assert!(Settings::default().dev_flow.show_sidebar_status);
    assert!(Settings::default().dev_flow.executable_path.is_none());
}

#[test]
fn dev_flow_settings_are_global_and_ignore_project_and_local_values() {
    let temp = tempfile::tempdir().unwrap();
    let global = temp.path().join("global.json");
    let project = temp.path().join("project.json");
    let local = temp.path().join("local.json");
    std::fs::write(
        &global,
        r#"{"devFlow":{"enabled":false,"showSidebarStatus":false,"executablePath":"/global/dow"}}"#,
    )
    .unwrap();
    std::fs::write(
        &project,
        r#"{"devFlow":{"enabled":true,"showSidebarStatus":true,"executablePath":"/project/dow"}}"#,
    )
    .unwrap();
    std::fs::write(&local, r#"{"devFlow":{"enabled":true}}"#).unwrap();

    let settings = SettingsManager::load(global, Some(project), Some(local)).unwrap();

    assert_eq!(
        settings.dev_flow_settings(),
        &DevFlowSettings {
            enabled: false,
            show_sidebar_status: false,
            executable_path: Some(PathBuf::from("/global/dow")),
        }
    );
}

#[test]
fn typed_dev_flow_updates_preserve_unrelated_global_fields() {
    let temp = tempfile::tempdir().unwrap();
    let global = temp.path().join("settings.json");
    std::fs::write(
        &global,
        r#"{"transport":"sse","unknownFutureField":{"keep":true},"devFlow":{"enabled":true}}"#,
    )
    .unwrap();
    let mut manager = SettingsManager::load(global.clone(), None, None).unwrap();

    manager.set_dev_flow_enabled(false).unwrap();
    manager.set_dev_flow_sidebar_status(false).unwrap();
    manager
        .set_dev_flow_executable_path(Some(PathBuf::from("/custom/dow")))
        .unwrap();

    let persisted: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&global).unwrap()).unwrap();
    assert_eq!(persisted["transport"], "sse");
    assert_eq!(persisted["unknownFutureField"]["keep"], true);
    assert_eq!(persisted["devFlow"]["enabled"], false);
    assert_eq!(persisted["devFlow"]["showSidebarStatus"], false);
    assert_eq!(persisted["devFlow"]["executablePath"], "/custom/dow");
    assert_eq!(
        manager.dev_flow_settings().executable_path,
        Some(PathBuf::from("/custom/dow"))
    );

    manager.set_dev_flow_executable_path(None).unwrap();
    let persisted: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(global).unwrap()).unwrap();
    assert!(persisted["devFlow"].get("executablePath").is_none());
    assert_eq!(persisted["devFlow"]["enabled"], false);
}

#[test]
fn relative_dev_flow_executable_is_rejected_before_persistence() {
    let temp = tempfile::tempdir().unwrap();
    let global = temp.path().join("settings.json");
    std::fs::write(&global, r#"{"transport":"sse"}"#).unwrap();
    let before = std::fs::read_to_string(&global).unwrap();
    let mut manager = SettingsManager::load(global.clone(), None, None).unwrap();

    let error = manager
        .set_dev_flow_executable_path(Some(PathBuf::from("relative/dow")))
        .unwrap_err();

    assert!(error.to_string().contains("must be absolute"));
    assert_eq!(std::fs::read_to_string(global).unwrap(), before);
}
