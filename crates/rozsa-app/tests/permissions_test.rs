use rozsa_app::permissions::{
    build_trust_key, classify_risk, PermissionMode, PermissionPolicy, PolicyVerdict, RiskLevel,
};

#[test]
fn parse_permission_mode() {
    assert_eq!(
        PermissionMode::parse("on-request"),
        Some(PermissionMode::OnRequest)
    );
    assert_eq!(
        PermissionMode::parse("auto-permission"),
        Some(PermissionMode::AutoApprove)
    );
    assert_eq!(
        PermissionMode::parse("free-permission"),
        Some(PermissionMode::FreePermission)
    );
    assert_eq!(PermissionMode::parse("invalid"), None);
}

#[test]
fn free_permission_always_allows() {
    let policy = PermissionPolicy::new(PermissionMode::FreePermission, vec![]);
    let args = serde_json::json!({"command": "rm -rf /"});
    assert!(matches!(
        policy.evaluate("Bash", &args),
        PolicyVerdict::Allow
    ));
}

#[test]
fn blacklist_blocks_dangerous_commands() {
    let policy = PermissionPolicy::new(PermissionMode::OnRequest, vec![]);

    let cases = vec![
        ("rm -rf /home", "rm -rf on root is always blocked"),
        ("sudo apt install foo", "sudo requires manual execution"),
        ("git reset --hard HEAD~3", "destructive: git reset --hard"),
        ("git push --force origin main", "destructive: force push"),
        ("mkfs.ext4 /dev/sda1", "destructive: filesystem format"),
    ];

    for (cmd, expected_reason) in cases {
        let args = serde_json::json!({"command": cmd});
        match policy.evaluate("Bash", &args) {
            PolicyVerdict::Block { reason } => {
                assert_eq!(reason, expected_reason, "command: {cmd}");
            }
            other => panic!("expected Block for '{cmd}', got: {other:?}"),
        }
    }
}

#[test]
fn blacklist_only_applies_to_bash() {
    let policy = PermissionPolicy::new(PermissionMode::OnRequest, vec![]);
    let args = serde_json::json!({"command": "sudo rm -rf /"});
    match policy.evaluate("CustomTool", &args) {
        PolicyVerdict::NeedApproval { .. } => {}
        other => panic!("expected NeedApproval, got: {other:?}"),
    }
}

#[test]
fn session_approval_allows_repeat() {
    let policy = PermissionPolicy::new(PermissionMode::OnRequest, vec![]);
    let args = serde_json::json!({"command": "git status"});

    match policy.evaluate("Bash", &args) {
        PolicyVerdict::NeedApproval { info } => {
            assert_eq!(info.tool_name, "Bash");
            policy.record_session_approval(info.trust_key);
        }
        other => panic!("expected NeedApproval, got: {other:?}"),
    }

    assert!(matches!(
        policy.evaluate("Bash", &args),
        PolicyVerdict::Allow
    ));
}

#[test]
fn auto_approve_pattern_matches() {
    let policy = PermissionPolicy::new(
        PermissionMode::AutoApprove,
        vec!["^Read:".to_string(), r"^Bash:git\s".to_string()],
    );

    let args = serde_json::json!({"file_path": "/src/main.rs"});
    assert!(matches!(
        policy.evaluate("Read", &args),
        PolicyVerdict::Allow
    ));

    let args = serde_json::json!({"command": "git log --oneline"});
    assert!(matches!(
        policy.evaluate("Bash", &args),
        PolicyVerdict::Allow
    ));

    let args = serde_json::json!({"command": "curl https://example.com"});
    match policy.evaluate("Bash", &args) {
        PolicyVerdict::NeedApproval { .. } => {}
        other => panic!("expected NeedApproval, got: {other:?}"),
    }
}

#[test]
fn classify_risk_correct() {
    assert_eq!(classify_risk("Bash"), RiskLevel::Shell);
    assert_eq!(classify_risk("Write"), RiskLevel::Write);
    assert_eq!(classify_risk("Edit"), RiskLevel::Write);
    assert_eq!(classify_risk("Read"), RiskLevel::Read);
    assert_eq!(classify_risk("Grep"), RiskLevel::Read);
    assert_eq!(classify_risk("Ls"), RiskLevel::Read);
    assert_eq!(classify_risk("Find"), RiskLevel::Read);
    assert_eq!(classify_risk("UnknownTool"), RiskLevel::Destructive);
}

#[test]
fn build_trust_key_bash() {
    let args = serde_json::json!({"command": "git status"});
    assert_eq!(build_trust_key("Bash", &args), "Bash:git status");
}

#[test]
fn build_trust_key_read() {
    let args = serde_json::json!({"file_path": "/src/main.rs"});
    assert_eq!(build_trust_key("Read", &args), "Read:/src/main.rs");
}

#[test]
fn build_trust_key_truncates_long_command() {
    let long_cmd = "a".repeat(100);
    let args = serde_json::json!({"command": long_cmd});
    let key = build_trust_key("Bash", &args);
    assert!(key.len() <= 48);
    assert!(key.ends_with("..."));
}
