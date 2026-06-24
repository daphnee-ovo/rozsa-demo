//! 自定义 Serialize/Deserialize 实现，将 ContentBlock、UserContent、Message
//! 序列化为 TS 兼容的线格式，并支持双格式反序列化（兼容旧 Rust 格式）。
//!
//! 相关文档:
//! - [protocol.rs](./protocol.rs) — 桥接层转换逻辑

use serde::de;
use serde::ser::SerializeMap;
use serde::{Deserializer, Serializer};
use serde_json::Value;

use crate::providers::common::provider_id;
use crate::types::{
    Api, AssistantMessage, ContentBlock, Message, Provider, StopReason, ToolCall,
    ToolResultMessage, Usage, UsageCost, UserContent, UserMessage,
};

// ─── ContentBlock Serialize ─────────────────────────────────────────────────

impl serde::Serialize for ContentBlock {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            ContentBlock::Text { text, signature } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("type", "text")?;
                map.serialize_entry("text", text)?;
                if let Some(sig) = signature {
                    map.serialize_entry("textSignature", sig)?;
                }
                map.end()
            }
            ContentBlock::Thinking {
                thinking,
                signature,
                redacted,
            } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("type", "thinking")?;
                map.serialize_entry("thinking", thinking)?;
                if let Some(sig) = signature {
                    map.serialize_entry("thinkingSignature", sig)?;
                }
                if *redacted {
                    map.serialize_entry("redacted", &true)?;
                }
                map.end()
            }
            ContentBlock::Image { data, mime_type } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("type", "image")?;
                map.serialize_entry("data", data)?;
                map.serialize_entry("mimeType", mime_type)?;
                map.end()
            }
            ContentBlock::ToolCall(tc) => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("type", "toolCall")?;
                map.serialize_entry("id", &tc.id)?;
                map.serialize_entry("name", &tc.name)?;
                map.serialize_entry("arguments", &tc.arguments)?;
                map.end()
            }
        }
    }
}

// ─── ContentBlock Deserialize ───────────────────────────────────────────────

impl<'de> serde::Deserialize<'de> for ContentBlock {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value: Value = Value::deserialize(deserializer)?;
        deserialize_content_block(&value).map_err(de::Error::custom)
    }
}

fn deserialize_content_block(value: &Value) -> Result<ContentBlock, String> {
    if let Some(typ) = value.get("type").and_then(Value::as_str) {
        return match typ {
            "text" => Ok(ContentBlock::Text {
                text: str_field(value, "text")?,
                signature: opt_str(value, "textSignature"),
            }),
            "thinking" => Ok(ContentBlock::Thinking {
                thinking: str_field(value, "thinking")?,
                signature: opt_str(value, "thinkingSignature"),
                redacted: value.get("redacted").and_then(Value::as_bool).unwrap_or(false),
            }),
            "image" => Ok(ContentBlock::Image {
                data: str_field(value, "data")?,
                mime_type: str_field(value, "mimeType")?,
            }),
            "toolCall" => Ok(ContentBlock::ToolCall(ToolCall {
                id: str_field(value, "id")?,
                name: str_field(value, "name")?,
                arguments: value.get("arguments").cloned().unwrap_or(Value::Object(Default::default())),
            })),
            _ => Err(format!("unknown content block type: {typ}")),
        };
    }

    // 旧 Rust 格式
    if let Some(inner) = value.get("Text") {
        return Ok(ContentBlock::Text {
            text: str_field(inner, "text")?,
            signature: opt_str(inner, "signature"),
        });
    }
    if let Some(inner) = value.get("Thinking") {
        return Ok(ContentBlock::Thinking {
            thinking: str_field(inner, "thinking")?,
            signature: opt_str(inner, "signature"),
            redacted: inner.get("redacted").and_then(Value::as_bool).unwrap_or(false),
        });
    }
    if let Some(inner) = value.get("Image") {
        return Ok(ContentBlock::Image {
            data: str_field(inner, "data")?,
            mime_type: str_field(inner, "mime_type")?,
        });
    }
    if let Some(inner) = value.get("ToolCall") {
        let tc: ToolCall = serde_json::from_value(inner.clone())
            .map_err(|e| format!("failed to parse ToolCall: {e}"))?;
        return Ok(ContentBlock::ToolCall(tc));
    }

    Err(format!("cannot parse ContentBlock from: {value}"))
}

