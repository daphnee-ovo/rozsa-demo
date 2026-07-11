use rozsa_app::session::manager::SessionManager;
use rozsa_core::messages::AgentMessage;
use rozsa_gui::turn_diff::{
    INTERACTION_STARTED, persisted_interaction_activity, summarize_messages,
};
use rozsa_model::types::{
    Api, AssistantMessage, ContentBlock, Message, Provider, StopReason, ToolResultMessage, Usage,
    UserContent, UserMessage,
};
use serde_json::json;

fn user(text: &str, timestamp: i64) -> AgentMessage {
    AgentMessage::standard(Message::User(UserMessage {
        content: UserContent::Text(text.to_string()),
        display_text: None,
        timestamp,
    }))
}

fn assistant(timestamp: i64) -> AgentMessage {
    AgentMessage::standard(Message::Assistant(AssistantMessage {
        content: vec![ContentBlock::Text {
            text: "done".to_string(),
            signature: None,
        }],
        api: Api::OpenAIResponses,
        provider: Provider::OpenAI,
        model: "test".to_string(),
        response_model: None,
        response_id: None,
        usage: Usage::default(),
        stop_reason: StopReason::Stop,
        error_message: None,
        timestamp,
    }))
}

fn tool(before: &str, after: &str, timestamp: i64) -> AgentMessage {
    AgentMessage::standard(Message::ToolResult(ToolResultMessage {
        tool_call_id: format!("call-{timestamp}"),
        tool_name: "edit".to_string(),
        content: vec![],
        details: json!({
            "file_deltas": [{
                "path": "same.txt",
                "status": "modified",
                "before": before,
                "after": after,
                "patch": "",
                "added": 1,
                "deleted": 1,
                "truncated": false
            }],
            "capture_complete": true
        }),
        is_error: false,
        timestamp,
    }))
}

#[test]
fn repeated_edits_merge_within_a_turn_but_reset_for_the_next_turn() {
    let messages = vec![
        user("first", 1),
        tool("a", "b", 2),
        tool("b", "c", 3),
        assistant(4),
        user("second", 5),
        tool("c", "d", 6),
        assistant(7),
    ];
    let summaries = summarize_messages(&messages);

    assert_eq!(summaries.len(), 2);
    assert_eq!(
        summaries[0].activity.file_changes[0].before.as_deref(),
        Some("a")
    );
    assert_eq!(
        summaries[0].activity.file_changes[0].after.as_deref(),
        Some("c")
    );
    assert_eq!(
        summaries[1].activity.file_changes[0].before.as_deref(),
        Some("c")
    );
    assert_eq!(
        summaries[1].activity.file_changes[0].after.as_deref(),
        Some("d")
    );
    assert_eq!(summaries[0].assistant_message_index, 3);
    assert_eq!(summaries[1].assistant_message_index, 6);
}

#[test]
fn persisted_interaction_reconstructs_writes_before_summary_finalization() {
    let directory = tempfile::tempdir().unwrap();
    let mut manager = SessionManager::create_lazy(
        directory.path().join("session.jsonl"),
        "session".to_string(),
        directory.path().to_string_lossy().to_string(),
        None,
    );
    manager
        .append_custom(INTERACTION_STARTED.to_string(), None)
        .unwrap();
    manager
        .append_message(tool("before", "after", 1).as_standard().unwrap().clone())
        .unwrap();

    let activity = persisted_interaction_activity(&manager);
    assert_eq!(activity.changed_files, ["same.txt"]);
    assert_eq!(activity.file_changes[0].before.as_deref(), Some("before"));
    assert_eq!(activity.file_changes[0].after.as_deref(), Some("after"));
}
