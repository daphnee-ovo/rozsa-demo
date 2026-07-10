use rozsa_app::permissions::{PermissionController, PermissionMode, PolicyVerdict};

#[test]
fn allow_session_trust_isolated_by_session_id_and_runtime_mode_updates() {
    let controller = PermissionController::new(PermissionMode::OnRequest, vec![], vec![], vec![]);
    let args = serde_json::json!({"file_path":"src/lib.rs","content":"x"});

    assert!(matches!(controller.evaluate("a", "write", &args), PolicyVerdict::NeedApproval { .. }));
    controller.record_session_approval("a", "write:src/lib.rs".to_string());
    assert!(matches!(controller.evaluate("a", "write", &args), PolicyVerdict::Allow));
    assert!(matches!(controller.evaluate("b", "write", &args), PolicyVerdict::NeedApproval { .. }));

    controller.update(PermissionMode::FreePermission, vec![], vec![], vec![]);
    assert!(matches!(controller.evaluate("b", "write", &args), PolicyVerdict::Allow));

    let destructive = serde_json::json!({"command":"rm -rf /"});
    assert!(matches!(
        controller.evaluate("a", "Bash", &destructive),
        PolicyVerdict::Block { .. }
    ));
}
