//! JSONL bridge protocol between the TypeScript AI layer and `rozsa-model`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::providers::common::provider_id;
use crate::types::{
    Api, AssistantMessage, CacheRetention, ContentBlock, Context, InputModality, Message, Model,
    ModelCost, Provider, SimpleStreamOptions, StopReason, StreamEvent, StreamOptions,
    ThinkingBudgets, ThinkingLevel, ToolCall, ToolResultMessage, ToolSchema, Transport, Usage,
    UserContent, UserMessage,
};

/// Request sent by TypeScript to the Rust bridge.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum BridgeInput {
    #[serde(rename = "request")]
    Request {
        id: String,
        method: BridgeMethod,
        model: Value,
        context: Value,
        #[serde(default)]
        options: Value,
    },
    #[serde(rename = "cancel")]
    Cancel { id: String },
}

/// Streaming method requested by the TypeScript side.
#[derive(Debug, Clone, Copy, Deserialize)]
pub enum BridgeMethod {
    #[serde(rename = "stream")]
    Stream,
    #[serde(rename = "streamSimple")]
    StreamSimple,
}

/// Response line sent by Rust to TypeScript.
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum BridgeOutput {
    #[serde(rename = "event")]
    Event { id: String, event: Value },
    #[serde(rename = "error")]
    Error {
        id: String,
        message: String,
        code: String,
    },
}

/// Parsed request ready for the Rust provider registry.
pub struct ProviderRequest {
    pub id: String,
    pub method: BridgeMethod,
    pub model: Model,
    pub context: Context,
    pub options: SimpleStreamOptions,
    pub models_json_path: Option<String>,
    pub auth_json_path: Option<String>,
}

/// Parse a JSONL input line into a bridge request.
pub fn parse_input_line(line: &str) -> Result<BridgeInput, String> {
    serde_json::from_str(line).map_err(|error| format!("invalid bridge input: {error}"))
}

/// Convert a bridge input into provider-ready Rust types.
pub fn provider_request(input: BridgeInput) -> Result<Option<ProviderRequest>, String> {
    match input {
        BridgeInput::Request {
            id,
            method,
            model,
            context,
            options,
        } => Ok(Some(ProviderRequest {
            id,
            method,
            model: parse_model(&model)?,
            context: parse_context(&context)?,
            models_json_path: optional_string_field(&options, "modelsJsonPath"),
            auth_json_path: optional_string_field(&options, "authJsonPath"),
            options: parse_simple_options(&options)?,
        })),
        BridgeInput::Cancel { .. } => Ok(None),
    }
}

/// Convert one normalized stream event into a TypeScript event JSON value.
pub fn event_to_bridge_output(id: &str, event: StreamEvent) -> BridgeOutput {
    BridgeOutput::Event {
        id: id.to_string(),
        event: stream_event_to_value(event),
    }
}

/// Convert an error into a bridge error line.
pub fn bridge_error(id: &str, message: impl ToString, code: &str) -> BridgeOutput {
    BridgeOutput::Error {
        id: id.to_string(),
        message: message.to_string(),
        code: code.to_string(),
    }
}

/// Parse TypeScript model metadata into Rust model metadata.
fn parse_model(value: &Value) -> Result<Model, String> {
    let id = string_field(value, "id")?;
    Ok(Model {
        id: id.clone(),
        name: optional_string_field(value, "name").unwrap_or_else(|| id.clone()),
        api: parse_api(&string_field(value, "api")?),
        provider: parse_provider(&string_field(value, "provider")?),
        base_url: string_field(value, "baseUrl")?,
        reasoning: bool_field(value, "reasoning").unwrap_or(false),
        input_modalities: parse_input_modalities(value.get("input")),
        cost: parse_cost(value.get("cost")),
        context_window: usize_field(value, "contextWindow").unwrap_or(128_000),
        max_tokens: usize_field(value, "maxTokens").unwrap_or(16_384),
        thinking_level_map: parse_thinking_level_map(value.get("thinkingLevelMap"))?,
        headers: parse_string_map(value.get("headers"))?,
        compat: value.get("compat").cloned(),
    })
}

