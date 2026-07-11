use rozsa_core::events::AgentEvent;
use rozsa_core::messages::AgentMessage;
use rozsa_gui::state::LiveState;
use rozsa_model::types::{Message, ToolResultMessage, UserContent, UserMessage};
use serde_json::json;

#[test]
fn queue_is_fifo_and_keeps_running_until_the_next_prompt_starts() {
    let mut state = LiveState::default();
    state.apply(&AgentEvent::AgentStart);
    state.enqueue_message("first queued message".to_string());
    state.enqueue_message("second queued message".to_string());

    state.apply(&AgentEvent::AgentEnd {
        messages: Vec::<AgentMessage>::new(),
    });

    assert!(state.is_streaming);
    assert_eq!(
        state.queued_messages,
        ["first queued message", "second queued message"]
    );
    assert_eq!(
        state.take_next_queued_message().as_deref(),
        Some("first queued message")
    );
    assert_eq!(
        state.take_next_queued_message().as_deref(),
        Some("second queued message")
    );
    assert_eq!(state.take_next_queued_message(), None);
}

#[test]
fn steer_leaves_the_waiting_panel_when_its_user_message_is_delivered() {
    let mut state = LiveState::default();
    state.apply(&AgentEvent::AgentStart);
    state.add_steering_message("prefer the smallest patch".to_string());

    let delivered = AgentMessage::standard(Message::User(UserMessage {
        content: UserContent::Text("prefer the smallest patch".to_string()),
        display_text: Some("[steer] prefer the smallest patch".to_string()),
        timestamp: 0,
    }));
    state.apply(&AgentEvent::MessageStart {
        message: delivered.clone(),
    });
    state.apply(&AgentEvent::MessageEnd { message: delivered });

    assert!(state.steering_conversation.is_empty());
    assert!(state.messages.iter().any(|message| {
        matches!(
            message.as_standard(),
            Some(Message::User(user)) if user.content.text() == "prefer the smallest patch"
        )
    }));
}

#[test]
fn queued_agent_cycles_share_one_interaction_activity() {
    let mut state = LiveState::default();
    state.begin_interaction();
    state.apply(&AgentEvent::AgentStart);
    state.apply(&AgentEvent::ToolExecutionEnd {
        tool_call_id: "write-1".to_string(),
        tool_name: "write".to_string(),
        result: ToolResultMessage {
            tool_call_id: "write-1".to_string(),
            tool_name: "write".to_string(),
            content: vec![],
            details: json!({
                "file_deltas": [{
                    "path": "poem.md",
                    "status": "added",
                    "before": null,
                    "after": "first",
                    "patch": "",
                    "added": 1,
                    "deleted": 0,
                    "truncated": false
                }]
            }),
            is_error: false,
            timestamp: 0,
        },
    });
    state.enqueue_message("continue".to_string());
    state.apply(&AgentEvent::AgentEnd { messages: vec![] });
    assert!(state.is_streaming);

    assert_eq!(
        state.take_next_queued_message().as_deref(),
        Some("continue")
    );
    state.apply(&AgentEvent::AgentStart);
    assert_eq!(state.turn_activity.changed_files, ["poem.md"]);

    state.apply(&AgentEvent::AgentEnd { messages: vec![] });
    let summary = state.turn_activity.clone();
    state.finish_interaction(summary.clone());
    assert_eq!(summary.changed_files, ["poem.md"]);
    assert_eq!(state.completed_summary, Some(summary));
}
