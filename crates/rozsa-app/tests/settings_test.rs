use rozsa_app::settings::{Settings, SettingsManager};

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