// ─── UserContent Serialize ──────────────────────────────────────────────────

impl serde::Serialize for UserContent {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            UserContent::Text(text) => {
                let block = ContentBlock::Text {
                    text: text.clone(),
                    signature: None,
                };
                let blocks = [&block];
                blocks.serialize(serializer)
            }
            UserContent::Blocks(blocks) => blocks.serialize(serializer),
        }
    }
}

// ─── UserContent Deserialize ────────────────────────────────────────────────

impl<'de> serde::Deserialize<'de> for UserContent {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value: Value = Value::deserialize(deserializer)?;
        deserialize_user_content(&value).map_err(de::Error::custom)
    }
}

fn deserialize_user_content(value: &Value) -> Result<UserContent, String> {
    if let Some(text) = value.as_str() {
        return Ok(UserContent::Text(text.to_string()));
    }
    if let Some(arr) = value.as_array() {
        let blocks: Result<Vec<ContentBlock>, _> = arr.iter().map(deserialize_content_block).collect();
        return Ok(UserContent::Blocks(blocks?));
    }
    // 旧 Rust 格式
    if let Some(Value::String(text)) = value.get("Text") {
        return Ok(UserContent::Text(text.clone()));
    }
    if let Some(arr) = value.get("Blocks").and_then(Value::as_array) {
        let blocks: Result<Vec<ContentBlock>, _> = arr.iter().map(deserialize_content_block).collect();
        return Ok(UserContent::Blocks(blocks?));
    }
    Err(format!("cannot parse UserContent from: {value}"))
}

// ─── Message Serialize ──────────────────────────────────────────────────────

impl serde::Serialize for Message {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Message::User(msg) => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("role", "user")?;
                map.serialize_entry("content", &msg.content)?;
                if let Some(ref dt) = msg.display_text {
                    map.serialize_entry("displayText", dt)?;
                }
                map.serialize_entry("timestamp", &msg.timestamp)?;
                map.end()
            }
            Message::Assistant(msg) => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("role", "assistant")?;
                map.serialize_entry("content", &msg.content)?;
                map.serialize_entry("api", &api_to_str(&msg.api))?;
                map.serialize_entry("provider", &provider_to_str(&msg.provider))?;
                map.serialize_entry("model", &msg.model)?;
                if let Some(ref rm) = msg.response_model {
                    map.serialize_entry("responseModel", rm)?;
                }
                if let Some(ref rid) = msg.response_id {
                    map.serialize_entry("responseId", rid)?;
                }
                map.serialize_entry("usage", &UsageWire::from(&msg.usage))?;
                map.serialize_entry("stopReason", &stop_reason_str(msg.stop_reason))?;
                if let Some(ref em) = msg.error_message {
                    map.serialize_entry("errorMessage", em)?;
                }
                map.serialize_entry("timestamp", &msg.timestamp)?;
                map.end()
            }
            Message::ToolResult(msg) => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("role", "toolResult")?;
                map.serialize_entry("toolCallId", &msg.tool_call_id)?;
                map.serialize_entry("toolName", &msg.tool_name)?;
                map.serialize_entry("content", &msg.content)?;
                map.serialize_entry("isError", &msg.is_error)?;
                map.serialize_entry("timestamp", &msg.timestamp)?;
                map.end()
            }
        }
    }
}

// ─── Message Deserialize ────────────────────────────────────────────────────

impl<'de> serde::Deserialize<'de> for Message {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value: Value = Value::deserialize(deserializer)?;
        deserialize_message(&value).map_err(de::Error::custom)
    }
}

