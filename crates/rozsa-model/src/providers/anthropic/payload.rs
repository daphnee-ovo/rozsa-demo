// FrameworkTree
// payload.rs
// ├── struct AnthropicCompat
// ├── resolve_compat()
// ├── build_messages_payload()
// ├── build_anthropic_headers()
// ├── is_oauth_token()
// ├── to_claude_code_name()
// ├── from_claude_code_name()
// ├── normalize_tool_call_id()
// ├── convert_messages()
// ├── convert_user_blocks()
// ├── convert_assistant_blocks()
// ├── build_tool_result_block()
// ├── convert_tool_result_content()
// ├── convert_tools()
// ├── build_system_prompt()
// ├── build_thinking_config()
// ├── should_enable_thinking()
// ├── resolve_thinking_budget()
// ├── map_thinking_effort_to_effort()
// ├── should_use_fine_grained_tool_streaming()
// ├── build_cache_control()
// ├── resolve_cache_retention()
// ├── compat_bool()
// ├── provider_str()
// └── sanitize()

//! Anthropic Messages API payload construction.
//!
//! Internal Framework:
//! payload.rs
//! ├── build_messages_payload()    — top-level JSON payload builder
//! ├── convert_messages()          — Message[] → Anthropic message params
//! ├── convert_tools()             — ToolSchema[] → Anthropic tool params
//! ├── resolve_compat()            — Model compat → AnthropicCompat flags
//! ├── build_thinking_config()     — reasoning → thinking/output_config
//! └── normalize_tool_call_id()    — 64-char limit + char filter
//!
//! Related Docs:

use std::collections::HashMap;

use serde_json::{Value, json};

use crate::types::{
    CacheRetention, ContentBlock, Context, InputModality, Message, Model, Provider,
    SimpleStreamOptions, StreamOptions, ThinkingEffort, ToolCall, ToolSchema,
};

const NON_VISION_USER_IMAGE_PLACEHOLDER: &str = "(image omitted: model does not support images)";
const NON_VISION_TOOL_IMAGE_PLACEHOLDER: &str =
    "(tool image omitted: model does not support images)";

const FINE_GRAINED_TOOL_STREAMING_BETA: &str = "fine-grained-tool-streaming-2025-05-14";
const INTERLEAVED_THINKING_BETA: &str = "interleaved-thinking-2025-05-14";

const CLAUDE_CODE_VERSION: &str = "2.1.75";

const CLAUDE_CODE_TOOLS: &[&str] = &[
    "Read",
    "Write",
    "Edit",
    "Bash",
    "Grep",
    "Glob",
    "AskUserQuestion",
    "EnterPlanMode",
    "ExitPlanMode",
    "KillShell",
    "NotebookEdit",
    "Skill",
    "Task",
    "TaskOutput",
    "TodoWrite",
    "WebFetch",
    "WebSearch",
];

/// Resolved compat flags for Anthropic-compatible providers.
#[derive(Debug, Clone)]
pub struct AnthropicCompat {
    pub supports_eager_tool_input_streaming: bool,
    pub supports_long_cache_retention: bool,
    pub send_session_affinity_headers: bool,
    pub supports_cache_control_on_tools: bool,
    pub force_adaptive_thinking: bool,
}

/// Resolve compat flags from model metadata.
pub fn resolve_compat(model: &Model) -> AnthropicCompat {
    let provider_id = provider_str(&model.provider);
    let is_fireworks = provider_id == "fireworks";
    let is_cloudflare_anthropic =
        provider_id == "cloudflare-ai-gateway" && model.base_url.contains("anthropic");
    let compat_val = model.compat.as_ref();

    AnthropicCompat {
        supports_eager_tool_input_streaming: compat_bool(
            compat_val,
            "supportsEagerToolInputStreaming",
        )
        .unwrap_or(!is_fireworks),
        supports_long_cache_retention: compat_bool(compat_val, "supportsLongCacheRetention")
            .unwrap_or(!is_fireworks),
        send_session_affinity_headers: compat_bool(compat_val, "sendSessionAffinityHeaders")
            .unwrap_or(is_fireworks || is_cloudflare_anthropic),
        supports_cache_control_on_tools: compat_bool(compat_val, "supportsCacheControlOnTools")
            .unwrap_or(!is_fireworks),
        force_adaptive_thinking: compat_bool(compat_val, "forceAdaptiveThinking").unwrap_or(false),
    }
}

