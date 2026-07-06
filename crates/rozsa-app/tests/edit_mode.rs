// edit_mode 单测 — 验证 think_first 模式的 tool blocking 逻辑

use rozsa_app::runtime_state::EditMode;
use serde_json::json;

#[test]
fn normal_mode_allows_all_tools() {
    let mode = EditMode::Normal;
    assert!(mode.check_tool_blocked("edit", &json!({})).is_none());
    assert!(mode.check_tool_blocked("write", &json!({})).is_none());
    assert!(
        mode.check_tool_blocked("bash", &json!({"command": "rm -rf /"}))
            .is_none()
    );
}

#[test]
fn think_first_blocks_edit_tool() {
    let mode = EditMode::ThinkFirst;
    let result = mode.check_tool_blocked("edit", &json!({}));
    assert!(result.is_some());
    assert!(result.unwrap().contains("think_first"));
}

#[test]
fn think_first_blocks_write_tool() {
    let mode = EditMode::ThinkFirst;
    let result = mode.check_tool_blocked("write", &json!({}));
    assert!(result.is_some());
    assert!(result.unwrap().contains("think_first"));
}

#[test]
fn think_first_allows_read_only_bash() {
    let mode = EditMode::ThinkFirst;
    let allowed = [
        "ls -la",
        "cat foo.txt",
        "grep -rn foo",
        "git status",
        "git log --oneline",
    ];
    for cmd in allowed {
        assert!(
            mode.check_tool_blocked("bash", &json!({"command": cmd}))
                .is_none(),
            "should allow: {cmd}"
        );
    }
}

#[test]
fn think_first_blocks_mutating_bash() {
    let mode = EditMode::ThinkFirst;
    let blocked = ["rm -rf /tmp", "cargo build", "npm install", "mkdir foo"];
    for cmd in blocked {
        let result = mode.check_tool_blocked("bash", &json!({"command": cmd}));
        assert!(result.is_some(), "should block: {cmd}");
        assert!(result.unwrap().contains("read-only allowlist"));
    }
}

#[test]
fn think_first_allows_other_tools() {
    let mode = EditMode::ThinkFirst;
    assert!(mode.check_tool_blocked("read", &json!({})).is_none());
    assert!(mode.check_tool_blocked("glob", &json!({})).is_none());
}

#[test]
fn cycle_toggles_between_modes() {
    assert_eq!(EditMode::Normal.cycle(), EditMode::ThinkFirst);
    assert_eq!(EditMode::ThinkFirst.cycle(), EditMode::Normal);
}