/// Parse TypeScript conversation context into Rust context.
fn parse_context(value: &Value) -> Result<Context, String> {
    let messages = value
        .get("messages")
        .and_then(Value::as_array)
        .ok_or_else(|| "context.messages must be an array".to_string())?
        .iter()
        .map(parse_message)
        .collect::<Result<Vec<_>, _>>()?;
    let tools = value
        .get("tools")
        .and_then(Value::as_array)
        .map(|items| items.iter().map(parse_tool).collect::<Result<Vec<_>, _>>())
        .transpose()?
        .unwrap_or_default();
    Ok(Context {
        system_prompt: optional_string_field(value, "systemPrompt"),
        messages,
        tools,
    })
}

/// Parse TypeScript stream options into Rust simple stream options.
fn parse_simple_options(value: &Value) -> Result<SimpleStreamOptions, String> {
    Ok(SimpleStreamOptions {
        base: StreamOptions {
            temperature: value.get("temperature").and_then(Value::as_f64),
            max_tokens: usize_field(value, "maxTokens"),
            api_key: optional_string_field(value, "apiKey"),
            transport: parse_transport(value.get("transport")),
            cache_retention: parse_cache_retention(value.get("cacheRetention")),
            session_id: optional_string_field(value, "sessionId"),
            headers: parse_string_map(value.get("headers"))?,
            timeout_ms: u64_field(value, "timeoutMs"),
            max_retries: u64_field(value, "maxRetries").map(|value| value as u32),
            max_retry_delay_ms: u64_field(value, "maxRetryDelayMs"),
            metadata: value.get("metadata").cloned(),
        },
        reasoning: value
            .get("reasoning")
            .or_else(|| value.get("reasoningEffort"))
            .and_then(Value::as_str)
            .map(parse_thinking_level),
        thinking_budgets: parse_thinking_budgets(value.get("thinkingBudgets")),
        tool_choice: value.get("toolChoice").cloned(),
    })
}

/// Parse one TypeScript message object into a Rust message.
fn parse_message(value: &Value) -> Result<Message, String> {
    match string_field(value, "role")?.as_str() {
        "user" => Ok(Message::User(UserMessage {
            content: parse_user_content(value.get("content"))?,
            display_text: optional_string_field(value, "displayText"),
            timestamp: i64_field(value, "timestamp").unwrap_or(0),
        })),
        "assistant" => Ok(Message::Assistant(parse_assistant_message(value)?)),
        "toolResult" => Ok(Message::ToolResult(ToolResultMessage {
            tool_call_id: string_field(value, "toolCallId")?,
            tool_name: string_field(value, "toolName")?,
            content: value
                .get("content")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .map(parse_content_block)
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?
                .unwrap_or_default(),
            is_error: bool_field(value, "isError").unwrap_or(false),
            timestamp: i64_field(value, "timestamp").unwrap_or(0),
        })),
        role => Err(format!("unsupported message role: {role}")),
    }
}

/// Parse one TypeScript assistant message into a Rust assistant message.
fn parse_assistant_message(value: &Value) -> Result<AssistantMessage, String> {
    Ok(AssistantMessage {
        content: value
            .get("content")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .map(parse_content_block)
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_default(),
        api: parse_api(&string_field(value, "api")?),
        provider: parse_provider(&string_field(value, "provider")?),
        model: string_field(value, "model")?,
        response_model: optional_string_field(value, "responseModel"),
        response_id: optional_string_field(value, "responseId"),
        usage: parse_usage(value.get("usage")),
        stop_reason: value
            .get("stopReason")
            .and_then(Value::as_str)
            .map(parse_stop_reason)
            .unwrap_or(StopReason::Stop),
        error_message: optional_string_field(value, "errorMessage"),
        timestamp: i64_field(value, "timestamp").unwrap_or(0),
    })
}

/// Parse user content from a string or content-block array.
fn parse_user_content(value: Option<&Value>) -> Result<UserContent, String> {
    match value {
        Some(Value::String(text)) => Ok(UserContent::Text(text.clone())),
        Some(Value::Array(items)) => Ok(UserContent::Blocks(
            items
                .iter()
                .map(parse_content_block)
                .collect::<Result<Vec<_>, _>>()?,
        )),
        _ => Err("user.content must be a string or content block array".to_string()),
    }
}