/// Build the complete Anthropic Messages API JSON payload.
pub fn build_messages_payload(
    model: &Model,
    context: &Context,
    options: &SimpleStreamOptions,
    is_oauth: bool,
    compat: &AnthropicCompat,
) -> Value {
    let cache_retention = resolve_cache_retention(&options.base);
    let cache_control = build_cache_control(cache_retention, compat);

    let mut payload = json!({
        "model": model.id,
        "messages": convert_messages(&context.messages, model, is_oauth, &cache_control),
        "max_tokens": options.base.max_tokens.unwrap_or(model.max_tokens),
        "stream": true,
    });

    build_system_prompt(context, is_oauth, &cache_control, &mut payload);

    let thinking_enabled = should_enable_thinking(model, options);
    if let Some(temp) = options.base.temperature {
        if !thinking_enabled {
            payload["temperature"] = json!(temp);
        }
    }

    if !context.tools.is_empty() {
        payload["tools"] = convert_tools(
            &context.tools,
            is_oauth,
            compat.supports_eager_tool_input_streaming,
            if compat.supports_cache_control_on_tools {
                cache_control.as_ref()
            } else {
                None
            },
        );
    }

    if model.reasoning {
        build_thinking_config(model, options, compat, thinking_enabled, &mut payload);
    }

    if let Some(metadata) = &options.base.metadata {
        if let Some(user_id) = metadata.get("user_id").and_then(|v| v.as_str()) {
            payload["metadata"] = json!({ "user_id": user_id });
        }
    }

    if let Some(tool_choice) = &options.tool_choice {
        payload["tool_choice"] = tool_choice.clone();
    }

    payload
}

/// Build headers required for the Anthropic Messages API request.
pub fn build_anthropic_headers(
    model: &Model,
    options: &SimpleStreamOptions,
    is_oauth: bool,
    compat: &AnthropicCompat,
    context: &Context,
) -> HashMap<String, String> {
    let mut headers = HashMap::new();
    headers.insert("content-type".to_string(), "application/json".to_string());
    headers.insert("accept".to_string(), "application/json".to_string());
    headers.insert("anthropic-version".to_string(), "2023-06-01".to_string());

    let mut betas = Vec::new();
    if should_use_fine_grained_tool_streaming(context, compat) {
        betas.push(FINE_GRAINED_TOOL_STREAMING_BETA);
    }
    if !compat.force_adaptive_thinking {
        betas.push(INTERLEAVED_THINKING_BETA);
    }
    if is_oauth {
        betas.push("claude-code-20250219");
        betas.push("oauth-2025-04-20");
    }
    if !betas.is_empty() {
        headers.insert("anthropic-beta".to_string(), betas.join(","));
    }

    if is_oauth {
        headers.insert(
            "user-agent".to_string(),
            format!("claude-cli/{CLAUDE_CODE_VERSION}"),
        );
        headers.insert("x-app".to_string(), "cli".to_string());
        headers.insert(
            "anthropic-dangerous-direct-browser-access".to_string(),
            "true".to_string(),
        );
    }

    if compat.send_session_affinity_headers {
        if let Some(session_id) = &options.base.session_id {
            headers.insert("x-session-affinity".to_string(), session_id.clone());
        }
    }

    if let Some(model_headers) = &model.headers {
        for (k, v) in model_headers {
            headers.insert(k.clone(), v.clone());
        }
    }
    if let Some(option_headers) = &options.base.headers {
        for (k, v) in option_headers {
            headers.insert(k.clone(), v.clone());
        }
    }

    headers
}

/// Detect whether API key is an OAuth token (sk-ant-oat prefix).
pub fn is_oauth_token(api_key: &str) -> bool {
    api_key.contains("sk-ant-oat")
}

