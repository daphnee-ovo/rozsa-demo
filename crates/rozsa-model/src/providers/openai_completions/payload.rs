//! Payload construction for OpenAI-compatible Chat Completions providers.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::types::{
    ContentBlock, Context, InputModality, Message, Model, SimpleStreamOptions, ThinkingLevel,
    ToolCall, ToolSchema, UserContent,
};

/// Compatibility options that normalize known OpenAI-compatible providers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAICompletionsCompat {
    pub supports_store: bool,
    pub supports_developer_role: bool,
    pub supports_reasoning_effort: bool,
    pub supports_usage_in_streaming: bool,
    pub max_tokens_field: MaxTokensField,
    pub requires_tool_result_name: bool,
    pub requires_assistant_after_tool_result: bool,
    pub requires_thinking_as_text: bool,
    pub requires_reasoning_content_on_assistant_messages: bool,
    pub thinking_format: ThinkingFormat,
    pub cache_control_format: Option<CacheControlFormat>,
    pub open_router_routing: Option<Value>,
    pub vercel_gateway_routing: Option<Value>,
    pub zai_tool_stream: bool,
    pub supports_strict_mode: bool,
    pub send_session_affinity_headers: bool,
    pub supports_long_cache_retention: bool,
}

/// Provider-specific max token field spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MaxTokensField {
    MaxTokens,
    MaxCompletionTokens,
}

/// Provider-specific reasoning control format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThinkingFormat {
    OpenAI,
    DeepSeek,
    OpenRouter,
    Together,
    Zai,
    Qwen,
    QwenChatTemplate,
}

/// Provider-specific prompt cache marker format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheControlFormat {
    Anthropic,
}

impl Default for OpenAICompletionsCompat {
    /// Return standard OpenAI Chat Completions compatibility defaults.
    fn default() -> Self {
        Self {
            supports_store: true,
            supports_developer_role: true,
            supports_reasoning_effort: true,
            supports_usage_in_streaming: true,
            max_tokens_field: MaxTokensField::MaxCompletionTokens,
            requires_tool_result_name: false,
            requires_assistant_after_tool_result: false,
            requires_thinking_as_text: false,
            requires_reasoning_content_on_assistant_messages: false,
            thinking_format: ThinkingFormat::OpenAI,
            cache_control_format: None,
            open_router_routing: None,
            vercel_gateway_routing: None,
            zai_tool_stream: false,
            supports_strict_mode: true,
            send_session_affinity_headers: false,
            supports_long_cache_retention: true,
        }
    }
}

/// Build the request body for `/chat/completions`.
pub fn build_chat_completions_payload(
    model: &Model,
    context: &Context,
    options: &SimpleStreamOptions,
) -> Value {
    let compat = resolve_compat(model);
    let reasoning = resolve_reasoning(model, options.reasoning);
    let mut payload = json!({
        "model": model.id,
        "messages": convert_messages(model, context, &compat),
        "stream": true,
    });

    if compat.supports_usage_in_streaming {
        payload["stream_options"] = json!({ "include_usage": true });
    }
    if compat.supports_store {
        payload["store"] = json!(false);
    }
    if let Some(max_tokens) = options.base.max_tokens {
        match compat.max_tokens_field {
            MaxTokensField::MaxTokens => payload["max_tokens"] = json!(max_tokens),
            MaxTokensField::MaxCompletionTokens => {
                payload["max_completion_tokens"] = json!(max_tokens)
            }
        }
    }
    if let Some(temperature) = options.base.temperature {
        payload["temperature"] = json!(temperature);
    }
    if let Some(tool_choice) = options.tool_choice.as_ref() {
        payload["tool_choice"] = tool_choice.clone();
    }
    if !context.tools.is_empty() {
        payload["tools"] = convert_tools(&context.tools, &compat);
        if compat.zai_tool_stream {
            payload["tool_stream"] = json!(true);
        }
    } else if has_tool_history(&context.messages) {
        payload["tools"] = json!([]);
    }
    apply_reasoning_options(model, &compat, reasoning, &mut payload);
    apply_prompt_cache_options(model, options, &compat, &mut payload);
    apply_provider_routing(model, &compat, &mut payload);
    apply_anthropic_cache_control(&compat, options, &mut payload);
    payload
}

