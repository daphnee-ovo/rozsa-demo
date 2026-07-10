use rozsa_core::events::AgentEvent;
use rozsa_gui::state::{LiveState, ToolEvent};
use rozsa_model::types::{ContentBlock, ToolResultMessage};

fn result(details: serde_json::Value) -> ToolResultMessage {
    ToolResultMessage {
        tool_call_id: "call-1".to_string(),
        tool_name: "tool".to_string(),
        content: vec![ContentBlock::Text {
            text: "done".to_string(),
            signature: None,
        }],
        details,
        is_error: false,
        timestamp: 0,
    }
}

#[test]
fn turn_activity_collects_changed_files_and_bash_verification() {
    let mut state = LiveState::default();
    state.apply(&AgentEvent::AgentStart);
    state.apply(&AgentEvent::ToolExecutionEnd {
        tool_call_id: "write-1".to_string(),
        tool_name: "write".to_string(),
        result: result(serde_json::json!({
            "changed_files": ["src/lib.rs"],
            "success": true,
        })),
    });
    state.apply(&AgentEvent::ToolExecutionEnd {
        tool_call_id: "bash-1".to_string(),
        tool_name: "bash".to_string(),
        result: result(serde_json::json!({
            "command": "cargo test",
            "exit_code": 0,
            "success": true,
            "timed_out": false,
            "truncated": false,
            "duration_ms": 120,
        })),
    });

    assert_eq!(state.turn_activity.changed_files, ["src/lib.rs"]);
    let verification = state.turn_activity.verification.unwrap();
    assert_eq!(verification.command, "cargo test");
    assert!(verification.success);
    assert_eq!(verification.exit_code, Some(0));
    assert_eq!(verification.duration_ms, 120);
}

#[test]
fn tool_event_end_serializes_structured_details() {
    let event = ToolEvent::End {
        session_id: "session-a".to_string(),
        turn_id: 1,
        id: "bash-1".to_string(),
        name: "bash".to_string(),
        success: true,
        output: "ok".to_string(),
        details: serde_json::json!({"exit_code": 0, "duration_ms": 120}),
    };

    let payload = serde_json::to_value(event).unwrap();
    assert_eq!(payload["details"]["exit_code"], 0);
    assert_eq!(payload["details"]["duration_ms"], 120);
}
