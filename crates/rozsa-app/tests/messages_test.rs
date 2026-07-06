use rozsa_app::messages::{AppMessage, BashExecutionMessage};
use rozsa_core::messages::AgentMessage;

#[test]
fn test_app_message_serialization() {
    let msg = AppMessage::status("test", true);
    let json = serde_json::to_value(&msg).unwrap();
    assert_eq!(json["type"], "status");
}

#[test]
fn test_app_message_to_agent_message() {
    let app_msg = AppMessage::compaction("Summary".to_string(), 10, 1000);
    let agent_msg: AgentMessage = app_msg.into();
    match agent_msg {
        AgentMessage::Custom { message } => {
            assert_eq!(message.message_type, "compaction");
        }
        _ => panic!("Expected custom message"),
    }
}

#[test]
fn test_bash_execution_message() {
    let bash = BashExecutionMessage::new("ls -la".to_string(), "file1\nfile2".to_string(), Some(0));
    assert_eq!(bash.command, "ls -la");
    assert!(!bash.exclude_from_context);

    let agent_msg = bash.to_agent_message();
    match agent_msg {
        AgentMessage::Custom { message } => {
            assert_eq!(message.message_type, "bash_execution");
        }
        _ => panic!("Expected custom message"),
    }
}

#[test]
fn test_model_change_message() {
    let msg = AppMessage::model_change(
        Some(("anthropic".to_string(), "claude-3-opus".to_string())),
        ("anthropic".to_string(), "claude-3-sonnet".to_string()),
    );

    match msg {
        AppMessage::ModelChange(m) => {
            assert_eq!(m.to_model.provider, "anthropic");
            assert_eq!(m.to_model.id, "claude-3-sonnet");
            assert!(m.from_model.is_some());
        }
        _ => panic!("Expected ModelChange variant"),
    }
}