/// Resolve compatibility defaults from provider/base URL and model overrides.
pub fn resolve_compat(model: &Model) -> OpenAICompletionsCompat {
    let mut compat = detect_compat(model);
    if let Some(overrides) = model.compat.as_ref().and_then(Value::as_object) {
        if let Some(value) = overrides.get("supportsStore").and_then(Value::as_bool) {
            compat.supports_store = value;
        }
        if let Some(value) = overrides
            .get("supportsDeveloperRole")
            .and_then(Value::as_bool)
        {
            compat.supports_developer_role = value;
        }
        if let Some(value) = overrides
            .get("supportsReasoningEffort")
            .and_then(Value::as_bool)
        {
            compat.supports_reasoning_effort = value;
        }
        if let Some(value) = overrides
            .get("supportsUsageInStreaming")
            .and_then(Value::as_bool)
        {
            compat.supports_usage_in_streaming = value;
        }
        if let Some(value) = overrides
            .get("requiresToolResultName")
            .and_then(Value::as_bool)
        {
            compat.requires_tool_result_name = value;
        }
        if let Some(value) = overrides
            .get("requiresAssistantAfterToolResult")
            .and_then(Value::as_bool)
        {
            compat.requires_assistant_after_tool_result = value;
        }
        if let Some(value) = overrides
            .get("requiresThinkingAsText")
            .and_then(Value::as_bool)
        {
            compat.requires_thinking_as_text = value;
        }
        if let Some(value) = overrides
            .get("requiresReasoningContentOnAssistantMessages")
            .and_then(Value::as_bool)
        {
            compat.requires_reasoning_content_on_assistant_messages = value;
        }
        if let Some(value) = overrides.get("supportsStrictMode").and_then(Value::as_bool) {
            compat.supports_strict_mode = value;
        }
        if let Some(value) = overrides
            .get("sendSessionAffinityHeaders")
            .and_then(Value::as_bool)
        {
            compat.send_session_affinity_headers = value;
        }
        if let Some(value) = overrides
            .get("supportsLongCacheRetention")
            .and_then(Value::as_bool)
        {
            compat.supports_long_cache_retention = value;
        }
        if let Some(value) = overrides.get("maxTokensField").and_then(Value::as_str) {
            compat.max_tokens_field = if value == "max_tokens" {
                MaxTokensField::MaxTokens
            } else {
                MaxTokensField::MaxCompletionTokens
            };
        }
        if let Some(value) = overrides.get("thinkingFormat").and_then(Value::as_str) {
            compat.thinking_format = match value {
                "deepseek" => ThinkingFormat::DeepSeek,
                "openrouter" => ThinkingFormat::OpenRouter,
                "together" => ThinkingFormat::Together,
                "zai" => ThinkingFormat::Zai,
                "qwen" => ThinkingFormat::Qwen,
                "qwen-chat-template" => ThinkingFormat::QwenChatTemplate,
                _ => ThinkingFormat::OpenAI,
            };
        }
        if let Some(value) = overrides.get("cacheControlFormat").and_then(Value::as_str) {
            compat.cache_control_format = if value == "anthropic" {
                Some(CacheControlFormat::Anthropic)
            } else {
                None
            };
        }
        if let Some(value) = overrides.get("openRouterRouting") {
            compat.open_router_routing = Some(value.clone());
        }
        if let Some(value) = overrides.get("vercelGatewayRouting") {
            compat.vercel_gateway_routing = Some(value.clone());
        }
        if let Some(value) = overrides.get("zaiToolStream").and_then(Value::as_bool) {
            compat.zai_tool_stream = value;
        }
    }
    compat
}

