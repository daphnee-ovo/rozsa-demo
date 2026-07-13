use rozsa_app::permissions::{PermissionController, PermissionMode, PolicyVerdict};
use rozsa_app::settings::SettingsManager;

#[test]
fn allow_session_trust_isolated_by_session_id_and_runtime_mode_updates() {
    let controller = PermissionController::new(PermissionMode::OnRequest, vec![], vec![], vec![]);
    let args = serde_json::json!({"file_path":"src/lib.rs","content":"x"});

    assert!(matches!(controller.evaluate("a", "write", &args), PolicyVerdict::NeedApproval { .. }));
    controller.record_session_approval("a", "write:src/lib.rs".to_string());
    assert!(matches!(controller.evaluate("a", "write", &args), PolicyVerdict::Allow));
    assert!(matches!(controller.evaluate("b", "write", &args), PolicyVerdict::NeedApproval { .. }));

    controller.update(PermissionMode::Yolo, vec![], vec![], vec![]);
    assert!(matches!(controller.evaluate("b", "write", &args), PolicyVerdict::Allow));

    let destructive = serde_json::json!({"command":"rm -rf /"});
    assert!(matches!(
        controller.evaluate("a", "Bash", &destructive),
        PolicyVerdict::Block { .. }
    ));
}

#[test]
fn project_file_trust_persists_and_applies_across_sessions() {
    let workspace = tempfile::tempdir().unwrap();
    let settings_path = workspace.path().join(".rozsa/agent/settings.json");
    let settings = SettingsManager::load(
        workspace.path().join("global-settings.json"),
        Some(settings_path.clone()),
        None,
    )
    .unwrap();
    let controller = PermissionController::with_project_rules(
        PermissionMode::OnRequest,
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        workspace.path().to_path_buf(),
        settings,
    );
    let scope = workspace.path().join("src/*.rs");
    controller
        .record_project_approval(&format!("Edit:{}", scope.display()))
        .unwrap();

    let rust_file = serde_json::json!({"file_path": workspace.path().join("src/lib.rs")});
    let text_file = serde_json::json!({"file_path": workspace.path().join("src/readme.txt")});
    assert!(matches!(
        controller.evaluate("another-session", "Edit", &rust_file),
        PolicyVerdict::Allow
    ));
    assert!(matches!(
        controller.evaluate("another-session", "Edit", &text_file),
        PolicyVerdict::NeedApproval { .. }
    ));

    let saved = std::fs::read_to_string(settings_path).unwrap();
    assert!(saved.contains("Edit("));
    assert!(saved.contains("src/*.rs"));
}