/// Parse one TypeScript content block into a Rust content block.
fn parse_content_block(value: &Value) -> Result<ContentBlock, String> {
    match string_field(value, "type")?.as_str() {
        "text" => Ok(ContentBlock::Text {
            text: string_field(value, "text")?,
            signature: optional_string_field(value, "textSignature"),
        }),
        "thinking" => Ok(ContentBlock::Thinking {
            thinking: string_field(value, "thinking")?,
            signature: optional_string_field(value, "thinkingSignature"),
            redacted: bool_field(value, "redacted").unwrap_or(false),
        }),
        "image" => Ok(ContentBlock::Image {
            data: string_field(value, "data")?,
            mime_type: string_field(value, "mimeType")?,
        }),
        "toolCall" => Ok(ContentBlock::ToolCall(ToolCall {
            id: string_field(value, "id")?,
            name: string_field(value, "name")?,
            arguments: value.get("arguments").cloned().unwrap_or_else(|| json!({})),
        })),
        block_type => Err(format!("unsupported content block type: {block_type}")),
    }
}

/// Parse one TypeScript tool schema into a Rust tool schema.
fn parse_tool(value: &Value) -> Result<ToolSchema, String> {
    Ok(ToolSchema {
        name: string_field(value, "name")?,
        description: string_field(value, "description")?,
        parameters: value
            .get("parameters")
            .cloned()
            .ok_or_else(|| "tool.parameters is required".to_string())?,
    })
}

/// Convert one Rust stream event into the TypeScript event JSON shape.
fn stream_event_to_value(event: StreamEvent) -> Value {
    match event {
        StreamEvent::Start { partial } => {
            json!({ "type": "start", "partial": assistant_message_to_value(partial) })
        }
        StreamEvent::TextStart {
            content_index,
            partial,
        } => {
            json!({ "type": "text_start", "contentIndex": content_index, "partial": assistant_message_to_value(partial) })
        }
        StreamEvent::TextDelta {
            content_index,
            delta,
            partial,
        } => {
            json!({ "type": "text_delta", "contentIndex": content_index, "delta": delta, "partial": assistant_message_to_value(partial) })
        }
        StreamEvent::TextEnd {
            content_index,
            content,
            partial,
        } => {
            json!({ "type": "text_end", "contentIndex": content_index, "content": content, "partial": assistant_message_to_value(partial) })
        }
        StreamEvent::ThinkingStart {
            content_index,
            partial,
        } => {
            json!({ "type": "thinking_start", "contentIndex": content_index, "partial": assistant_message_to_value(partial) })
        }
        StreamEvent::ThinkingDelta {
            content_index,
            delta,
            partial,
        } => {
            json!({ "type": "thinking_delta", "contentIndex": content_index, "delta": delta, "partial": assistant_message_to_value(partial) })
        }
        StreamEvent::ThinkingEnd {
            content_index,
            content,
            partial,
        } => {
            json!({ "type": "thinking_end", "contentIndex": content_index, "content": content, "partial": assistant_message_to_value(partial) })
        }
        StreamEvent::ToolCallStart {
            content_index,
            partial,
        } => {
            json!({ "type": "toolcall_start", "contentIndex": content_index, "partial": assistant_message_to_value(partial) })
        }
        StreamEvent::ToolCallDelta {
            content_index,
            delta,
            partial,
        } => {
            json!({ "type": "toolcall_delta", "contentIndex": content_index, "delta": delta, "partial": assistant_message_to_value(partial) })
        }
        StreamEvent::ToolCallEnd {
            content_index,
            tool_call,
            partial,
        } => {
            json!({ "type": "toolcall_end", "contentIndex": content_index, "toolCall": tool_call_to_value(tool_call), "partial": assistant_message_to_value(partial) })
        }
        StreamEvent::Done { reason, message } => {
            json!({ "type": "done", "reason": stop_reason_to_string(reason), "message": assistant_message_to_value(message) })
        }
        StreamEvent::Error { reason, error } => {
            json!({ "type": "error", "reason": stop_reason_to_string(reason), "error": assistant_message_to_value(error) })
        }
    }
}

/// Convert a Rust assistant message into the TypeScript assistant message shape.
fn assistant_message_to_value(message: AssistantMessage) -> Value {
    let mut output = serde_json::Map::new();
    output.insert("role".to_string(), json!("assistant"));
    output.insert(
        "content".to_string(),
        Value::Array(
            message
                .content
                .into_iter()
                .map(content_block_to_value)
                .collect(),
        ),
    );
    output.insert("api".to_string(), json!(api_to_string(&message.api)));
    output.insert(
        "provider".to_string(),
        json!(provider_to_string(&message.provider)),
    );
    output.insert("model".to_string(), json!(message.model));
    if let Some(response_model) = message.response_model {
        output.insert("responseModel".to_string(), json!(response_model));
    }
    if let Some(response_id) = message.response_id {
        output.insert("responseId".to_string(), json!(response_id));
    }
    output.insert("usage".to_string(), usage_to_value(message.usage));
    output.insert(
        "stopReason".to_string(),
        json!(stop_reason_to_string(message.stop_reason)),
    );
    if let Some(error_message) = message.error_message {
        output.insert("errorMessage".to_string(), json!(error_message));
    }
    output.insert("timestamp".to_string(), json!(message.timestamp));
    Value::Object(output)
}

