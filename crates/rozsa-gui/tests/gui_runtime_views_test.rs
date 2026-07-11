use rozsa_app::subagent::{SubagentInfo, SubagentStatus};
use rozsa_model::types::ThinkingLevel;

#[test]
fn subagent_view_payload_keeps_runtime_identity_and_status() {
    let info = SubagentInfo {
        id: "subagent-1".to_string(),
        name: "reviewer".to_string(),
        status: SubagentStatus::Running,
        model_id: "test-model".to_string(),
        model_provider: "test".to_string(),
        thinking_level: ThinkingLevel::Off,
        created_at: 1,
        last_activity_at: 2,
        last_error: None,
        message_count: 3,
        session_file: None,
    };

    let value = serde_json::to_value(info).unwrap();
    assert_eq!(value["id"], "subagent-1");
    assert_eq!(value["status"], "running");
    assert_eq!(value["message_count"], 3);
}
