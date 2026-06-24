// File: view_model.rs
//
// Internal Framework:
// view_model.rs
// ├── message_to_view()       # AgentMessage → render-ready JSON Value
// ├── messages_to_view()      # 批量转换
// ├── user_to_view()          # Message::User → {role:"user", ...}
// ├── assistant_to_view()     # Message::Assistant → {role:"assistant", ...}
// ├── tool_result_to_view()   # Message::ToolResult → {role:"toolResult", ...}
// ├── custom_to_view()        # AgentMessage::Custom → {role:<type>, ...payload}
// └── content_block_to_view() # ContentBlock → content item Value
//
// 背景：
// rozsa-core 的 `AgentMessage` 用外部 tag + snake_case 序列化
// （`{"kind":"standard","message":{"User":{...}}}`），而 ui/render.rs 读的是
// TS 后端的扁平 camelCase 形状（顶层 `role` / `content` / `toolName` ...）。
// 直接 `serde_json::to_value(AgentMessage)` 会让 NativeBackend 的消息渲染成空白。
// 本模块做一次显式、稳定的视图模型转换，render.rs（1600 行，被 socket 路径共用）
// 完全不动。
//
// Related Docs:
// - [SPEC](../../../dev-doc/main/SPEC.md)
// - 对端 TS 形状：packages/coding-agent/src/core/messages.ts

use serde_json::{json, Value};

use rozsa_core::messages::AgentMessage;
use rozsa_model::types::{
    AssistantMessage, ContentBlock, Message, ToolResultMessage, UserContent, UserMessage,
};

/// Convert a batch of AgentMessages to render-ready JSON values.
pub fn messages_to_view(messages: &[AgentMessage]) -> Vec<Value> {
    messages.iter().map(message_to_view).collect()
}

/// Convert one AgentMessage to the flat camelCase shape ui/render.rs expects.
pub fn message_to_view(message: &AgentMessage) -> Value {
    match message {
        AgentMessage::Standard { message } => match message {
            Message::User(u) => user_to_view(u),
            Message::Assistant(a) => assistant_to_view(a),
            Message::ToolResult(t) => tool_result_to_view(t),
        },
        AgentMessage::Custom { message } => {
            custom_to_view(&message.message_type, &message.payload, message.timestamp)
        }
    }
}

fn user_to_view(u: &UserMessage) -> Value {
    let content = match &u.content {
        UserContent::Text(text) => json!([{ "type": "text", "text": text }]),
        UserContent::Blocks(blocks) => {
            Value::Array(blocks.iter().map(content_block_to_view).collect())
        }
    };
    let mut obj = json!({
        "role": "user",
        "content": content,
        "timestamp": u.timestamp,
    });
    if let Some(display) = &u.display_text {
        obj["displayText"] = json!(display);
    }
    obj
}

fn assistant_to_view(a: &AssistantMessage) -> Value {
    let content: Vec<Value> = a.content.iter().map(content_block_to_view).collect();
    json!({
        "role": "assistant",
        "content": content,
        "timestamp": a.timestamp,
    })
}

fn tool_result_to_view(t: &ToolResultMessage) -> Value {
    let content: Vec<Value> = t.content.iter().map(content_block_to_view).collect();
    json!({
        "role": "toolResult",
        "toolName": t.tool_name,
        "toolCallId": t.tool_call_id,
        "isError": t.is_error,
        "content": content,
        "timestamp": t.timestamp,
    })
}

/// Custom messages carry their type-specific fields in `payload`. The TS view
/// model flattens those fields to the top level alongside `role: <type>`
/// (bashExecution / custom / branchSummary / compactionSummary). We mirror that:
/// start from the payload object, then stamp `role` and `timestamp`.
fn custom_to_view(message_type: &str, payload: &Value, timestamp: i64) -> Value {
    let mut obj = match payload {
        Value::Object(map) => Value::Object(map.clone()),
        // Non-object payloads are unexpected for known custom types; wrap so
        // the renderer still sees a valid object instead of dropping it.
        other => json!({ "payload": other }),
    };
    obj["role"] = json!(message_type);
    // Only set timestamp if the payload didn't already carry one.
    if obj.get("timestamp").is_none() {
        obj["timestamp"] = json!(timestamp);
    }
    obj
}

fn content_block_to_view(block: &ContentBlock) -> Value {
    match block {
        ContentBlock::Text { text, .. } => json!({ "type": "text", "text": text }),
        ContentBlock::Thinking {
            thinking, redacted, ..
        } => json!({
            "type": "thinking",
            "thinking": thinking,
            "redacted": redacted,
        }),
        ContentBlock::Image { data, mime_type } => json!({
            "type": "image",
            // render.rs reads `source.data`; keep mimeType alongside for completeness.
            "source": { "data": data, "mimeType": mime_type },
        }),
        ContentBlock::ToolCall(call) => json!({
            "type": "toolCall",
            "id": call.id,
            "name": call.name,
            "arguments": call.arguments,
        }),
    }
}