/// Convert the normalized context into OpenAI-compatible chat messages.
pub fn convert_messages(
    model: &Model,
    context: &Context,
    compat: &OpenAICompletionsCompat,
) -> Vec<Value> {
    let mut messages = Vec::new();
    if let Some(system_prompt) = context
        .system_prompt
        .as_ref()
        .filter(|value| !value.is_empty())
    {
        let role = if model.reasoning && compat.supports_developer_role {
            "developer"
        } else {
            "system"
        };
        messages.push(json!({ "role": role, "content": system_prompt }));
    }

    let mut last_role: Option<&str> = None;
    for message in &context.messages {
        if compat.requires_assistant_after_tool_result && last_role == Some("toolResult") {
            if matches!(message, Message::User(_)) {
                messages.push(
                    json!({ "role": "assistant", "content": "I have processed the tool results." }),
                );
            }
        }
        match message {
            Message::User(user) => {
                if let Some(message) = convert_user_message(user, model) {
                    messages.push(message);
                }
                last_role = Some("user");
            }
            Message::Assistant(assistant) => {
                if let Some(message) = convert_assistant_message(assistant, model, compat) {
                    messages.push(message);
                }
                last_role = Some("assistant");
            }
            Message::ToolResult(tool_result) => {
                let text = tool_result
                    .content
                    .iter()
                    .filter_map(text_from_block)
                    .collect::<Vec<_>>()
                    .join("\n");
                let mut tool_message = json!({
                    "role": "tool",
                    "content": if text.is_empty() { "(see attached image)" } else { &text },
                    "tool_call_id": tool_result.tool_call_id,
                });
                if compat.requires_tool_result_name {
                    tool_message["name"] = json!(tool_result.tool_name);
                }
                messages.push(tool_message);
                last_role = Some("toolResult");
            }
        }
    }
    messages
}

/// Convert tools into OpenAI-compatible function tools.
pub fn convert_tools(tools: &[ToolSchema], compat: &OpenAICompletionsCompat) -> Value {
    Value::Array(
        tools
            .iter()
            .map(|tool| {
                let mut function = json!({
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.parameters,
                });
                if compat.supports_strict_mode {
                    function["strict"] = json!(false);
                }
                json!({ "type": "function", "function": function })
            })
            .collect(),
    )
}

/// Determine whether history contains tool calls or tool results.
pub fn has_tool_history(messages: &[Message]) -> bool {
    messages.iter().any(|message| match message {
        Message::ToolResult(_) => true,
        Message::Assistant(assistant) => assistant
            .content
            .iter()
            .any(|block| matches!(block, ContentBlock::ToolCall(_))),
        Message::User(_) => false,
    })
}

/// Detect compatibility behavior from model provider and base URL.
fn detect_compat(model: &Model) -> OpenAICompletionsCompat {
    let provider = format!("{:?}", model.provider).to_ascii_lowercase();
    let base_url = model.base_url.to_ascii_lowercase();
    let is_zai = provider.contains("zai") || base_url.contains("api.z.ai");
    let is_together = provider.contains("together")
        || base_url.contains("api.together.ai")
        || base_url.contains("api.together.xyz");
    let is_moonshot = provider.contains("moonshot") || base_url.contains("api.moonshot.");
    let is_cloudflare_workers =
        provider.contains("cloudflareworkers") || base_url.contains("api.cloudflare.com");
    let is_cloudflare_gateway =
        provider.contains("cloudflareaigateway") || base_url.contains("gateway.ai.cloudflare.com");
    let is_xai = provider.contains("xai") || base_url.contains("api.x.ai");
    let is_deepseek = provider.contains("deepseek") || base_url.contains("deepseek.com");
    let cache_control_format =
        if provider.contains("openrouter") && model.id.starts_with("anthropic/") {
            Some(CacheControlFormat::Anthropic)
        } else {
            None
        };
    let is_non_standard = provider.contains("cerebras")
        || base_url.contains("cerebras.ai")
        || is_xai
        || is_together
        || base_url.contains("chutes.ai")
        || is_deepseek
        || is_zai
        || is_moonshot
        || provider.contains("opencode")
        || base_url.contains("opencode.ai")
        || is_cloudflare_workers
        || is_cloudflare_gateway;

    OpenAICompletionsCompat {
        supports_store: !is_non_standard,
        supports_developer_role: !is_non_standard,
        supports_reasoning_effort: !(is_xai
            || is_zai
            || is_moonshot
            || is_together
            || is_cloudflare_gateway),
        max_tokens_field: if base_url.contains("chutes.ai")
            || is_moonshot
            || is_cloudflare_gateway
            || is_together
        {
            MaxTokensField::MaxTokens
        } else {
            MaxTokensField::MaxCompletionTokens
        },
        requires_reasoning_content_on_assistant_messages: is_deepseek,
        thinking_format: if is_deepseek {
            ThinkingFormat::DeepSeek
        } else if is_zai {
            ThinkingFormat::Zai
        } else if is_together {
            ThinkingFormat::Together
        } else if provider.contains("openrouter") || base_url.contains("openrouter.ai") {
            ThinkingFormat::OpenRouter
        } else {
            ThinkingFormat::OpenAI
        },
        cache_control_format,
        supports_strict_mode: !(is_moonshot || is_together || is_cloudflare_gateway),
        supports_long_cache_retention: !(is_together
            || is_cloudflare_workers
            || is_cloudflare_gateway),
        ..OpenAICompletionsCompat::default()
    }
}

