use rozsa_core::messages::AgentMessage;
use rozsa_model::types::*;

#[test]
fn dump_serialized_message_formats() {
    let user = AgentMessage::standard(Message::User(UserMessage {
        content: UserContent::Text("hello world".to_string()),
        display_text: None,
        timestamp: 1000,
    }));
    eprintln!("USER:\n{}\n", serde_json::to_string_pretty(&user).unwrap());

    let assistant = AgentMessage::standard(Message::Assistant(AssistantMessage {
        content: vec![
            ContentBlock::Thinking {
                thinking: "let me think".to_string(),
                signature: None,
                redacted: false,
            },
            ContentBlock::Text {
                text: "response text".to_string(),
                signature: None,
            },
            ContentBlock::ToolCall(ToolCall {
                id: "tc1".to_string(),
                name: "Bash".to_string(),
                arguments: serde_json::json!({"command": "ls"}),
            }),
        ],
        api: Api::OpenAICompletions,
        provider: Provider::OpenAI,
        model: "gpt-4o".to_string(),
        response_model: None,
        response_id: None,
        usage: Usage::default(),
        stop_reason: StopReason::Stop,
        error_message: None,
        timestamp: 2000,
    }));
    eprintln!("ASSISTANT:\n{}\n", serde_json::to_string_pretty(&assistant).unwrap());

    let tool_result = AgentMessage::standard(Message::ToolResult(ToolResultMessage {
        tool_call_id: "tc1".to_string(),
        tool_name: "Bash".to_string(),
        content: vec![ContentBlock::Text {
            text: "output here".to_string(),
            signature: None,
        }],
        details: serde_json::Value::Null,
        is_error: false,
        timestamp: 3000,
    }));
    eprintln!("TOOL_RESULT:\n{}\n", serde_json::to_string_pretty(&tool_result).unwrap());
}