fn deserialize_message(value: &Value) -> Result<Message, String> {
    // TS 格式: {"role":"user",...}
    if let Some(role) = value.get("role").and_then(Value::as_str) {
        return match role {
            "user" => {
                let content = value.get("content")
                    .ok_or("user message missing content")?;
                Ok(Message::User(UserMessage {
                    content: deserialize_user_content(content)?,
                    display_text: opt_str(value, "displayText"),
                    timestamp: value.get("timestamp").and_then(Value::as_i64).unwrap_or(0),
                }))
            }
            "assistant" => Ok(Message::Assistant(deserialize_assistant(value)?)),
            "toolResult" => Ok(Message::ToolResult(ToolResultMessage {
                tool_call_id: str_field(value, "toolCallId")?,
                tool_name: str_field(value, "toolName")?,
                content: deserialize_content_blocks(value.get("content"))?,
                is_error: value.get("isError").and_then(Value::as_bool).unwrap_or(false),
                timestamp: value.get("timestamp").and_then(Value::as_i64).unwrap_or(0),
            })),
            _ => Err(format!("unknown message role: {role}")),
        };
    }

    // 旧 Rust 格式
    if let Some(inner) = value.get("User") {
        let content = inner.get("content")
            .ok_or("User message missing content")?;
        return Ok(Message::User(UserMessage {
            content: deserialize_user_content(content)?,
            display_text: opt_str(inner, "display_text"),
            timestamp: inner.get("timestamp").and_then(Value::as_i64).unwrap_or(0),
        }));
    }
    if let Some(inner) = value.get("Assistant") {
        return Ok(Message::Assistant(deserialize_assistant_legacy(inner)?));
    }
    if let Some(inner) = value.get("ToolResult") {
        return Ok(Message::ToolResult(ToolResultMessage {
            tool_call_id: str_field(inner, "tool_call_id")?,
            tool_name: str_field(inner, "tool_name")?,
            content: deserialize_content_blocks(inner.get("content"))?,
            is_error: inner.get("is_error").and_then(Value::as_bool).unwrap_or(false),
            timestamp: inner.get("timestamp").and_then(Value::as_i64).unwrap_or(0),
        }));
    }

    Err(format!("cannot parse Message from: {value}"))
}

fn deserialize_assistant(value: &Value) -> Result<AssistantMessage, String> {
    Ok(AssistantMessage {
        content: deserialize_content_blocks(value.get("content"))?,
        api: parse_api(value.get("api").and_then(Value::as_str).unwrap_or("anthropic-messages")),
        provider: parse_provider(value.get("provider").and_then(Value::as_str).unwrap_or("anthropic")),
        model: str_field(value, "model")?,
        response_model: opt_str(value, "responseModel"),
        response_id: opt_str(value, "responseId"),
        usage: deserialize_usage(value.get("usage")),
        stop_reason: value.get("stopReason").and_then(Value::as_str)
            .map(parse_stop_reason).unwrap_or(StopReason::Stop),
        error_message: opt_str(value, "errorMessage"),
        timestamp: value.get("timestamp").and_then(Value::as_i64).unwrap_or(0),
    })
}

fn deserialize_assistant_legacy(value: &Value) -> Result<AssistantMessage, String> {
    Ok(AssistantMessage {
        content: deserialize_content_blocks(value.get("content"))?,
        api: deserialize_api_field(value.get("api")),
        provider: deserialize_provider_field(value.get("provider")),
        model: str_field(value, "model")?,
        response_model: opt_str(value, "response_model"),
        response_id: opt_str(value, "response_id"),
        usage: deserialize_usage_legacy(value.get("usage")),
        stop_reason: deserialize_stop_reason_field(value.get("stop_reason")),
        error_message: opt_str(value, "error_message"),
        timestamp: value.get("timestamp").and_then(Value::as_i64).unwrap_or(0),
    })
}

fn deserialize_content_blocks(value: Option<&Value>) -> Result<Vec<ContentBlock>, String> {
    match value {
        Some(Value::Array(arr)) => arr.iter().map(deserialize_content_block).collect(),
        Some(_) => Err("content must be an array".to_string()),
        None => Ok(Vec::new()),
    }
}