/// Convert one Rust content block into the TypeScript content block shape.
fn content_block_to_value(block: ContentBlock) -> Value {
    match block {
        ContentBlock::Text { text, signature } => {
            let mut value = json!({ "type": "text", "text": text });
            if let Some(signature) = signature {
                value["textSignature"] = json!(signature);
            }
            value
        }
        ContentBlock::Thinking {
            thinking,
            signature,
            redacted,
        } => {
            let mut value = json!({ "type": "thinking", "thinking": thinking });
            if let Some(signature) = signature {
                value["thinkingSignature"] = json!(signature);
            }
            if redacted {
                value["redacted"] = json!(true);
            }
            value
        }
        ContentBlock::Image { data, mime_type } => {
            json!({ "type": "image", "data": data, "mimeType": mime_type })
        }
        ContentBlock::ToolCall(tool_call) => {
            let mut value = tool_call_to_value(tool_call);
            value["type"] = json!("toolCall");
            value
        }
    }
}

/// Convert a Rust tool call into the TypeScript tool call shape.
fn tool_call_to_value(tool_call: ToolCall) -> Value {
    json!({
        "id": tool_call.id,
        "name": tool_call.name,
        "arguments": tool_call.arguments,
    })
}

/// Convert Rust token usage into the TypeScript usage shape.
fn usage_to_value(usage: Usage) -> Value {
    json!({
        "input": usage.input,
        "output": usage.output,
        "cacheRead": usage.cache_read,
        "cacheWrite": usage.cache_write,
        "totalTokens": usage.total_tokens,
        "cost": {
            "input": usage.cost.input,
            "output": usage.cost.output,
            "cacheRead": usage.cost.cache_read,
            "cacheWrite": usage.cost.cache_write,
            "total": usage.cost.total,
        }
    })
}

/// Parse the TypeScript API identifier into the Rust API enum.
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

/// Parse the TypeScript provider identifier into the Rust provider enum.
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

/// Convert a Rust API enum back into the TypeScript API identifier.
fn api_to_string(api: &Api) -> String {
    match api {
        Api::AnthropicMessages => "anthropic-messages",
        Api::OpenAICompletions => "openai-completions",
        Api::OpenAIResponses => "openai-responses",
        Api::BedrockConverseStream => "bedrock-converse-stream",
        Api::GoogleGenerativeAI => "google-generative-ai",
        Api::GoogleVertex => "google-vertex",
        Api::MistralConversations => "mistral-conversations",
        Api::Custom(value) => value,
    }
    .to_string()
}

/// Convert a Rust provider enum back into the TypeScript provider identifier.
fn provider_to_string(provider: &Provider) -> String {
    match provider {
        Provider::Custom(value) => value.clone(),
        _ => provider_id(provider),
    }
}

/// Parse TypeScript model input modalities into Rust input modalities.
fn parse_input_modalities(value: Option<&Value>) -> Vec<InputModality> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(|item| {
                    if item == "image" {
                        InputModality::Image
                    } else {
                        InputModality::Text
                    }
                })
                .collect()
        })
        .unwrap_or_else(|| vec![InputModality::Text])
}

