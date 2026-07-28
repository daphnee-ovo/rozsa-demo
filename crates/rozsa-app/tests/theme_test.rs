use rozsa_app::settings::{AppearanceSettings, SettingsManager};
use rozsa_app::themes::{ThemeDefinition, ThemeMode, ThemeStore};
use std::collections::BTreeMap;

#[test]
fn built_in_themes_are_available_without_theme_files() {
    let temp = tempfile::tempdir().unwrap();
    let store = ThemeStore::new(temp.path().join("themes"));

    let themes = store.list().unwrap();
    assert_eq!(themes.len(), 2);
    assert_eq!(themes[0].id, "rozsa");
    assert_eq!(themes[1].id, "rozsa-dark");
    let light = store.load("rozsa", ThemeMode::Light).unwrap();
    assert_eq!(light.name, "Rozsa");
    assert_eq!(light.accent, "#D7827E");
    assert_eq!(light.background, "#FFFFFF");
    assert_eq!(light.foreground, "#575279");
    assert_eq!(
        store.load("rozsa-dark", ThemeMode::Dark).unwrap().name,
        "Rozsa Dark"
    );
}

#[test]
fn appearance_defaults_to_following_the_system_theme() {
    assert_eq!(AppearanceSettings::default().theme_mode, "system");
}

#[test]
fn custom_theme_round_trips_and_applies_extra_variables() {
    let temp = tempfile::tempdir().unwrap();
    let store = ThemeStore::new(temp.path().join("themes"));
    let mut theme = store.load("rozsa", ThemeMode::Light).unwrap();
    theme.id = "paper-light".to_string();
    theme.name = "Paper Light".to_string();
    theme.accent = "#336699".to_string();
    theme.variables = BTreeMap::from([("--surface".to_string(), "#fffdf8".to_string())]);

    store.save(&theme).unwrap();
    let loaded = store.load("paper-light", ThemeMode::Light).unwrap();
    assert_eq!(loaded.name, "Paper Light");
    assert_eq!(loaded.accent, "#336699");
    assert_eq!(loaded.variables["--surface"], "#fffdf8");
    assert!(
        store
            .list()
            .unwrap()
            .iter()
            .any(|item| item.id == "paper-light")
    );
}

#[test]
fn project_theme_overrides_global_theme_with_the_same_id() {
    let temp = tempfile::tempdir().unwrap();
    let global = temp.path().join("global");
    let project = temp.path().join("project");
    let global_store = ThemeStore::new(global.clone());
    let project_store = ThemeStore::new(project.clone());
    let mut theme = global_store.load("rozsa", ThemeMode::Light).unwrap();
    theme.id = "shared".to_string();
    theme.name = "Global".to_string();
    global_store.save(&theme).unwrap();
    theme.name = "Project".to_string();
    project_store.save(&theme).unwrap();

    let layered = ThemeStore::layered(global, project);
    assert_eq!(
        layered.load("shared", ThemeMode::Light).unwrap().name,
        "Project"
    );
    assert_eq!(
        layered
            .list()
            .unwrap()
            .into_iter()
            .filter(|item| item.id == "shared")
            .count(),
        1
    );
}

#[test]
fn invalid_theme_files_and_values_fail_loudly() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("themes");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("broken.json"), "{not json").unwrap();
    let store = ThemeStore::new(root.clone());
    assert!(store.list().is_err());

    let mut theme = store.load("rozsa", ThemeMode::Light).unwrap();
    theme.id = "unsafe;theme".to_string();
    assert!(store.save(&theme).is_err());
}

#[test]
fn settings_reject_out_of_range_font_size() {
    let temp = tempfile::tempdir().unwrap();
    let settings = temp.path().join("settings.json");
    std::fs::write(&settings, r#"{"appearance":{"fontSize":51}}"#).unwrap();
    assert!(SettingsManager::load(settings, None, None).is_err());
}

#[test]
fn theme_definition_serializes_expected_json_shape() {
    let theme = ThemeDefinition {
        id: "custom".to_string(),
        name: "Custom".to_string(),
        mode: ThemeMode::Dark,
        accent: "#fff".to_string(),
        background: "#000".to_string(),
        foreground: "#fff".to_string(),
        ui_font: "system-ui".to_string(),
        translucent_sidebar: true,
        code_font: "monospace".to_string(),
        variables: BTreeMap::new(),
    };
    let value = serde_json::to_value(theme).unwrap();
    assert_eq!(value["uiFont"], "system-ui");
    assert_eq!(value["translucentSidebar"], true);
}
