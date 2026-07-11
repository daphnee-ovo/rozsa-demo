use rozsa_app::permissions::{TrustGroup, TrustLevel};
use rozsa_gui::state::{
    ContextUsage, PermissionEvent, RuntimeState, SessionTab, ToolEvent, TurnActivity, UiSnapshot,
    find_tab_index_by_session, permission_pending_key,
};

#[test]
fn all_gui_event_payloads_preserve_the_origin_session_id() {
    let session_id = "session-a".to_string();
    let snapshot = UiSnapshot {
        session_id: session_id.clone(),
        turn_id: 1,
        messages: vec![],
        is_streaming: false,
        model: None,
        thinking_level: "off".to_string(),
        session_name: None,
        cwd: "/workspace".to_string(),
        git: None,
        context_usage: ContextUsage {
            percent: 0.0,
            tokens: 0,
            context_window: 0,
            input_tokens: 0,
            uncached_input_tokens: 0,
            cached_input_tokens: 0,
            output_tokens: 0,
        },
        runtime_state: RuntimeState {
            prompt_tokens: 0,
            completion_tokens: 0,
            session_total_tokens: 0,
        },
        turn_activity: TurnActivity::default(),
        turn_summaries: vec![],
        queued_messages: vec![],
        steering_conversation: vec![],
        stream_update: false,
    };
    let tool = ToolEvent::Start {
        session_id: session_id.clone(),
        turn_id: 1,
        id: "tool-1".to_string(),
        name: "write".to_string(),
        args: serde_json::json!({"file_path": "src/lib.rs"}),
    };
    let permission = PermissionEvent {
        session_id: session_id.clone(),
        turn_id: "turn-1".to_string(),
        request_id: "tool-1".to_string(),
        tool: "write".to_string(),
        summary: "write src/lib.rs".to_string(),
        risk: "Write".to_string(),
        trust_key: "write:src/lib.rs".to_string(),
        trust_levels: vec![TrustLevel {
            label: "write src/lib.rs".to_string(),
            key: "write:src/lib.rs".to_string(),
        }],
        trust_groups: vec![TrustGroup {
            target: "src/lib.rs".to_string(),
            levels: vec![],
        }],
    };

    assert_eq!(
        serde_json::to_value(snapshot).unwrap()["sessionId"],
        session_id
    );
    assert_eq!(
        serde_json::to_value(tool).unwrap()["session_id"],
        session_id
    );
    assert_eq!(
        serde_json::to_value(&permission).unwrap()["session_id"],
        session_id
    );
    assert_eq!(
        serde_json::to_value(&permission).unwrap()["trust_levels"][0]["key"],
        "write:src/lib.rs"
    );
}

#[test]
fn immutable_session_lookup_survives_tab_reordering() {
    let tabs = vec![
        SessionTab::Idle {
            path: "workspace/session-a.jsonl".to_string(),
            name: "A".to_string(),
            modified: "".to_string(),
            message_count: 0,
        },
        SessionTab::Idle {
            path: "workspace/session-b.jsonl".to_string(),
            name: "B".to_string(),
            modified: "".to_string(),
            message_count: 0,
        },
    ];
    assert_eq!(find_tab_index_by_session(&tabs, "session-b"), Some(1));
    assert_eq!(find_tab_index_by_session(&tabs, "missing"), None);
}

#[test]
fn pending_approval_keys_are_unique_per_session() {
    assert_eq!(permission_pending_key("a", "call-1"), "a:call-1");
    assert_ne!(
        permission_pending_key("a", "call-1"),
        permission_pending_key("b", "call-1")
    );
}
