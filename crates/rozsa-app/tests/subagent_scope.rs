// subagent::scope 单测 — 验证 SubagentScope 的工具/路径/命令/skill 访问控制

use std::path::{Path, PathBuf};

use rozsa_app::subagent::scope::{AllowedTools, SubagentScope};
use serde_json::json;

#[test]
fn inherit_allows_everything() {
    let scope = SubagentScope::inherit();
    let cwd = Path::new("/tmp");
    assert!(scope.check_tool_allowed("write", &json!({"path": "/etc/foo"}), cwd).is_ok());
    assert!(scope.check_tool_allowed("bash", &json!({"command": "rm -rf /"}), cwd).is_ok());
    assert!(scope.check_tool_allowed("skill", &json!({"skill": "anything"}), cwd).is_ok());
    assert!(scope.check_tool_allowed("anything_else", &json!({}), cwd).is_ok());
}

#[test]
fn readonly_blocks_write_tool() {
    let scope = SubagentScope::readonly();
    let cwd = Path::new("/tmp");
    let err = scope
        .check_tool_allowed("write", &json!({"path": "/tmp/x"}), cwd)
        .unwrap_err();
    assert!(err.contains("write"));
    // 只读工具放行
    assert!(scope.check_tool_allowed("read", &json!({"path": "/anywhere"}), cwd).is_ok());
    assert!(scope.check_tool_allowed("grep", &json!({}), cwd).is_ok());
}

#[test]
fn scoped_blocks_read_outside_allowed_path() {
    let allowed = vec![PathBuf::from("/repo/src")];
    let scope = SubagentScope::scoped(allowed);
    let cwd = Path::new("/repo");

    // 范围内 — 允许
    assert!(
        scope
            .check_tool_allowed("read", &json!({"path": "/repo/src/main.rs"}), cwd)
            .is_ok()
    );

    // 范围外 — 拒绝
    let err = scope
        .check_tool_allowed("read", &json!({"path": "/etc/passwd"}), cwd)
        .unwrap_err();
    assert!(err.contains("outside the allowed scope"));
}

#[test]
fn custom_with_bash_prefixes_blocks_unauthorized_commands() {
    let scope = SubagentScope::custom(
        AllowedTools::All,
        None,
        Some(vec!["git ".to_string(), "ls ".to_string()]),
        None,
    );
    let cwd = Path::new("/tmp");

    assert!(
        scope
            .check_tool_allowed("bash", &json!({"command": "git status"}), cwd)
            .is_ok()
    );
    let err = scope
        .check_tool_allowed("bash", &json!({"command": "rm -rf /"}), cwd)
        .unwrap_err();
    assert!(err.contains("does not match any allowed prefix"));
}

#[test]
fn relative_path_resolved_against_cwd() {
    let allowed = vec![PathBuf::from("/repo/src")];
    let scope = SubagentScope::scoped(allowed);
    let cwd = Path::new("/repo");

    // 相对路径 src/main.rs 应被解析为 /repo/src/main.rs — 在范围内
    assert!(
        scope
            .check_tool_allowed("read", &json!({"path": "src/main.rs"}), cwd)
            .is_ok()
    );

    // 相对路径 ../etc — 解析为 /repo/../etc，不在 /repo/src 内
    let err = scope
        .check_tool_allowed("read", &json!({"path": "../etc/passwd"}), cwd)
        .unwrap_err();
    assert!(err.contains("outside the allowed scope"));
}

#[test]
fn custom_with_allowed_skills_blocks_others() {
    let scope = SubagentScope::custom(
        AllowedTools::All,
        None,
        None,
        Some(vec!["search".to_string()]),
    );
    let cwd = Path::new("/tmp");

    assert!(
        scope
            .check_tool_allowed("skill", &json!({"skill": "search"}), cwd)
            .is_ok()
    );
    let err = scope
        .check_tool_allowed("skill", &json!({"skill": "dangerous"}), cwd)
        .unwrap_err();
    assert!(err.contains("dangerous"));
}