fn deserialize_usage(value: Option<&Value>) -> Usage {
    let input = value.and_then(|v| v.get("input")).and_then(Value::as_u64).unwrap_or(0);
    let output = value.and_then(|v| v.get("output")).and_then(Value::as_u64).unwrap_or(0);
    let cache_read = value.and_then(|v| v.get("cacheRead")).and_then(Value::as_u64).unwrap_or(0);
    let cache_write = value.and_then(|v| v.get("cacheWrite")).and_then(Value::as_u64).unwrap_or(0);
    let total_tokens = value.and_then(|v| v.get("totalTokens")).and_then(Value::as_u64)
        .unwrap_or(input + output + cache_read + cache_write);
    let cost = value.and_then(|v| v.get("cost")).map(|c| UsageCost {
        input: c.get("input").and_then(Value::as_f64).unwrap_or(0.0),
        output: c.get("output").and_then(Value::as_f64).unwrap_or(0.0),
        cache_read: c.get("cacheRead").and_then(Value::as_f64).unwrap_or(0.0),
        cache_write: c.get("cacheWrite").and_then(Value::as_f64).unwrap_or(0.0),
        total: c.get("total").and_then(Value::as_f64).unwrap_or(0.0),
    }).unwrap_or(UsageCost { input: 0.0, output: 0.0, cache_read: 0.0, cache_write: 0.0, total: 0.0 });
    Usage { input, output, cache_read, cache_write, total_tokens, cost }
}

fn deserialize_usage_legacy(value: Option<&Value>) -> Usage {
    let input = value.and_then(|v| v.get("input")).and_then(Value::as_u64).unwrap_or(0);
    let output = value.and_then(|v| v.get("output")).and_then(Value::as_u64).unwrap_or(0);
    let cache_read = value.and_then(|v| v.get("cache_read")).and_then(Value::as_u64).unwrap_or(0);
    let cache_write = value.and_then(|v| v.get("cache_write")).and_then(Value::as_u64).unwrap_or(0);
    let total_tokens = value.and_then(|v| v.get("total_tokens")).and_then(Value::as_u64)
        .unwrap_or(input + output + cache_read + cache_write);
    let cost = value.and_then(|v| v.get("cost")).map(|c| UsageCost {
        input: c.get("input").and_then(Value::as_f64).unwrap_or(0.0),
        output: c.get("output").and_then(Value::as_f64).unwrap_or(0.0),
        cache_read: c.get("cache_read").and_then(Value::as_f64).unwrap_or(0.0),
        cache_write: c.get("cache_write").and_then(Value::as_f64).unwrap_or(0.0),
        total: c.get("total").and_then(Value::as_f64).unwrap_or(0.0),
    }).unwrap_or(UsageCost { input: 0.0, output: 0.0, cache_read: 0.0, cache_write: 0.0, total: 0.0 });
    Usage { input, output, cache_read, cache_write, total_tokens, cost }
}

fn deserialize_api_field(value: Option<&Value>) -> Api {
    match value {
        Some(Value::String(s)) => parse_api(s),
        Some(Value::Object(map)) => {
            if let Some(key) = map.keys().next() {
                parse_api_legacy(key)
            } else {
                Api::AnthropicMessages
            }
        }
        _ => Api::AnthropicMessages,
    }
}

fn deserialize_provider_field(value: Option<&Value>) -> Provider {
    match value {
        Some(Value::String(s)) => parse_provider(s),
        Some(Value::Object(map)) => {
            if let Some(key) = map.keys().next() {
                parse_provider_legacy(key)
            } else {
                Provider::Anthropic
            }
        }
        _ => Provider::Anthropic,
    }
}

fn deserialize_stop_reason_field(value: Option<&Value>) -> StopReason {
    match value {
        Some(Value::String(s)) => parse_stop_reason(s),
        _ => StopReason::Stop,
    }
}