/// Convert tool name for OAuth stealth mode.
pub fn to_claude_code_name(name: &str) -> String {
    let lower = name.to_lowercase();
    for &cc_name in CLAUDE_CODE_TOOLS {
        if cc_name.to_lowercase() == lower {
            return cc_name.to_string();
        }
    }
    name.to_string()
}

/// Reverse CC tool name lookup using the provided tools list.
pub fn from_claude_code_name(name: &str, tools: &[ToolSchema]) -> String {
    let lower = name.to_lowercase();
    for tool in tools {
        if tool.name.to_lowercase() == lower {
            return tool.name.clone();
        }
    }
    name.to_string()
}

/// Normalize tool call ID to match Anthropic's required pattern and length.
pub fn normalize_tool_call_id(id: &str) -> String {
    id.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .take(64)
        .collect()
}

// --- Internal helpers ---

fn convert_messages(
    messages: &[Message],
    model: &Model,
    is_oauth: bool,
    cache_control: &Option<Value>,
) -> Value {
    let supports_images = model.input_modalities.contains(&InputModality::Image);
    let mut params: Vec<Value> = Vec::new();
    let len = messages.len();
    let mut i = 0;

    while i < len {
        match &messages[i] {
            Message::User(user_msg) => {
                let blocks = match &user_msg.content {
                    crate::types::UserContent::Text(text) => {
                        let text = sanitize(text);
                        if text.trim().is_empty() {
                            i += 1;
                            continue;
                        }
                        json!([{ "type": "text", "text": text }])
                    }
                    crate::types::UserContent::Blocks(blocks) => convert_user_blocks(
                        blocks,
                        supports_images,
                        NON_VISION_USER_IMAGE_PLACEHOLDER,
                    ),
                };
                if blocks.as_array().is_some_and(|a| a.is_empty()) {
                    i += 1;
                    continue;
                }
                params.push(json!({ "role": "user", "content": blocks }));
            }
            Message::Assistant(msg) => {
                let blocks = convert_assistant_blocks(&msg.content, is_oauth);
                if blocks.as_array().is_some_and(|a| a.is_empty()) {
                    i += 1;
                    continue;
                }
                params.push(json!({ "role": "assistant", "content": blocks }));
            }
            Message::ToolResult(tr) => {
                let mut tool_results = vec![build_tool_result_block(tr, supports_images)];
                let mut j = i + 1;
                while j < len {
                    if let Message::ToolResult(next) = &messages[j] {
                        tool_results.push(build_tool_result_block(next, supports_images));
                        j += 1;
                    } else {
                        break;
                    }
                }
                i = j - 1;
                params.push(json!({ "role": "user", "content": tool_results }));
            }
        }
        i += 1;
    }

    if let Some(cc) = cache_control {
        if let Some(last) = params.last_mut() {
            if last.get("role").and_then(|v| v.as_str()) == Some("user") {
                if let Some(arr) = last.get_mut("content").and_then(|c| c.as_array_mut()) {
                    if let Some(last_block) = arr.last_mut() {
                        last_block["cache_control"] = cc.clone();
                    }
                }
            }
        }
    }

    Value::Array(params)
}

fn convert_user_blocks(blocks: &[ContentBlock], supports_images: bool, placeholder: &str) -> Value {
    let mut result: Vec<Value> = Vec::new();
    let mut prev_placeholder = false;

    for block in blocks {
        match block {
            ContentBlock::Text { text, .. } => {
                let text = sanitize(text);
                if !text.trim().is_empty() {
                    result.push(json!({ "type": "text", "text": text }));
                    prev_placeholder = false;
                }
            }
            ContentBlock::Image { data, mime_type } => {
                if supports_images {
                    result.push(json!({
                        "type": "image",
                        "source": { "type": "base64", "media_type": mime_type, "data": data }
                    }));
                    prev_placeholder = false;
                } else if !prev_placeholder {
                    result.push(json!({ "type": "text", "text": placeholder }));
                    prev_placeholder = true;
                }
            }
            _ => {}
        }
    }

    if !result.is_empty()
        && result
            .iter()
            .all(|b| b.get("type").and_then(|v| v.as_str()) == Some("image"))
    {
        result.insert(0, json!({ "type": "text", "text": "(see attached image)" }));
    }

    Value::Array(result)
}