/// Convert one user message into an OpenAI-compatible message value.
fn convert_user_message(user: &crate::types::UserMessage, model: &Model) -> Option<Value> {
    match &user.content {
        UserContent::Text(text) => Some(json!({ "role": "user", "content": text })),
        UserContent::Blocks(blocks) => {
            let content = blocks
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::Text { text, .. } => Some(json!({ "type": "text", "text": text })),
                    ContentBlock::Image { data, mime_type } if model.input_modalities.contains(&InputModality::Image) => {
                        Some(json!({ "type": "image_url", "image_url": { "url": format!("data:{mime_type};base64,{data}") } }))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            if content.is_empty() {
                None
            } else {
                Some(json!({ "role": "user", "content": content }))
            }
        }
    }
}

/// Convert one assistant message into text, reasoning, and tool-call fields.
fn convert_assistant_message(
    assistant: &crate::types::AssistantMessage,
    model: &Model,
    compat: &OpenAICompletionsCompat,
) -> Option<Value> {
    let text = assistant
        .content
        .iter()
        .filter_map(text_from_block)
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>()
        .join("");
    let thinking = assistant
        .content
        .iter()
        .filter_map(thinking_from_block)
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    let tool_calls = assistant
        .content
        .iter()
        .filter_map(tool_call_from_block)
        .collect::<Vec<_>>();

    if text.is_empty() && thinking.is_empty() && tool_calls.is_empty() {
        return None;
    }

    let mut message = json!({
        "role": "assistant",
        "content": if text.is_empty() { Value::Null } else { json!(text) },
    });
    if compat.requires_assistant_after_tool_result && text.is_empty() && thinking.is_empty() {
        message["content"] = json!("");
    }
    if !thinking.is_empty() {
        if compat.requires_thinking_as_text {
            message["content"] = json!(if text.is_empty() {
                thinking
            } else {
                format!("{thinking}\n\n{text}")
            });
        } else {
            message["reasoning_content"] = json!(thinking);
        }
    }
    if !tool_calls.is_empty() {
        message["tool_calls"] = Value::Array(tool_calls);
    }
    if compat.requires_reasoning_content_on_assistant_messages
        && model.reasoning
        && message.get("reasoning_content").is_none()
    {
        message["reasoning_content"] = json!("");
    }
    Some(message)
}

/// Extract text from a content block when it carries text.
fn text_from_block(block: &ContentBlock) -> Option<String> {
    match block {
        ContentBlock::Text { text, .. } => Some(text.clone()),
        _ => None,
    }
}

/// Extract thinking content from a content block when it carries reasoning.
fn thinking_from_block(block: &ContentBlock) -> Option<String> {
    match block {
        ContentBlock::Thinking { thinking, .. } => Some(thinking.clone()),
        _ => None,
    }
}

/// Convert a normalized tool call into OpenAI-compatible JSON.
fn tool_call_from_block(block: &ContentBlock) -> Option<Value> {
    match block {
        ContentBlock::ToolCall(ToolCall {
            id,
            name,
            arguments,
        }) => Some(json!({
            "id": id,
            "type": "function",
            "function": {
                "name": name,
                "arguments": arguments.to_string(),
            }
        })),
        _ => None,
    }
}

/// Resolve requested reasoning to an enabled reasoning level for this model.
fn resolve_reasoning(model: &Model, requested: Option<ThinkingLevel>) -> Option<ThinkingLevel> {
    if !model.reasoning {
        return None;
    }
    requested.filter(|level| *level != ThinkingLevel::Off)
}

/// Add provider-specific reasoning fields to a chat completion payload.
fn apply_reasoning_options(
    model: &Model,
    compat: &OpenAICompletionsCompat,
    reasoning: Option<ThinkingLevel>,
    payload: &mut Value,
) {
    match compat.thinking_format {
        ThinkingFormat::Zai | ThinkingFormat::Qwen if model.reasoning => {
            payload["enable_thinking"] = json!(reasoning.is_some());
        }
        ThinkingFormat::QwenChatTemplate => {
            if model.reasoning {
                payload["chat_template_kwargs"] =
                    json!({ "enable_thinking": reasoning.is_some(), "preserve_thinking": true });
            }
        }
        ThinkingFormat::DeepSeek => {
            if model.reasoning {
                payload["thinking"] =
                    json!({ "type": if reasoning.is_some() { "enabled" } else { "disabled" } });
            }
            if let Some(reasoning) = reasoning.filter(|_| compat.supports_reasoning_effort) {
                payload["reasoning_effort"] = json!(thinking_level_value(model, reasoning));
            }
        }
        ThinkingFormat::OpenRouter => {
            if let Some(reasoning) = reasoning {
                payload["reasoning"] = json!({ "effort": thinking_level_value(model, reasoning) });
            } else if model.reasoning {
                if let Some(value) = thinking_level_value_optional(model, ThinkingLevel::Off) {
                    payload["reasoning"] = json!({ "effort": value });
                }
            }
        }
        ThinkingFormat::Together => {
            if model.reasoning {
                payload["reasoning"] = json!({ "enabled": reasoning.is_some() });
            }
            if let Some(reasoning) = reasoning.filter(|_| compat.supports_reasoning_effort) {
                payload["reasoning_effort"] = json!(thinking_level_value(model, reasoning));
            }
        }
        ThinkingFormat::OpenAI => {
            if let Some(reasoning) = reasoning.filter(|_| compat.supports_reasoning_effort) {
                payload["reasoning_effort"] = json!(thinking_level_value(model, reasoning));
            } else if model.reasoning && compat.supports_reasoning_effort {
                if let Some(value) = thinking_level_value_optional(model, ThinkingLevel::Off) {
                    payload["reasoning_effort"] = json!(value);
                }
            }
        }
        _ => {}
    }
}

/// Add prompt-cache fields supported by OpenAI-compatible providers.
fn apply_prompt_cache_options(
    model: &Model,
    options: &SimpleStreamOptions,
    compat: &OpenAICompletionsCompat,
    payload: &mut Value,
) {
    let cache_enabled = options.base.cache_retention != crate::types::CacheRetention::None;
    if model.base_url.contains("api.openai.com") && cache_enabled {
        if let Some(session_id) = options.base.session_id.as_ref() {
            payload["prompt_cache_key"] = json!(session_id.chars().take(64).collect::<String>());
        }
    }
    if options.base.cache_retention == crate::types::CacheRetention::Long
        && compat.supports_long_cache_retention
    {
        payload["prompt_cache_retention"] = json!("24h");
    }
}

/// Add provider routing options for OpenRouter and Vercel AI Gateway.
fn apply_provider_routing(model: &Model, compat: &OpenAICompletionsCompat, payload: &mut Value) {
    if model.base_url.contains("openrouter.ai") {
        if let Some(routing) = compat.open_router_routing.as_ref() {
            payload["provider"] = routing.clone();
        }
    }
    if model.base_url.contains("ai-gateway.vercel.sh") {
        if let Some(routing) = compat.vercel_gateway_routing.as_ref() {
            let mut gateway = serde_json::Map::new();
            if let Some(only) = routing.get("only") {
                gateway.insert("only".to_string(), only.clone());
            }
            if let Some(order) = routing.get("order") {
                gateway.insert("order".to_string(), order.clone());
            }
            if !gateway.is_empty() {
                payload["providerOptions"] = json!({ "gateway": Value::Object(gateway) });
            }
        }
    }
}

/// Add Anthropic-style cache-control markers for compatible proxies.
fn apply_anthropic_cache_control(
    compat: &OpenAICompletionsCompat,
    options: &SimpleStreamOptions,
    payload: &mut Value,
) {
    if compat.cache_control_format != Some(CacheControlFormat::Anthropic)
        || options.base.cache_retention == crate::types::CacheRetention::None
    {
        return;
    }
    let cache_control = if options.base.cache_retention == crate::types::CacheRetention::Long
        && compat.supports_long_cache_retention
    {
        json!({ "type": "ephemeral", "ttl": "1h" })
    } else {
        json!({ "type": "ephemeral" })
    };
    add_cache_control_to_system_prompt(payload, &cache_control);
    add_cache_control_to_last_tool(payload, &cache_control);
    add_cache_control_to_last_conversation_message(payload, &cache_control);
}

/// Mark the first instruction message as cacheable when possible.
fn add_cache_control_to_system_prompt(payload: &mut Value, cache_control: &Value) {
    let Some(messages) = payload.get_mut("messages").and_then(Value::as_array_mut) else {
        return;
    };
    for message in messages {
        let role = message.get("role").and_then(Value::as_str);
        if matches!(role, Some("system" | "developer")) {
            let _ = add_cache_control_to_text_content(message, cache_control);
            return;
        }
    }
}

/// Mark the last tool definition as cacheable when present.
fn add_cache_control_to_last_tool(payload: &mut Value, cache_control: &Value) {
    let Some(tools) = payload.get_mut("tools").and_then(Value::as_array_mut) else {
        return;
    };
    if let Some(tool) = tools.last_mut() {
        tool["cache_control"] = cache_control.clone();
    }
}

/// Mark the latest user or assistant text content as cacheable.
fn add_cache_control_to_last_conversation_message(payload: &mut Value, cache_control: &Value) {
    let Some(messages) = payload.get_mut("messages").and_then(Value::as_array_mut) else {
        return;
    };
    for message in messages.iter_mut().rev() {
        let role = message.get("role").and_then(Value::as_str);
        if matches!(role, Some("user" | "assistant"))
            && add_cache_control_to_text_content(message, cache_control)
        {
            return;
        }
    }
}

/// Add cache-control metadata to string or content-part text.
fn add_cache_control_to_text_content(message: &mut Value, cache_control: &Value) -> bool {
    let Some(content) = message.get_mut("content") else {
        return false;
    };
    if let Some(text) = content.as_str().filter(|value| !value.is_empty()) {
        *content = json!([{ "type": "text", "text": text, "cache_control": cache_control }]);
        return true;
    }
    let Some(parts) = content.as_array_mut() else {
        return false;
    };
    for part in parts.iter_mut().rev() {
        if part.get("type").and_then(Value::as_str) == Some("text") {
            part["cache_control"] = cache_control.clone();
            return true;
        }
    }
    false
}

/// Convert a unified thinking level into the provider-facing value.
fn thinking_level_value(model: &Model, level: ThinkingLevel) -> String {
    thinking_level_value_optional(model, level).unwrap_or_else(|| {
        match level {
            ThinkingLevel::Off => "off",
            ThinkingLevel::Minimal => "minimal",
            ThinkingLevel::Low => "low",
            ThinkingLevel::Medium => "medium",
            ThinkingLevel::High => "high",
            ThinkingLevel::XHigh => "xhigh",
        }
        .to_string()
    })
}

/// Return a provider-facing thinking value when this level is supported.
fn thinking_level_value_optional(model: &Model, level: ThinkingLevel) -> Option<String> {
    model
        .thinking_level_map
        .as_ref()
        .and_then(|map| map.get(&level))
        .and_then(|value| value.clone())
        .or_else(|| {
            if model
                .thinking_level_map
                .as_ref()
                .and_then(|map| map.get(&level))
                .is_some()
            {
                None
            } else {
                Some(
                    match level {
                        ThinkingLevel::Off => "off",
                        ThinkingLevel::Minimal => "minimal",
                        ThinkingLevel::Low => "low",
                        ThinkingLevel::Medium => "medium",
                        ThinkingLevel::High => "high",
                        ThinkingLevel::XHigh => "xhigh",
                    }
                    .to_string(),
                )
            }
        })
}
