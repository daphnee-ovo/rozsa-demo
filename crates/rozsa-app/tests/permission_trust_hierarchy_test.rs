use rozsa_app::permissions::{
    PermissionController, PermissionMode, PermissionPolicy, PolicyVerdict,
};
use serde_json::json;

fn needs_approval(verdict: PolicyVerdict) -> rozsa_app::permissions::ApprovalInfo {
    match verdict {
        PolicyVerdict::NeedApproval { info } => info,
        other => panic!("expected NeedApproval, got {other:?}"),
    }
}

#[test]
fn compound_command_requires_every_segment_to_be_trusted() {
    let controller = PermissionController::new(PermissionMode::OnRequest, vec![], vec![], vec![]);
    let args = json!({"command": "dow status set --phase dev && cargo test"});

    let initial = needs_approval(controller.evaluate("session-a", "Bash", &args));
    let keys = initial
        .trust_levels
        .iter()
        .map(|level| level.key.as_str())
        .collect::<Vec<_>>();
    assert!(keys.contains(&"Bash:dow status set --phase dev"));
    assert!(keys.contains(&"Bash:dow status set"));
    assert!(keys.contains(&"Bash:dow status"));
    assert!(keys.contains(&"Bash:dow"));
    assert!(keys.contains(&"Bash:cargo test"));
    assert!(keys.contains(&"Bash:cargo"));

    controller.record_session_approval("session-a", "Bash:cargo test".to_string());
    let remaining = needs_approval(controller.evaluate("session-a", "Bash", &args));
    let remaining_keys = remaining
        .trust_levels
        .iter()
        .map(|level| level.key.as_str())
        .collect::<Vec<_>>();
    assert!(remaining_keys.contains(&"Bash:dow status set --phase dev"));
    assert!(
        !remaining_keys
            .iter()
            .any(|key| key.starts_with("Bash:cargo"))
    );

    controller.record_session_approval("session-a", "Bash:dow status set".to_string());
    assert!(matches!(
        controller.evaluate("session-a", "Bash", &args),
        PolicyVerdict::Allow
    ));
}

#[test]
fn progressive_trust_exposes_broader_scopes_for_a_new_untrusted_command() {
    let controller = PermissionController::new(PermissionMode::OnRequest, vec![], vec![], vec![]);
    controller.record_session_approval("session-a", "Bash:dow status set".to_string());
    let args = json!({"command": "dow status show --json"});

    let approval = needs_approval(controller.evaluate("session-a", "Bash", &args));
    let keys = approval
        .trust_levels
        .iter()
        .map(|level| level.key.as_str())
        .collect::<Vec<_>>();
    assert!(keys.contains(&"Bash:dow status show --json"));
    assert!(keys.contains(&"Bash:dow status show"));
    assert!(keys.contains(&"Bash:dow status"));
    assert!(keys.contains(&"Bash:dow"));
}

#[test]
fn allow_once_does_not_create_session_trust() {
    let policy = PermissionPolicy::new(PermissionMode::OnRequest, vec![], vec![], vec![]);
    let args = json!({"command": "cargo test"});

    let first = needs_approval(policy.evaluate("Bash", &args));
    assert_eq!(first.trust_key, "Bash:cargo test");
    // The runtime only calls record_session_approval for AllowSession. A plain
    // Allow is intentionally not recorded, so the repeated request prompts again.
    assert!(matches!(
        policy.evaluate("Bash", &args),
        PolicyVerdict::NeedApproval { .. }
    ));
}

#[test]
fn shell_prefixes_respect_word_boundaries() {
    let controller = PermissionController::new(PermissionMode::OnRequest, vec![], vec![], vec![]);
    controller.record_session_approval("session-a", "Bash:cargo".to_string());

    assert!(matches!(
        controller.evaluate("session-a", "Bash", &json!({"command": "cargo test"})),
        PolicyVerdict::Allow
    ));
    assert!(matches!(
        controller.evaluate("session-a", "Bash", &json!({"command": "cargotest"})),
        PolicyVerdict::NeedApproval { .. }
    ));
}