fn convert_assistant_blocks(blocks: &[ContentBlock], is_oauth: bool) -> Value {
    let mut result: Vec<Value> = Vec::new();

    for block in blocks {
        match block {
            ContentBlock::Text { text, .. } => {
                let text = sanitize(text);
                if !text.trim().is_empty() {
                    result.push(json!({ "type": "text", "text": text }));
                }
            }
            ContentBlock::Thinking {
                thinking,
                signature,
                redacted,
            } => {
                if *redacted {
                    if let Some(sig) = signature {
                        result.push(json!({ "type": "redacted_thinking", "data": sig }));
                    }
                    continue;
                }
                let thinking = sanitize(thinking);
                if thinking.trim().is_empty() {
                    continue;
                }
                match signature {
                    Some(sig) if !sig.trim().is_empty() => {
                        result.push(json!({
                            "type": "thinking", "thinking": thinking, "signature": sig,
                        }));
                    }
                    _ => {
                        result.push(json!({ "type": "text", "text": thinking }));
                    }
                }
            }
            ContentBlock::ToolCall(ToolCall {
                id,
                name,
                arguments,
            }) => {
                let tool_name = if is_oauth {
                    to_claude_code_name(name)
                } else {
                    name.clone()
                };
                result.push(json!({
                    "type": "tool_use", "id": id, "name": tool_name, "input": arguments,
                }));
            }
            ContentBlock::Image { .. } => {}
        }
    }

    Value::Array(result)
}

fn build_tool_result_block(tr: &crate::types::ToolResultMessage, supports_images: bool) -> Value {
    let content = convert_tool_result_content(&tr.content, supports_images);
    json!({
        "type": "tool_result",
        "tool_use_id": tr.tool_call_id,
        "content": content,
        "is_error": tr.is_error,
    })
}

fn convert_tool_result_content(blocks: &[ContentBlock], supports_images: bool) -> Value {
    let has_images = blocks
        .iter()
        .any(|b| matches!(b, ContentBlock::Image { .. }));
    if !has_images {
        let text: String = blocks
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text, .. } => Some(sanitize(text)),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        return json!(text);
    }
    convert_user_blocks(blocks, supports_images, NON_VISION_TOOL_IMAGE_PLACEHOLDER)
}

fn convert_tools(
    tools: &[ToolSchema],
    is_oauth: bool,
    eager: bool,
    cache_control: Option<&Value>,
) -> Value {
    let last_idx = tools.len().saturating_sub(1);
    let result: Vec<Value> = tools
        .iter()
        .enumerate()
        .map(|(i, tool)| {
            let name = if is_oauth {
                to_claude_code_name(&tool.name)
            } else {
                tool.name.clone()
            };
            let props = tool
                .parameters
                .get("properties")
                .cloned()
                .unwrap_or(json!({}));
            let req = tool
                .parameters
                .get("required")
                .cloned()
                .unwrap_or(json!([]));

            let mut t = json!({
                "name": name,
                "description": tool.description,
                "input_schema": { "type": "object", "properties": props, "required": req },
            });
            if eager {
                t["eager_input_streaming"] = json!(true);
            }
            if i == last_idx {
                if let Some(cc) = cache_control {
                    t["cache_control"] = cc.clone();
                }
            }
            t
        })
        .collect();
    Value::Array(result)
}

fn build_system_prompt(
    context: &Context,
    is_oauth: bool,
    cache_control: &Option<Value>,
    payload: &mut Value,
) {
    if is_oauth {
        let mut blocks = vec![{
            let mut b = json!({
                "type": "text",
                "text": "You are Claude Code, Anthropic's official CLI for Claude.",
            });
            if let Some(cc) = cache_control {
                b["cache_control"] = cc.clone();
            }
            b
        }];
        if let Some(prompt) = &context.system_prompt {
            let mut b = json!({ "type": "text", "text": sanitize(prompt) });
            if let Some(cc) = cache_control {
                b["cache_control"] = cc.clone();
            }
            blocks.push(b);
        }
        payload["system"] = Value::Array(blocks);
    } else if let Some(prompt) = &context.system_prompt {
        let mut b = json!({ "type": "text", "text": sanitize(prompt) });
        if let Some(cc) = cache_control {
            b["cache_control"] = cc.clone();
        }
        payload["system"] = json!([b]);
    }
}

