// FrameworkTree
// settings_test.rs
// ├── compaction_defaults_use_ratios_and_not_persisted_token_counts()
// ├── invalid_compaction_ratio_order_is_rejected_when_loading_settings()
// ├── compaction_settings_reject_invalid_ratios()
// ├── automatic_session_naming_defaults_on_and_can_be_disabled()
// ├── dev_flow_defaults_are_enabled_without_a_custom_executable()
// ├── dev_flow_settings_are_global_and_ignore_project_and_local_values()
// ├── typed_dev_flow_updates_preserve_unrelated_global_fields()
// └── relative_dev_flow_executable_is_rejected_before_persistence()

use std::path::PathBuf;

use rozsa_app::settings::{CompactionSettings, DevFlowSettings, Settings, SettingsManager};

#[test]
fn compaction_defaults_use_ratios_and_not_persisted_token_counts() {
    let settings = Settings::default();
    assert_eq!(settings.compaction.trigger_ratio, 0.85);
    assert_eq!(settings.compaction.target_ratio, 0.30);

    let serialized = serde_json::to_value(settings).unwrap();
    assert_eq!(serialized["compaction"]["triggerRatio"], 0.85);
    assert_eq!(serialized["compaction"]["targetRatio"], 0.30);
    assert!(serialized["compaction"].get("thresholdTokens").is_none());
    assert!(serialized["compaction"].get("targetTokens").is_none());
}

#[test]
fn invalid_compaction_ratio_order_is_rejected_when_loading_settings() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("settings.json");
    std::fs::write(
        &path,
        r#"{"compaction":{"triggerRatio":0.3,"targetRatio":0.7}}"#,
    )
    .unwrap();

    let error = match SettingsManager::load(path, None, None) {
        Ok(_) => panic!("invalid compaction ratios should be rejected"),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("target ratio must be less than trigger ratio")
    );
}

#[test]
fn compaction_settings_reject_invalid_ratios() {
    let mut settings = CompactionSettings::default();
    settings.target_ratio = 0.85;
    assert!(settings.validate().is_err());

    settings.trigger_ratio = 1.1;
    settings.target_ratio = 0.3;
    assert!(settings.validate().is_err());
}

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
    assert!(Settings::default().dev_flow.show_dashboard_button);
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
        r#"{"devFlow":{"enabled":false,"showSidebarStatus":false,"showDashboardButton":false,"executablePath":"/global/dow"}}"#,
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
            show_dashboard_button: false,
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
    manager.set_dev_flow_dashboard_button(false).unwrap();
    manager
        .set_dev_flow_executable_path(Some(PathBuf::from("/custom/dow")))
        .unwrap();

    let persisted: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&global).unwrap()).unwrap();
    assert_eq!(persisted["transport"], "sse");
    assert_eq!(persisted["unknownFutureField"]["keep"], true);
    assert_eq!(persisted["devFlow"]["enabled"], false);
    assert_eq!(persisted["devFlow"]["showSidebarStatus"], false);
    assert_eq!(persisted["devFlow"]["showDashboardButton"], false);
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
