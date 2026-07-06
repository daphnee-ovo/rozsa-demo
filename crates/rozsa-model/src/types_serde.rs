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
    let typ = value
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("ContentBlock missing 'type': {value}"))?;
    match typ {
        "text" => Ok(ContentBlock::Text {
            text: str_field(value, "text")?,
            signature: opt_str(value, "textSignature"),
        }),
        "thinking" => Ok(ContentBlock::Thinking {
            thinking: str_field(value, "thinking")?,
            signature: opt_str(value, "thinkingSignature"),
            redacted: value
                .get("redacted")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }),
        "image" => Ok(ContentBlock::Image {
            data: str_field(value, "data")?,
            mime_type: str_field(value, "mimeType")?,
        }),
        "toolCall" => Ok(ContentBlock::ToolCall(ToolCall {
            id: str_field(value, "id")?,
            name: str_field(value, "name")?,
            arguments: value
                .get("arguments")
                .cloned()
                .unwrap_or(Value::Object(Default::default())),
        })),
        _ => Err(format!("unknown content block type: {typ}")),
    }
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
        let blocks: Result<Vec<ContentBlock>, _> =
            arr.iter().map(deserialize_content_block).collect();
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
    let role = value
        .get("role")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Message missing 'role': {value}"))?;
    match role {
        "user" => {
            let content = value.get("content").ok_or("user message missing content")?;
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
            details: value.get("details").cloned().unwrap_or(Value::Null),
            is_error: value
                .get("isError")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            timestamp: value.get("timestamp").and_then(Value::as_i64).unwrap_or(0),
        })),
        _ => Err(format!("unknown message role: {role}")),
    }
}

fn deserialize_assistant(value: &Value) -> Result<AssistantMessage, String> {
    Ok(AssistantMessage {
        content: deserialize_content_blocks(value.get("content"))?,
        api: parse_api(
            value
                .get("api")
                .and_then(Value::as_str)
                .unwrap_or("anthropic-messages"),
        ),
        provider: parse_provider(
            value
                .get("provider")
                .and_then(Value::as_str)
                .unwrap_or("anthropic"),
        ),
        model: str_field(value, "model")?,
        response_model: opt_str(value, "responseModel"),
        response_id: opt_str(value, "responseId"),
        usage: deserialize_usage(value.get("usage")),
        stop_reason: value
            .get("stopReason")
            .and_then(Value::as_str)
            .map(parse_stop_reason)
            .unwrap_or(StopReason::Stop),
        error_message: opt_str(value, "errorMessage"),
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
    let input = value
        .and_then(|v| v.get("input"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let output = value
        .and_then(|v| v.get("output"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let cache_read = value
        .and_then(|v| v.get("cacheRead"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let cache_write = value
        .and_then(|v| v.get("cacheWrite"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let total_tokens = value
        .and_then(|v| v.get("totalTokens"))
        .and_then(Value::as_u64)
        .unwrap_or(input + output + cache_read + cache_write);
    let cost = value
        .and_then(|v| v.get("cost"))
        .map(|c| UsageCost {
            input: c.get("input").and_then(Value::as_f64).unwrap_or(0.0),
            output: c.get("output").and_then(Value::as_f64).unwrap_or(0.0),
            cache_read: c.get("cacheRead").and_then(Value::as_f64).unwrap_or(0.0),
            cache_write: c.get("cacheWrite").and_then(Value::as_f64).unwrap_or(0.0),
            total: c.get("total").and_then(Value::as_f64).unwrap_or(0.0),
        })
        .unwrap_or(UsageCost {
            input: 0.0,
            output: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
            total: 0.0,
        });
    Usage {
        input,
        output,
        cache_read,
        cache_write,
        total_tokens,
        cost,
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
    value
        .get(field)
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| format!("missing or invalid field: {field}"))
}

fn opt_str(value: &Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(ToString::to_string)
}