// ─── Usage Serialize (camelCase wire format) ────────────────────────────────

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct UsageWire {
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write: u64,
    total_tokens: u64,
    cost: UsageCostWire,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct UsageCostWire {
    input: f64,
    output: f64,
    cache_read: f64,
    cache_write: f64,
    total: f64,
}

impl From<&Usage> for UsageWire {
    fn from(u: &Usage) -> Self {
        UsageWire {
            input: u.input,
            output: u.output,
            cache_read: u.cache_read,
            cache_write: u.cache_write,
            total_tokens: u.total_tokens,
            cost: UsageCostWire {
                input: u.cost.input,
                output: u.cost.output,
                cache_read: u.cost.cache_read,
                cache_write: u.cost.cache_write,
                total: u.cost.total,
            },
        }
    }
}

// ─── 辅助函数 ───────────────────────────────────────────────────────────────

fn api_to_str(api: &Api) -> &str {
    match api {
        Api::AnthropicMessages => "anthropic-messages",
        Api::OpenAICompletions => "openai-completions",
        Api::OpenAIResponses => "openai-responses",
        Api::BedrockConverseStream => "bedrock-converse-stream",
        Api::GoogleGenerativeAI => "google-generative-ai",
        Api::GoogleVertex => "google-vertex",
        Api::MistralConversations => "mistral-conversations",
        Api::Custom(s) => s,
    }
}

fn provider_to_str(provider: &Provider) -> String {
    provider_id(provider)
}

fn stop_reason_str(reason: StopReason) -> &'static str {
    match reason {
        StopReason::Stop => "stop",
        StopReason::Length => "length",
        StopReason::ToolUse => "toolUse",
        StopReason::Error => "error",
        StopReason::Aborted => "aborted",
    }
}

fn parse_api(value: &str) -> Api {
    match value {
        "anthropic-messages" => Api::AnthropicMessages,
        "openai-completions" => Api::OpenAICompletions,
        "openai-responses" => Api::OpenAIResponses,
        "bedrock-converse-stream" => Api::BedrockConverseStream,
        "google-generative-ai" => Api::GoogleGenerativeAI,
        "google-vertex" => Api::GoogleVertex,
        "mistral-conversations" => Api::MistralConversations,
        other => Api::Custom(other.to_string()),
    }
}

fn parse_api_legacy(value: &str) -> Api {
    match value {
        "AnthropicMessages" => Api::AnthropicMessages,
        "OpenAICompletions" => Api::OpenAICompletions,
        "OpenAIResponses" => Api::OpenAIResponses,
        "BedrockConverseStream" => Api::BedrockConverseStream,
        "GoogleGenerativeAI" => Api::GoogleGenerativeAI,
        "GoogleVertex" => Api::GoogleVertex,
        "MistralConversations" => Api::MistralConversations,
        other => Api::Custom(other.to_string()),
    }
}

fn parse_provider(value: &str) -> Provider {
    match value {
        "anthropic" => Provider::Anthropic,
        "openai" => Provider::OpenAI,
        "amazon-bedrock" => Provider::AmazonBedrock,
        "google" => Provider::Google,
        "google-vertex" => Provider::GoogleVertex,
        "deepseek" => Provider::DeepSeek,
        "openrouter" => Provider::OpenRouter,
        "xai" => Provider::XAI,
        "groq" => Provider::Groq,
        "cerebras" => Provider::Cerebras,
        "mistral" => Provider::Mistral,
        "nvidia" => Provider::Nvidia,
        "zai" => Provider::Zai,
        "together" => Provider::Together,
        "moonshotai" => Provider::MoonshotAI,
        "moonshotai-cn" => Provider::MoonshotAICn,
        "huggingface" => Provider::HuggingFace,
        "cloudflare-workers-ai" => Provider::CloudflareWorkersAI,
        "cloudflare-ai-gateway" => Provider::CloudflareAIGateway,
        "xiaomi" => Provider::Xiaomi,
        "xiaomi-token-plan-cn" => Provider::XiaomiTokenPlanCn,
        "xiaomi-token-plan-ams" => Provider::XiaomiTokenPlanAms,
        "xiaomi-token-plan-sgp" => Provider::XiaomiTokenPlanSgp,
        other => Provider::Custom(other.to_string()),
    }
}

