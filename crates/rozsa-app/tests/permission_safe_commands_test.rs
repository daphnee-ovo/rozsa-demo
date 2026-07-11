use rozsa_app::permissions::{PermissionMode, PermissionPolicy, PolicyVerdict};

fn policy_for_workspace(workspace: &std::path::Path) -> PermissionPolicy {
    PermissionPolicy::with_workspace_root(
        PermissionMode::OnRequest,
        vec![],
        vec![],
        vec![],
        workspace.to_path_buf(),
    )
}

#[test]
fn only_workspace_readonly_bash_commands_auto_allow() {
    let workspace = tempfile::tempdir().unwrap();
    let source = workspace.path().join("src/lib.rs");
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(&source, "fn main() {}\n").unwrap();
    let policy = policy_for_workspace(workspace.path());

    for command in [
        "head -n 2 src/lib.rs",
        "tail -n 2 src/lib.rs",
        "cat src/lib.rs",
        "grep main src/lib.rs",
        "sort src/lib.rs",
        "cat src/lib.rs | sort",
    ] {
        let args = serde_json::json!({"command": command});
        assert!(matches!(
            policy.evaluate("Bash", &args),
            PolicyVerdict::Allow
        ), "expected auto-allow for {command}");
    }
}

#[test]
fn potentially_side_effectful_or_outside_commands_still_need_approval() {
    let workspace = tempfile::tempdir().unwrap();
    let source = workspace.path().join("src/lib.rs");
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(&source, "fn main() {}\n").unwrap();
    let policy = policy_for_workspace(workspace.path());

    for command in [
        "echo hello",
        "cat src/lib.rs > output.txt",
        "sort -o output.txt src/lib.rs",
        "cat ../outside.txt",
        "cat /etc/hosts",
        "cat src/lib.rs | echo copied",
    ] {
        let args = serde_json::json!({"command": command});
        assert!(matches!(
            policy.evaluate("Bash", &args),
            PolicyVerdict::NeedApproval { .. }
        ), "expected approval for {command}");
    }
}

#[test]
fn read_requires_the_resolved_path_to_stay_inside_workspace() {
    let workspace = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let inside = workspace.path().join("src/lib.rs");
    std::fs::create_dir_all(inside.parent().unwrap()).unwrap();
    std::fs::write(&inside, "inside\n").unwrap();
    let outside_file = outside.path().join("secret.txt");
    std::fs::write(&outside_file, "outside\n").unwrap();
    let link = workspace.path().join("linked-secret.txt");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside_file, &link).unwrap();

    let policy = policy_for_workspace(workspace.path());
    assert!(matches!(
        policy.evaluate("Read", &serde_json::json!({"file_path": "src/lib.rs"})),
        PolicyVerdict::Allow
    ));
    assert!(matches!(
        policy.evaluate(
            "Read",
            &serde_json::json!({"file_path": "../outside/secret.txt"})
        ),
        PolicyVerdict::NeedApproval { .. }
    ));
    #[cfg(unix)]
    assert!(matches!(
        policy.evaluate(
            "Read",
            &serde_json::json!({"file_path": link.to_string_lossy()})
        ),
        PolicyVerdict::NeedApproval { .. }
    ));
}