fn build_thinking_config(
    model: &Model,
    options: &SimpleStreamOptions,
    compat: &AnthropicCompat,
    enabled: bool,
    payload: &mut Value,
) {
    if enabled {
        if compat.force_adaptive_thinking {
            payload["thinking"] = json!({ "type": "adaptive", "display": "summarized" });
            if let Some(effort) = map_thinking_effort_to_effort(model, options.reasoning) {
                payload["output_config"] = json!({ "effort": effort });
            }
        } else {
            let budget = resolve_thinking_budget(options);
            payload["thinking"] = json!({
                "type": "enabled", "budget_tokens": budget, "display": "summarized",
            });
        }
    } else if options.reasoning == Some(ThinkingEffort::Off) {
        payload["thinking"] = json!({ "type": "disabled" });
    }
}

fn should_enable_thinking(model: &Model, options: &SimpleStreamOptions) -> bool {
    model.reasoning && matches!(options.reasoning, Some(effort) if effort != ThinkingEffort::Off)
}

fn resolve_thinking_budget(options: &SimpleStreamOptions) -> u64 {
    if let Some(budgets) = &options.thinking_effort_budgets {
        match options.reasoning {
            Some(ThinkingEffort::Low) => budgets.low.unwrap_or(1024),
            Some(ThinkingEffort::Medium) => budgets.medium.unwrap_or(1024),
            Some(ThinkingEffort::High) => budgets.high.unwrap_or(1024),
            Some(ThinkingEffort::XHigh) => budgets.xhigh.or(budgets.high).unwrap_or(1024),
            Some(ThinkingEffort::Max) => budgets
                .max
                .or(budgets.xhigh)
                .or(budgets.high)
                .unwrap_or(1024),
            _ => 1024,
        }
    } else {
        1024
    }
}

fn map_thinking_effort_to_effort(model: &Model, effort: Option<ThinkingEffort>) -> Option<String> {
    if let Some(effort) = effort {
        if let Some(map) = &model.thinking_effort_map {
            if let Some(mapped) = map.get(&effort) {
                return mapped.clone();
            }
        }
    }
    match effort {
        Some(ThinkingEffort::Low) => Some("low".to_string()),
        Some(ThinkingEffort::Medium) => Some("medium".to_string()),
        Some(ThinkingEffort::High) => Some("high".to_string()),
        Some(ThinkingEffort::XHigh) => Some("xhigh".to_string()),
        Some(ThinkingEffort::Max) => Some("max".to_string()),
        _ => None,
    }
}

fn should_use_fine_grained_tool_streaming(context: &Context, compat: &AnthropicCompat) -> bool {
    !context.tools.is_empty() && !compat.supports_eager_tool_input_streaming
}

fn build_cache_control(retention: CacheRetention, compat: &AnthropicCompat) -> Option<Value> {
    match retention {
        CacheRetention::None => None,
        CacheRetention::Long if compat.supports_long_cache_retention => {
            Some(json!({ "type": "ephemeral", "ttl": "1h" }))
        }
        _ => Some(json!({ "type": "ephemeral" })),
    }
}

fn resolve_cache_retention(options: &StreamOptions) -> CacheRetention {
    if options.cache_retention != CacheRetention::None {
        return options.cache_retention;
    }
    if std::env::var("ROZSA_CACHE_RETENTION").ok().as_deref() == Some("long") {
        return CacheRetention::Long;
    }
    CacheRetention::Short
}

fn compat_bool(compat: Option<&Value>, key: &str) -> Option<bool> {
    compat.and_then(|v| v.get(key)).and_then(|v| v.as_bool())
}

fn provider_str(provider: &Provider) -> &str {
    match provider {
        Provider::Anthropic => "anthropic",
        Provider::CloudflareAIGateway => "cloudflare-ai-gateway",
        Provider::Custom(v) => v.as_str(),
        _ => "",
    }
}

fn sanitize(input: &str) -> String {
    input.to_string()
}