fn parse_provider_legacy(value: &str) -> Provider {
    match value {
        "Anthropic" => Provider::Anthropic,
        "OpenAI" => Provider::OpenAI,
        "AmazonBedrock" => Provider::AmazonBedrock,
        "Google" => Provider::Google,
        "GoogleVertex" => Provider::GoogleVertex,
        "DeepSeek" => Provider::DeepSeek,
        "OpenRouter" => Provider::OpenRouter,
        "XAI" => Provider::XAI,
        "Groq" => Provider::Groq,
        "Cerebras" => Provider::Cerebras,
        "Mistral" => Provider::Mistral,
        "Nvidia" => Provider::Nvidia,
        "Zai" => Provider::Zai,
        "Together" => Provider::Together,
        "MoonshotAI" => Provider::MoonshotAI,
        "MoonshotAICn" => Provider::MoonshotAICn,
        "HuggingFace" => Provider::HuggingFace,
        "CloudflareWorkersAI" => Provider::CloudflareWorkersAI,
        "CloudflareAIGateway" => Provider::CloudflareAIGateway,
        "Xiaomi" => Provider::Xiaomi,
        "XiaomiTokenPlanCn" => Provider::XiaomiTokenPlanCn,
        "XiaomiTokenPlanAms" => Provider::XiaomiTokenPlanAms,
        "XiaomiTokenPlanSgp" => Provider::XiaomiTokenPlanSgp,
        other => Provider::Custom(other.to_string()),
    }
}

fn parse_stop_reason(value: &str) -> StopReason {
    match value {
        "stop" | "Stop" => StopReason::Stop,
        "length" | "Length" => StopReason::Length,
        "toolUse" | "ToolUse" => StopReason::ToolUse,
        "error" | "Error" => StopReason::Error,
        "aborted" | "Aborted" => StopReason::Aborted,
        _ => StopReason::Stop,
    }
}

fn str_field(value: &Value, field: &str) -> Result<String, String> {
    value.get(field)
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| format!("missing or invalid field: {field}"))
}

fn opt_str(value: &Value, field: &str) -> Option<String> {
    value.get(field).and_then(Value::as_str).map(ToString::to_string)
}

// ─── 测试 ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
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
            ContentBlock::Thinking { thinking, signature, redacted } => {
                assert_eq!(thinking, "hmm");
                assert_eq!(signature, Some("sig123".to_string()));
                assert!(redacted);
            }
            _ => panic!("expected Thinking"),
        }
    }

    #[test]
    fn content_block_legacy_format() {
        let legacy = json!({"Text": {"text": "hi", "signature": null}});
        let block: ContentBlock = serde_json::from_value(legacy).unwrap();
        match block {
            ContentBlock::Text { text, signature } => {
                assert_eq!(text, "hi");
                assert!(signature.is_none());
            }
            _ => panic!("expected Text"),
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
    fn user_content_legacy_format() {
        let json = json!({"Text": "old format"});
        let content: UserContent = serde_json::from_value(json).unwrap();
        match content {
            UserContent::Text(t) => assert_eq!(t, "old format"),
            _ => panic!("expected Text"),
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
    fn message_user_legacy_format() {
        let legacy = json!({
            "User": {
                "content": {"Text": "old"},
                "display_text": null,
                "timestamp": 999
            }
        });
        let msg: Message = serde_json::from_value(legacy).unwrap();
        match msg {
            Message::User(u) => {
                assert_eq!(u.timestamp, 999);
                match u.content {
                    UserContent::Text(t) => assert_eq!(t, "old"),
                    _ => panic!("expected Text"),
                }
            }
            _ => panic!("expected User"),
        }
    }

    #[test]
    fn message_assistant_roundtrip() {
        let msg = Message::Assistant(AssistantMessage {
            content: vec![ContentBlock::Text { text: "hi".to_string(), signature: None }],
            api: Api::AnthropicMessages,
            provider: Provider::Anthropic,
            model: "claude-3".to_string(),
            response_model: None,
            response_id: None,
            usage: Usage {
                input: 10, output: 20, cache_read: 0, cache_write: 0, total_tokens: 30,
                cost: UsageCost { input: 0.0, output: 0.0, cache_read: 0.0, cache_write: 0.0, total: 0.0 },
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
            is_error: false,
            timestamp: 111,
        });
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "toolResult");
        assert_eq!(json["toolCallId"], "tc_1");
        let back: Message = serde_json::from_value(json).unwrap();
        match back {
            Message::ToolResult(tr) => {
                assert_eq!(tr.tool_call_id, "tc_1");
            }
            _ => panic!("expected ToolResult"),
        }
    }
}
