// view_model 转换单测 — 验证 AgentMessage → render-ready Value 的扁平 camelCase 形状。
//
// 重点：render.rs 读顶层 `role` / `content` / `toolName`，而非 AgentMessage 的
// 外部 tag (`kind`/`message`/`User`)。本测试钉住这个契约。

use rozsa_core::messages::AgentMessage;
use rozsa_model::types::{
    AssistantMessage, Api, ContentBlock, Message, Provider, StopReason, ToolCall,
    ToolResultMessage, Usage, UsageCost, UserContent, UserMessage,
};
use rozsa_tui::view_model::message_to_view;
use serde_json::json;

fn usage() -> Usage {
    Usage {
        input: 0,
        output: 0,
        cache_read: 0,
        cache_write: 0,
        total_tokens: 0,
        cost: UsageCost {
            input: 0.0,
            output: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
            total: 0.0,
        },
    }
}

#[test]
fn user_text_message_has_top_level_role_and_content() {
    let msg = AgentMessage::standard(Message::User(UserMessage {
        content: UserContent::Text("hello".to_string()),
        display_text: None,
        timestamp: 123,
    }));
    let v = message_to_view(&msg);
    assert_eq!(v["role"], "user");
    assert_eq!(v["content"][0]["type"], "text");
    assert_eq!(v["content"][0]["text"], "hello");
    assert_eq!(v["timestamp"], 123);
}

#[test]
fn user_display_text_is_preserved() {
    let msg = AgentMessage::standard(Message::User(UserMessage {
        content: UserContent::Text("/expanded".to_string()),
        display_text: Some("/raw".to_string()),
        timestamp: 1,
    }));
    let v = message_to_view(&msg);
    assert_eq!(v["displayText"], "/raw");
}

#[test]
fn assistant_message_renders_content_blocks() {
    let msg = AgentMessage::standard(Message::Assistant(AssistantMessage {
        content: vec![
            ContentBlock::Text {
                text: "thinking out loud".to_string(),
                signature: None,
            },
            ContentBlock::ToolCall(ToolCall {
                id: "call_1".to_string(),
                name: "bash".to_string(),
                arguments: json!({ "command": "ls" }),
            }),
        ],
        api: Api::OpenAIResponses,
        provider: Provider::OpenAI,
        model: "test".to_string(),
        response_model: None,
        response_id: None,
        usage: usage(),
        stop_reason: StopReason::Stop,
        error_message: None,
        timestamp: 5,
    }));
    let v = message_to_view(&msg);
    assert_eq!(v["role"], "assistant");
    assert_eq!(v["content"][0]["type"], "text");
    assert_eq!(v["content"][1]["type"], "toolCall");
    assert_eq!(v["content"][1]["name"], "bash");
    assert_eq!(v["content"][1]["arguments"]["command"], "ls");
}

#[test]
fn tool_result_uses_camel_case_tool_name() {
    let msg = AgentMessage::standard(Message::ToolResult(ToolResultMessage {
        tool_call_id: "call_1".to_string(),
        tool_name: "bash".to_string(),
        content: vec![ContentBlock::Text {
            text: "output".to_string(),
            signature: None,
        }],
        is_error: false,
        timestamp: 7,
    }));
    let v = message_to_view(&msg);
    assert_eq!(v["role"], "toolResult");
    assert_eq!(v["toolName"], "bash");
    assert_eq!(v["isError"], false);
    assert_eq!(v["content"][0]["text"], "output");
}

#[test]
fn custom_message_flattens_payload_with_role() {
    let msg = AgentMessage::custom(
        "bashExecution".to_string(),
        json!({
            "command": "echo hi",
            "output": "hi",
            "exitCode": 0,
            "cancelled": false,
        }),
        99,
    );
    let v = message_to_view(&msg);
    assert_eq!(v["role"], "bashExecution");
    assert_eq!(v["command"], "echo hi");
    assert_eq!(v["output"], "hi");
    assert_eq!(v["exitCode"], 0);
    // timestamp falls back to the AgentMessage timestamp when payload omits it.
    assert_eq!(v["timestamp"], 99);
}

#[test]
fn custom_payload_timestamp_is_not_overwritten() {
    let msg = AgentMessage::custom(
        "compactionSummary".to_string(),
        json!({ "summary": "did stuff", "timestamp": 42 }),
        99,
    );
    let v = message_to_view(&msg);
    assert_eq!(v["role"], "compactionSummary");
    assert_eq!(v["summary"], "did stuff");
    assert_eq!(v["timestamp"], 42);
}
