use rozsa_app::settings::{Settings, SettingsManager};

#[test]
fn automatic_session_naming_defaults_on_and_can_be_disabled() {
    assert!(Settings::default().auto_session_naming);

    let temp = tempfile::tempdir().unwrap();
    let settings_path = temp.path().join("settings.json");
    std::fs::write(&settings_path, r#"{"autoSessionNaming":false}"#).unwrap();
    let settings = SettingsManager::load(settings_path, None, None).unwrap();

    assert!(!settings.resolved().auto_session_naming);
}
