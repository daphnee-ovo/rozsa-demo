use rozsa_app::settings::{SettingsManager, merge::merge_settings, schema::PartialSettings};

#[test]
fn running_send_mode_defaults_to_queue_and_accepts_settings_override() {
    let temp = tempfile::tempdir().unwrap();
    let defaults =
        SettingsManager::load(temp.path().join("missing-settings.json"), None, None).unwrap();
    assert_eq!(defaults.resolved().running_send_mode, "queue");

    let override_settings: PartialSettings = serde_json::from_value(serde_json::json!({
        "runningSendMode": "steer"
    }))
    .unwrap();
    let resolved = merge_settings(defaults.resolved(), &override_settings);
    assert_eq!(resolved.running_send_mode, "steer");
}
