use std::path::{Path, PathBuf};

use rozsa_app::config_paths::{ConfigRoots, encode_project_path};
use rozsa_app::settings::SettingsManager;

#[test]
fn every_category_uses_identical_paths_below_both_roots() {
    let roots = ConfigRoots::from_roots(
        PathBuf::from("/global/.rozsa"),
        PathBuf::from("/workspace/.rozsa"),
    );

    assert_eq!(
        roots.settings_paths(),
        [
            PathBuf::from("/global/.rozsa/settings.json"),
            PathBuf::from("/workspace/.rozsa/settings.json"),
        ]
    );
    assert_eq!(
        roots.model_dirs(),
        [
            PathBuf::from("/global/.rozsa/models"),
            PathBuf::from("/workspace/.rozsa/models"),
        ]
    );
    assert_eq!(
        roots.theme_dirs(),
        [
            PathBuf::from("/global/.rozsa/themes"),
            PathBuf::from("/workspace/.rozsa/themes"),
        ]
    );
    assert_eq!(
        roots.skill_dirs(),
        [
            PathBuf::from("/global/.rozsa/skills"),
            PathBuf::from("/workspace/.rozsa/skills"),
        ]
    );
    assert_eq!(
        roots.resource_dirs(),
        [
            PathBuf::from("/global/.rozsa"),
            PathBuf::from("/workspace/.rozsa"),
        ]
    );
    assert!(
        roots
            .settings_paths()
            .iter()
            .all(|path| !path.to_string_lossy().contains("/agent/"))
    );
}

#[test]
fn session_layers_keep_the_existing_project_partition_without_agent_directory() {
    let roots = ConfigRoots::from_roots(
        PathBuf::from("/global/.rozsa"),
        PathBuf::from("/workspace/.rozsa"),
    );
    let project = Path::new("/workspace/example");
    let key = encode_project_path(project);

    assert_eq!(
        roots.session_dirs(project),
        [
            PathBuf::from("/global/.rozsa/sessions").join(&key),
            PathBuf::from("/workspace/.rozsa/sessions").join(key),
        ]
    );
    assert_eq!(
        roots.writable_session_dir(project),
        PathBuf::from("/global/.rozsa/sessions").join(encode_project_path(project))
    );
}

#[test]
fn project_path_encoding_is_stable_on_unix_and_windows_style_paths() {
    assert_eq!(
        encode_project_path(Path::new("/workspace/example")),
        "-workspace-example-"
    );
    assert_eq!(
        encode_project_path(Path::new(r"C:\workspace\example")),
        "-C:-workspace-example-"
    );
}

#[test]
fn explicit_roots_are_kept_verbatim() {
    let global = PathBuf::from("/custom/global");
    let project = PathBuf::from("/custom/project");
    let roots = ConfigRoots::from_roots(global.clone(), project.clone());

    assert_eq!(roots.global(), global);
    assert_eq!(roots.project(), project);
}

#[test]
fn environment_overrides_replace_both_default_roots() {
    let roots = ConfigRoots::from_overrides(
        Path::new("/workspace"),
        Some(PathBuf::from("/env/global")),
        Some(PathBuf::from("/env/project")),
        Some(PathBuf::from("/home/ignored")),
    )
    .unwrap();

    assert_eq!(roots.global(), Path::new("/env/global"));
    assert_eq!(roots.project(), Path::new("/env/project"));
}

#[test]
fn defaults_are_home_and_project_dot_rozsa() {
    let roots = ConfigRoots::from_overrides(
        Path::new("/workspace"),
        None,
        None,
        Some(PathBuf::from("/home/user")),
    )
    .unwrap();

    assert_eq!(roots.global(), Path::new("/home/user/.rozsa"));
    assert_eq!(roots.project(), Path::new("/workspace/.rozsa"));
    assert_eq!(
        roots.agents_skills_dir(),
        Some(Path::new("/home/user/.agents/skills"))
    );
}

#[test]
fn empty_environment_override_is_rejected() {
    let error = ConfigRoots::from_overrides(
        Path::new("/workspace"),
        Some(PathBuf::new()),
        None,
        Some(PathBuf::from("/home/user")),
    )
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("ROZSA_CONFIG_DIR must not be empty")
    );
}

#[test]
fn settings_reader_uses_global_values_unless_project_overrides_them() {
    let temp = tempfile::tempdir().unwrap();
    let roots = ConfigRoots::from_roots(temp.path().join("global"), temp.path().join("project"));
    let [global, project] = roots.settings_paths();
    std::fs::create_dir_all(global.parent().unwrap()).unwrap();
    std::fs::create_dir_all(project.parent().unwrap()).unwrap();
    std::fs::write(
        &global,
        r#"{"defaultModel":"global-model","transport":"sse"}"#,
    )
    .unwrap();
    std::fs::write(&project, r#"{"defaultModel":"project-model"}"#).unwrap();

    let settings = SettingsManager::load(global, Some(project), None).unwrap();
    assert_eq!(settings.default_model(), Some("project-model"));
    assert_eq!(settings.transport(), "sse");
}
