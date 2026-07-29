use rozsa_app::permissions::{PermissionController, PermissionMode, PolicyVerdict};
use rozsa_app::settings::SettingsManager;

fn make_controller(
    settings: &SettingsManager,
    workspace: &std::path::Path,
) -> PermissionController {
    let permission = &settings.resolved().permissions;
    PermissionController::with_project_rules(
        PermissionMode::OnRequest,
        permission.deny.clone(),
        permission.ask.clone(),
        permission.allow.clone(),
        workspace.to_path_buf(),
        settings.clone(),
    )
}

#[test]
fn project_trust_persists_and_deny_precedes_ask_and_allow() {
    let temp = tempfile::tempdir().unwrap();
    let global = temp.path().join("global/settings.json");
    let project = temp.path().join("project/.rozsa/settings.json");
    std::fs::create_dir_all(global.parent().unwrap()).unwrap();
    std::fs::write(
        &global,
        r#"{"permission":{"deny":["Bash(git push *)"],"allow":["Bash(cargo test *)"]}}"#,
    )
    .unwrap();
    std::fs::create_dir_all(project.parent().unwrap()).unwrap();
    std::fs::write(
        &project,
        r#"{"permission":{"ask":["Edit(./src/)","Bash(cargo test *)"],"allow":["Bash(git push *)"]}}"#,
    )
    .unwrap();

    let settings = SettingsManager::load(global.clone(), Some(project.clone()), None).unwrap();
    let controller = make_controller(&settings, temp.path().join("project").as_path());

    assert!(matches!(
        controller.evaluate(
            "one",
            "Bash",
            &serde_json::json!({"command":"git push origin main"})
        ),
        PolicyVerdict::Block { .. }
    ));
    assert!(matches!(
        controller.evaluate(
            "one",
            "Bash",
            &serde_json::json!({"command":"cargo test unit"})
        ),
        PolicyVerdict::NeedApproval { .. }
    ));
    assert!(matches!(
        controller.evaluate(
            "one",
            "Edit",
            &serde_json::json!({"file_path":"src/lib.rs"})
        ),
        PolicyVerdict::NeedApproval { .. }
    ));

    controller
        .record_project_approval("Bash:cargo check")
        .unwrap();
    let text = std::fs::read_to_string(&project).unwrap();
    assert!(text.contains("Bash(cargo check *)"));

    let reloaded = SettingsManager::load(global, Some(project), None).unwrap();
    let later_session = make_controller(&reloaded, temp.path().join("project").as_path());
    assert!(matches!(
        later_session.evaluate(
            "later",
            "Bash",
            &serde_json::json!({"command":"cargo check --all"})
        ),
        PolicyVerdict::Allow
    ));
}
