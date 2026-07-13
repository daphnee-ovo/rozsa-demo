use rozsa_model::types::{
    Api, AssistantMessage, ContentBlock, Message, Provider, StopReason, ToolResultMessage, Usage,
    UsageCost, UserContent, UserMessage,
};
use serde_json::json;

#[test]
fn content_block_text_roundtrip() {
    let block = ContentBlock::Text {
        text: "hello".to_string(),
        signature: None,
    };
    let json = serde_json::to_value(&block).unwrap();
    assert_eq!(json, json!({"type": "text", "text": "hello"}));
    let back: ContentBlock = serde_json::from_value(json).unwrap();
    match back {
        ContentBlock::Text { text, signature } => {
            assert_eq!(text, "hello");
            assert!(signature.is_none());
        }
        _ => panic!("expected Text"),
    }
}

#[test]
fn content_block_thinking_roundtrip() {
    let block = ContentBlock::Thinking {
        thinking: "hmm".to_string(),
        signature: Some("sig123".to_string()),
        redacted: true,
    };
    let json = serde_json::to_value(&block).unwrap();
    assert_eq!(json["type"], "thinking");
    assert_eq!(json["thinking"], "hmm");
    assert_eq!(json["thinkingSignature"], "sig123");
    assert_eq!(json["redacted"], true);
    let back: ContentBlock = serde_json::from_value(json).unwrap();
    match back {
        ContentBlock::Thinking {
            thinking,
            signature,
            redacted,
        } => {
            assert_eq!(thinking, "hmm");
            assert_eq!(signature, Some("sig123".to_string()));
            assert!(redacted);
        }
        _ => panic!("expected Thinking"),
    }
}

#[test]
fn user_content_text_serializes_as_array() {
    let content = UserContent::Text("hello".to_string());
    let json = serde_json::to_value(&content).unwrap();
    assert_eq!(json, json!([{"type": "text", "text": "hello"}]));
}

#[test]
fn user_content_deserialize_string() {
    let json = json!("plain text");
    let content: UserContent = serde_json::from_value(json).unwrap();
    match content {
        UserContent::Text(t) => assert_eq!(t, "plain text"),
        _ => panic!("expected Text"),
    }
}

#[test]
fn user_content_deserialize_array() {
    let json = json!([{"type": "text", "text": "hi"}]);
    let content: UserContent = serde_json::from_value(json).unwrap();
    match content {
        UserContent::Blocks(blocks) => {
            assert_eq!(blocks.len(), 1);
        }
        _ => panic!("expected Blocks"),
    }
}

#[test]
fn message_user_roundtrip() {
    let msg = Message::User(UserMessage {
        content: UserContent::Text("hello".to_string()),
        display_text: None,
        timestamp: 1234,
    });
    let json = serde_json::to_value(&msg).unwrap();
    assert_eq!(json["role"], "user");
    assert_eq!(json["content"], json!([{"type": "text", "text": "hello"}]));
    assert_eq!(json["timestamp"], 1234);
    let back: Message = serde_json::from_value(json).unwrap();
    match back {
        Message::User(u) => {
            assert_eq!(u.timestamp, 1234);
        }
        _ => panic!("expected User"),
    }
}

#[test]
fn message_assistant_roundtrip() {
    let msg = Message::Assistant(AssistantMessage {
        content: vec![ContentBlock::Text {
            text: "hi".to_string(),
            signature: None,
        }],
        api: Api::AnthropicMessages,
        provider: Provider::Anthropic,
        model: "claude-3".to_string(),
        response_model: None,
        response_id: None,
        usage: Usage {
            input: 10,
            output: 20,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 30,
            cost: UsageCost {
                input: 0.0,
                output: 0.0,
                cache_read: 0.0,
                cache_write: 0.0,
                total: 0.0,
            },
        },
        stop_reason: StopReason::Stop,
        error_message: None,
        timestamp: 5678,
    });
    let json = serde_json::to_value(&msg).unwrap();
    assert_eq!(json["role"], "assistant");
    assert_eq!(json["model"], "claude-3");
    assert_eq!(json["stopReason"], "stop");
    assert_eq!(json["usage"]["input"], 10);
    assert_eq!(json["usage"]["cacheRead"], 0);
    let back: Message = serde_json::from_value(json).unwrap();
    match back {
        Message::Assistant(a) => {
            assert_eq!(a.model, "claude-3");
            assert_eq!(a.usage.input, 10);
        }
        _ => panic!("expected Assistant"),
    }
}

#[test]
fn message_tool_result_roundtrip() {
    let msg = Message::ToolResult(ToolResultMessage {
        tool_call_id: "tc_1".to_string(),
        tool_name: "read".to_string(),
        content: vec![],
        details: serde_json::json!({
            "file_deltas": [{"path": "poem.md", "after": "written content"}]
        }),
        is_error: false,
        timestamp: 111,
    });
    let json = serde_json::to_value(&msg).unwrap();
    assert_eq!(json["role"], "toolResult");
    assert_eq!(json["toolCallId"], "tc_1");
    assert_eq!(
        json["details"]["file_deltas"][0]["after"],
        "written content"
    );
    let back: Message = serde_json::from_value(json).unwrap();
    match back {
        Message::ToolResult(tr) => {
            assert_eq!(tr.tool_call_id, "tc_1");
            assert_eq!(tr.details["file_deltas"][0]["after"], "written content");
        }
        _ => panic!("expected ToolResult"),
    }
}