/// Parse TypeScript per-token model cost metadata.
fn parse_cost(value: Option<&Value>) -> ModelCost {
    ModelCost {
        input: value
            .and_then(|v| v.get("input"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
        output: value
            .and_then(|v| v.get("output"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
        cache_read: value
            .and_then(|v| v.get("cacheRead"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
        cache_write: value
            .and_then(|v| v.get("cacheWrite"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
    }
}

/// Parse optional TypeScript usage data into Rust usage data.
fn parse_usage(value: Option<&Value>) -> Usage {
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
    Usage {
        input,
        output,
        cache_read,
        cache_write,
        total_tokens: value
            .and_then(|v| v.get("totalTokens"))
            .and_then(Value::as_u64)
            .unwrap_or(input + output + cache_read + cache_write),
        cost: crate::providers::common::empty_usage().cost,
    }
}

/// Parse TypeScript thinking-level mappings into Rust thinking-level mappings.
fn parse_thinking_level_map(
    value: Option<&Value>,
) -> Result<Option<HashMap<ThinkingLevel, Option<String>>>, String> {
    let Some(object) = value.and_then(Value::as_object) else {
        return Ok(None);
    };
    let mut map = HashMap::new();
    for (key, value) in object {
        map.insert(
            parse_thinking_level(key),
            if value.is_null() {
                None
            } else {
                Some(value.as_str().unwrap_or_default().to_string())
            },
        );
    }
    Ok(Some(map))
}

/// Parse optional TypeScript reasoning token budgets.
fn parse_thinking_budgets(value: Option<&Value>) -> Option<ThinkingBudgets> {
    value.map(|value| ThinkingBudgets {
        minimal: value.get("minimal").and_then(Value::as_u64),
        low: value.get("low").and_then(Value::as_u64),
        medium: value.get("medium").and_then(Value::as_u64),
        high: value.get("high").and_then(Value::as_u64),
    })
}

/// Parse the TypeScript transport option into the Rust transport enum.
fn parse_transport(value: Option<&Value>) -> Transport {
    match value.and_then(Value::as_str) {
        Some("websocket") => Transport::WebSocket,
        Some("websocket-cached") => Transport::WebSocketCached,
        Some("auto") => Transport::Auto,
        _ => Transport::Sse,
    }
}

/// Parse the TypeScript cache-retention option into the Rust enum.
fn parse_cache_retention(value: Option<&Value>) -> CacheRetention {
    match value.and_then(Value::as_str) {
        Some("none") => CacheRetention::None,
        Some("long") => CacheRetention::Long,
        _ => CacheRetention::Short,
    }
}

/// Parse the TypeScript stop reason into the Rust stop reason enum.
fn parse_stop_reason(value: &str) -> StopReason {
    match value {
        "length" => StopReason::Length,
        "toolUse" => StopReason::ToolUse,
        "error" => StopReason::Error,
        "aborted" => StopReason::Aborted,
        _ => StopReason::Stop,
    }
}

/// Parse the TypeScript thinking level into the Rust thinking level enum.
fn parse_thinking_level(value: &str) -> ThinkingLevel {
    match value {
        "off" => ThinkingLevel::Off,
        "minimal" => ThinkingLevel::Minimal,
        "low" => ThinkingLevel::Low,
        "medium" => ThinkingLevel::Medium,
        "high" => ThinkingLevel::High,
        "xhigh" => ThinkingLevel::XHigh,
        _ => ThinkingLevel::Off,
    }
}

/// Convert a Rust stop reason into the TypeScript stop reason identifier.
fn stop_reason_to_string(reason: StopReason) -> &'static str {
    match reason {
        StopReason::Stop => "stop",
        StopReason::Length => "length",
        StopReason::ToolUse => "toolUse",
        StopReason::Error => "error",
        StopReason::Aborted => "aborted",
    }
}

/// Parse optional string-only object values into a Rust string map.
fn parse_string_map(value: Option<&Value>) -> Result<Option<HashMap<String, String>>, String> {
    let Some(object) = value.and_then(Value::as_object) else {
        return Ok(None);
    };
    Ok(Some(
        object
            .iter()
            .filter_map(|(key, value)| value.as_str().map(|value| (key.clone(), value.to_string())))
            .collect(),
    ))
}

/// Read a required string field from a JSON object.
fn string_field(value: &Value, field: &str) -> Result<String, String> {
    optional_string_field(value, field).ok_or_else(|| format!("{field} must be a string"))
}

/// Read an optional string field from a JSON object.
fn optional_string_field(value: &Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

/// Read an optional boolean field from a JSON object.
fn bool_field(value: &Value, field: &str) -> Option<bool> {
    value.get(field).and_then(Value::as_bool)
}

/// Read an optional unsigned size field from a JSON object.
fn usize_field(value: &Value, field: &str) -> Option<usize> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .map(|value| value as usize)
}

/// Read an optional unsigned 64-bit field from a JSON object.
fn u64_field(value: &Value, field: &str) -> Option<u64> {
    value.get(field).and_then(Value::as_u64)
}

/// Read an optional signed 64-bit field from a JSON object.
fn i64_field(value: &Value, field: &str) -> Option<i64> {
    value.get(field).and_then(Value::as_i64)
}
