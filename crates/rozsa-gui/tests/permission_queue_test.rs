use rozsa_app::permissions::{PermissionController, PermissionMode, PolicyVerdict};

#[test]
fn queued_request_is_re_evaluated_after_an_earlier_trust() {
    let controller = PermissionController::new(PermissionMode::OnRequest);
    let first = serde_json::json!({"command": "cargo test test1"});
    let queued = serde_json::json!({"command": "cargo test test2"});

    assert!(matches!(
        controller.evaluate("session", "Bash", &first),
        PolicyVerdict::NeedApproval { .. }
    ));
    assert!(matches!(
        controller.evaluate("session", "Bash", &queued),
        PolicyVerdict::NeedApproval { .. }
    ));

    // This mirrors the selected `cargo test *` scope being persisted before
    // the next queued item reaches the permission panel.
    controller
        .record_project_approval("Bash:cargo test")
        .unwrap();
    assert!(matches!(
        controller.evaluate("session", "Bash", &queued),
        PolicyVerdict::Allow
    ));
}
