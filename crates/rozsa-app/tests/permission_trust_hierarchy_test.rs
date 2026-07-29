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
    let controller = PermissionController::new(PermissionMode::OnRequest);
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
    assert_eq!(initial.trust_groups.len(), 2);
    assert_eq!(initial.trust_groups[0].target, "dow status set --phase dev");
    assert_eq!(initial.trust_groups[1].target, "cargo test");

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
    assert_eq!(remaining.trust_groups.len(), 1);
    assert_eq!(
        remaining.trust_groups[0].target,
        "dow status set --phase dev"
    );

    controller.record_session_approval("session-a", "Bash:dow status set".to_string());
    assert!(matches!(
        controller.evaluate("session-a", "Bash", &args),
        PolicyVerdict::Allow
    ));
}

#[test]
fn file_trust_stays_inside_workspace_and_offers_progressive_scopes() {
    let workspace = std::env::current_dir().unwrap();
    let controller = PermissionController::new(PermissionMode::OnRequest);
    let file = workspace.join("src/test.rs");
    let approval =
        needs_approval(controller.evaluate("session-a", "Edit", &json!({"file_path": file})));

    assert_eq!(approval.trust_groups.len(), 1);
    let labels = approval.trust_groups[0]
        .levels
        .iter()
        .map(|level| level.label.as_str())
        .collect::<Vec<_>>();
    assert_eq!(labels[0], workspace.join("src/test.rs").to_string_lossy());
    assert_eq!(labels[1], workspace.join("src/*.rs").to_string_lossy());
    assert_eq!(labels[2], workspace.join("src/*").to_string_lossy());
    assert_eq!(labels[3], workspace.join("*").to_string_lossy());

    let outside = needs_approval(controller.evaluate(
        "session-a",
        "Edit",
        &json!({"file_path": workspace.parent().unwrap().join("outside.rs")}),
    ));
    assert_eq!(outside.trust_groups[0].levels.len(), 1);
}

#[test]
fn progressive_trust_exposes_broader_scopes_for_a_new_untrusted_command() {
    let controller = PermissionController::new(PermissionMode::OnRequest);
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
    let policy = PermissionPolicy::new(PermissionMode::OnRequest);
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
    let controller = PermissionController::new(PermissionMode::OnRequest);
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
