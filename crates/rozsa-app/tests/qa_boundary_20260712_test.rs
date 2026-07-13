use std::path::Path;

use rozsa_app::permissions::{PermissionMode, PermissionPolicy, PolicyVerdict};

fn policy(root: &Path) -> PermissionPolicy {
    PermissionPolicy::with_workspace_root(
        PermissionMode::OnRequest,
        vec![],
        vec![],
        vec![],
        root.to_path_buf(),
    )
}

#[test]
fn readonly_commands_allow_normal_and_stdin_cases() {
    let workspace = tempfile::tempdir().unwrap();
    std::fs::write(workspace.path().join("input.txt"), "b\na\n").unwrap();
    let policy = policy(workspace.path());

    for command in [
        "cat input.txt",
        "head -n 1 input.txt",
        "tail -n 1 input.txt",
        "grep a input.txt",
        "sort input.txt",
        "cat input.txt | sort",
        "sort",
        "grep pattern",
    ] {
        assert!(
            matches!(
                policy.evaluate("Bash", &serde_json::json!({"command": command})),
                PolicyVerdict::Allow
            ),
            "expected readonly command to be allowed: {command}"
        );
    }
}

#[test]
fn shell_side_effect_and_control_syntax_never_auto_allows() {
    let workspace = tempfile::tempdir().unwrap();
    std::fs::write(workspace.path().join("input.txt"), "data\n").unwrap();
    let policy = policy(workspace.path());

    for command in [
        "echo data",
        "cat input.txt > output.txt",
        "cat input.txt; echo leaked",
        "cat input.txt && rm output.txt",
        "cat input.txt | tee output.txt",
        "cat input.txt $(touch output.txt)",
        "sort --output=output.txt input.txt",
        "tail --follow input.txt",
    ] {
        assert!(
            matches!(
                policy.evaluate("Bash", &serde_json::json!({"command": command})),
                PolicyVerdict::NeedApproval { .. } | PolicyVerdict::Block { .. }
            ),
            "expected non-readonly command not to auto-allow: {command}"
        );
    }
}

#[test]
fn read_boundary_handles_missing_paths_and_symlink_escape() {
    let workspace = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(workspace.path().join("inside.txt"), "inside\n").unwrap();
    std::fs::write(outside.path().join("secret.txt"), "secret\n").unwrap();
    let symlink = workspace.path().join("outside-link");
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path().join("secret.txt"), &symlink).unwrap();

    let policy = policy(workspace.path());
    assert!(matches!(
        policy.evaluate("Read", &serde_json::json!({"file_path": "inside.txt"})),
        PolicyVerdict::Allow
    ));
    assert!(matches!(
        policy.evaluate(
            "Read",
            &serde_json::json!({"file_path": "missing/subpath.txt"})
        ),
        PolicyVerdict::Allow
    ));
    assert!(matches!(
        policy.evaluate("Read", &serde_json::json!({"file_path": "../secret.txt"})),
        PolicyVerdict::NeedApproval { .. }
    ));
    #[cfg(unix)]
    assert!(matches!(
        policy.evaluate(
            "Read",
            &serde_json::json!({"file_path": symlink.to_string_lossy()})
        ),
        PolicyVerdict::NeedApproval { .. }
    ));
}
