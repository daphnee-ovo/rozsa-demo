use rozsa_app::subagent::{SubagentInfo, SubagentStatus};
use rozsa_model::types::ThinkingEffort;

#[test]
fn subagent_view_payload_keeps_runtime_identity_and_status() {
    let info = SubagentInfo {
        id: "subagent-1".to_string(),
        name: "reviewer".to_string(),
        status: SubagentStatus::Running,
        model_id: "test-model".to_string(),
        model_provider: "test".to_string(),
        thinking_effort: ThinkingEffort::Off,
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

#[test]
fn native_main_panel_starts_at_the_window_top() {
    let html = include_str!("../frontend/index.html");
    let native_main = html
        .split("body.native-split-main [data-od-id=\"app-body\"] {")
        .nth(1)
        .and_then(|tail| tail.split('}').next())
        .unwrap();

    assert!(native_main.contains("padding-top: 0"));
}
