//! Bedrock ConverseStream payload construction.
//!
//! Converts Context + SimpleStreamOptions into the SDK input structure.

use aws_sdk_bedrockruntime::types::{
    CachePointBlock, CachePointType, CacheTtl, ContentBlock, ConversationRole,
    InferenceConfiguration, Message, SystemContentBlock, Tool as BedrockTool,
    ToolConfiguration, ToolInputSchema, ToolSpecification,
};
use aws_smithy_types::Document;

use crate::providers::common::ProviderError;
use crate::types::{CacheRetention, Context, Model, SimpleStreamOptions};

pub struct ConverseStreamInput {
    pub messages: Vec<Message>,
    pub system: Option<Vec<SystemContentBlock>>,
    pub inference_config: Option<InferenceConfiguration>,
    pub tool_config: Option<ToolConfiguration>,
    pub additional_model_request_fields: Option<Document>,
}

pub fn build_converse_stream_input(
    model: &Model,
    context: &Context,
    options: &SimpleStreamOptions,
) -> Result<ConverseStreamInput, ProviderError> {
    let cache_retention = resolve_cache_retention(options);
    let use_cache = cache_retention != CacheRetention::None && supports_prompt_caching(model);

    let mut messages = convert_messages(context);
    let system = build_system_prompt(context, use_cache, cache_retention);
    let inference_config = build_inference_config(model, options);
    let tool_config = convert_tool_config(context);
    let additional_model_request_fields = build_additional_model_request_fields(model, options);

    // Add cache point to the last user message.
    if use_cache {
        if let Some(last) = messages.last_mut() {
            if matches!(last.role(), ConversationRole::User) {
                let mut content = last.content().to_vec();
                content.push(ContentBlock::CachePoint(build_cache_point(cache_retention)));
                *last = Message::builder()
                    .role(ConversationRole::User)
                    .set_content(Some(content))
                    .build()
                    .unwrap();
            }
        }
    }

    Ok(ConverseStreamInput {
        messages,
        system,
        inference_config,
        tool_config,
        additional_model_request_fields,
    })
}

fn convert_messages(context: &Context) -> Vec<Message> {
    let mut result = Vec::new();

    for msg in &context.messages {
        match msg {
            crate::types::Message::User(user_msg) => {
                let content = match &user_msg.content {
                    crate::types::UserContent::Text(text) => {
                        vec![ContentBlock::Text(text.clone())]
                    }
                    crate::types::UserContent::Blocks(blocks) => {
                        blocks
                            .iter()
                            .filter_map(|b| match b {
                                crate::types::ContentBlock::Text { text, .. } => {
                                    Some(ContentBlock::Text(text.clone()))
                                }
                                crate::types::ContentBlock::Image { data, mime_type } => {
                                    let format = match mime_type.as_str() {
                                        "image/jpeg" | "image/jpg" => {
                                            aws_sdk_bedrockruntime::types::ImageFormat::Jpeg
                                        }
                                        "image/png" => {
                                            aws_sdk_bedrockruntime::types::ImageFormat::Png
                                        }
                                        "image/gif" => {
                                            aws_sdk_bedrockruntime::types::ImageFormat::Gif
                                        }
                                        "image/webp" => {
                                            aws_sdk_bedrockruntime::types::ImageFormat::Webp
                                        }
                                        _ => return None,
                                    };
                                    use aws_sdk_bedrockruntime::primitives::Blob;
                                    let bytes = aws_smithy_types::base64::decode(data)
                                        .unwrap_or_default();
                                    Some(ContentBlock::Image(
                                        aws_sdk_bedrockruntime::types::ImageBlock::builder()
                                            .format(format)
                                            .source(
                                                aws_sdk_bedrockruntime::types::ImageSource::Bytes(
                                                    Blob::new(bytes),
                                                ),
                                            )
                                            .build()
                                            .unwrap(),
                                    ))
                                }
                                _ => None,
                            })
                            .collect()
                    }
                };
                if content.is_empty() {
                    continue;
                }
                result.push(
                    Message::builder()
                        .role(ConversationRole::User)
                        .set_content(Some(content))
                        .build()
                        .unwrap(),
                );
            }
            crate::types::Message::Assistant(assistant_msg) => {
                let content: Vec<ContentBlock> = assistant_msg
                    .content
                    .iter()
                    .filter_map(|b| match b {
                        crate::types::ContentBlock::Text { text, .. } => {
                            if text.trim().is_empty() {
                                None
                            } else {
                                Some(ContentBlock::Text(text.clone()))
                            }
                        }
                        crate::types::ContentBlock::ToolCall(tc) => {
                            let input_doc = json_value_to_document(&tc.arguments);
                            Some(ContentBlock::ToolUse(
                                aws_sdk_bedrockruntime::types::ToolUseBlock::builder()
                                    .tool_use_id(&tc.id)
                                    .name(&tc.name)
                                    .input(input_doc)
                                    .build()
                                    .unwrap(),
                            ))
                        }
                        crate::types::ContentBlock::Thinking {
                            thinking,
                            signature,
                            ..
                        } => {
                            if thinking.trim().is_empty() {
                                None
                            } else if let Some(sig) = signature.as_ref().filter(|s| !s.trim().is_empty()) {
                                Some(ContentBlock::ReasoningContent(
                                    aws_sdk_bedrockruntime::types::ReasoningContentBlock::ReasoningText(
                                        aws_sdk_bedrockruntime::types::ReasoningTextBlock::builder()
                                            .text(thinking)
                                            .signature(sig)
                                            .build()
                                            .unwrap(),
                                    ),
                                ))
                            } else {
                                Some(ContentBlock::Text(thinking.clone()))
                            }
                        }
                        _ => None,
                    })
                    .collect();
                if content.is_empty() {
                    continue;
                }
                result.push(
                    Message::builder()
                        .role(ConversationRole::Assistant)
                        .set_content(Some(content))
                        .build()
                        .unwrap(),
                );
            }
            crate::types::Message::ToolResult(tool_result) => {
                let content_blocks: Vec<_> = tool_result
                    .content
                    .iter()
                    .filter_map(|b| match b {
                        crate::types::ContentBlock::Text { text, .. } => {
                            Some(aws_sdk_bedrockruntime::types::ToolResultContentBlock::Text(
                                text.clone(),
                            ))
                        }
                        _ => None,
                    })
                    .collect();
                let status = if tool_result.is_error {
                    aws_sdk_bedrockruntime::types::ToolResultStatus::Error
                } else {
                    aws_sdk_bedrockruntime::types::ToolResultStatus::Success
                };
                let tool_result_block =
                    aws_sdk_bedrockruntime::types::ToolResultBlock::builder()
                        .tool_use_id(&tool_result.tool_call_id)
                        .set_content(Some(content_blocks))
                        .status(status)
                        .build()
                        .unwrap();
                // Consecutive tool results should be in a single user message.
                // Check if last message is a user message we can append to.
                if let Some(last) = result.last_mut() {
                    if matches!(last.role(), ConversationRole::User) {
                        // Clone and append
                        let mut content = last.content().to_vec();
                        content.push(ContentBlock::ToolResult(tool_result_block));
                        *last = Message::builder()
                            .role(ConversationRole::User)
                            .set_content(Some(content))
                            .build()
                            .unwrap();
                        continue;
                    }
                }
                result.push(
                    Message::builder()
                        .role(ConversationRole::User)
                        .set_content(Some(vec![ContentBlock::ToolResult(tool_result_block)]))
                        .build()
                        .unwrap(),
                );
            }
        }
    }

    result
}

fn build_system_prompt(
    context: &Context,
    use_cache: bool,
    cache_retention: CacheRetention,
) -> Option<Vec<SystemContentBlock>> {
    let system_prompt = context.system_prompt.as_ref()?;
    if system_prompt.is_empty() {
        return None;
    }
    let mut blocks = vec![SystemContentBlock::Text(system_prompt.clone())];
    if use_cache {
        blocks.push(SystemContentBlock::CachePoint(build_cache_point(
            cache_retention,
        )));
    }
    Some(blocks)
}

fn build_inference_config(
    model: &Model,
    options: &SimpleStreamOptions,
) -> Option<InferenceConfiguration> {
    let max_tokens = options.base.max_tokens.or(Some(model.max_tokens));
    let temperature = options.base.temperature;

    if max_tokens.is_none() && temperature.is_none() {
        return None;
    }

    let mut builder = InferenceConfiguration::builder();
    if let Some(mt) = max_tokens {
        builder = builder.max_tokens(mt as i32);
    }
    if let Some(temp) = temperature {
        builder = builder.temperature(temp as f32);
    }
    Some(builder.build())
}

fn convert_tool_config(context: &Context) -> Option<ToolConfiguration> {
    if context.tools.is_empty() {
        return None;
    }

    let tools: Vec<BedrockTool> = context
        .tools
        .iter()
        .map(|tool| {
            let input_schema = json_value_to_document(&tool.parameters);
            BedrockTool::ToolSpec(
                ToolSpecification::builder()
                    .name(&tool.name)
                    .description(&tool.description)
                    .input_schema(ToolInputSchema::Json(input_schema))
                    .build()
                    .unwrap(),
            )
        })
        .collect();

    Some(
        ToolConfiguration::builder()
            .set_tools(Some(tools))
            .build()
            .unwrap(),
    )
}

fn build_additional_model_request_fields(
    model: &Model,
    options: &SimpleStreamOptions,
) -> Option<Document> {
    let reasoning = options.reasoning.as_ref()?;
    if !model.reasoning {
        return None;
    }

    if !is_anthropic_claude_model(model) {
        return None;
    }

    let candidates = model_match_candidates(&model.id, &model.name);

    if supports_adaptive_thinking(&candidates) {
        let effort = map_thinking_level_to_effort(model, reasoning);
        let display = if is_govcloud_model(model) {
            None
        } else {
            Some("summarized")
        };
        let mut thinking_obj = std::collections::HashMap::new();
        thinking_obj.insert("type".to_string(), Document::String("adaptive".to_string()));
        if let Some(d) = display {
            thinking_obj.insert("display".to_string(), Document::String(d.to_string()));
        }
        let mut output_config = std::collections::HashMap::new();
        output_config.insert("effort".to_string(), Document::String(effort.to_string()));
        let mut fields = std::collections::HashMap::new();
        fields.insert("thinking".to_string(), Document::Object(thinking_obj));
        fields.insert("output_config".to_string(), Document::Object(output_config));

        // Interleaved thinking for non-adaptive is handled below; adaptive doesn't need it.
        Some(Document::Object(fields))
    } else {
        // Budget-based thinking for older Claude models.
        let budget = resolve_thinking_budget(reasoning, options);
        let display = if is_govcloud_model(model) {
            None
        } else {
            Some("summarized")
        };
        let mut thinking_obj = std::collections::HashMap::new();
        thinking_obj.insert("type".to_string(), Document::String("enabled".to_string()));
        thinking_obj.insert(
            "budget_tokens".to_string(),
            Document::Number(aws_smithy_types::Number::NegInt(budget as i64)),
        );
        if let Some(d) = display {
            thinking_obj.insert("display".to_string(), Document::String(d.to_string()));
        }
        let mut fields = std::collections::HashMap::new();
        fields.insert("thinking".to_string(), Document::Object(thinking_obj));

        // Interleaved thinking beta for non-adaptive models.
        fields.insert(
            "anthropic_beta".to_string(),
            Document::Array(vec![Document::String(
                "interleaved-thinking-2025-05-14".to_string(),
            )]),
        );

        Some(Document::Object(fields))
    }
}

fn supports_adaptive_thinking(candidates: &[String]) -> bool {
    candidates.iter().any(|s| {
        s.contains("opus-4-6")
            || s.contains("opus-4-7")
            || s.contains("opus-4-8")
            || s.contains("sonnet-4-6")
    })
}

fn is_govcloud_model(model: &Model) -> bool {
    let id = model.id.to_lowercase();
    id.starts_with("us-gov.") || id.starts_with("arn:aws-us-gov:")
}

fn map_thinking_level_to_effort(
    model: &Model,
    level: &crate::types::ThinkingLevel,
) -> &'static str {
    use crate::types::ThinkingLevel;

    // Check model-specific mapping first.
    if let Some(map) = &model.thinking_level_map {
        if let Some(Some(mapped)) = map.get(level) {
            return match mapped.as_str() {
                "low" => "low",
                "medium" => "medium",
                "high" => "high",
                "xhigh" => "xhigh",
                "max" => "max",
                _ => "high",
            };
        }
    }

    match level {
        ThinkingLevel::Off => "low",
        ThinkingLevel::Minimal | ThinkingLevel::Low => "low",
        ThinkingLevel::Medium => "medium",
        ThinkingLevel::High => "high",
        ThinkingLevel::XHigh => {
            let candidates = model_match_candidates(&model.id, &model.name);
            if candidates.iter().any(|s| s.contains("opus-4-7") || s.contains("opus-4-8")) {
                "xhigh"
            } else {
                "high"
            }
        }
    }
}

fn resolve_thinking_budget(
    level: &crate::types::ThinkingLevel,
    options: &SimpleStreamOptions,
) -> u64 {
    use crate::types::ThinkingLevel;

    // Check custom budgets from options.
    if let Some(budgets) = &options.thinking_budgets {
        let budget = match level {
            ThinkingLevel::Minimal => budgets.minimal,
            ThinkingLevel::Low => budgets.low,
            ThinkingLevel::Medium => budgets.medium,
            ThinkingLevel::High | ThinkingLevel::XHigh => budgets.high,
            ThinkingLevel::Off => None,
        };
        if let Some(b) = budget {
            return b;
        }
    }

    // Default budgets.
    match level {
        ThinkingLevel::Off => 1024,
        ThinkingLevel::Minimal => 1024,
        ThinkingLevel::Low => 2048,
        ThinkingLevel::Medium => 8192,
        ThinkingLevel::High | ThinkingLevel::XHigh => 16384,
    }
}

fn resolve_cache_retention(options: &SimpleStreamOptions) -> CacheRetention {
    if options.base.cache_retention != CacheRetention::None {
        return options.base.cache_retention;
    }
    if std::env::var("ROZSA_CACHE_RETENTION").ok().as_deref() == Some("long") {
        return CacheRetention::Long;
    }
    // Default for Bedrock: short caching enabled.
    CacheRetention::Short
}

fn supports_prompt_caching(model: &Model) -> bool {
    if std::env::var("AWS_BEDROCK_FORCE_CACHE").ok().as_deref() == Some("1") {
        return true;
    }
    let candidates = model_match_candidates(&model.id, &model.name);
    let has_claude = candidates.iter().any(|s| s.contains("claude"));
    if !has_claude {
        return false;
    }
    // Claude 4.x
    if candidates.iter().any(|s| s.contains("-4-")) {
        return true;
    }
    // Claude 3.7 Sonnet
    if candidates.iter().any(|s| s.contains("claude-3-7-sonnet")) {
        return true;
    }
    // Claude 3.5 Haiku
    if candidates.iter().any(|s| s.contains("claude-3-5-haiku")) {
        return true;
    }
    false
}

fn build_cache_point(cache_retention: CacheRetention) -> CachePointBlock {
    let mut builder = CachePointBlock::builder().r#type(CachePointType::Default);
    if cache_retention == CacheRetention::Long {
        builder = builder.ttl(CacheTtl::OneHour);
    }
    builder.build().unwrap()
}

pub fn is_anthropic_claude_model(model: &Model) -> bool {
    let candidates = model_match_candidates(&model.id, &model.name);
    candidates.iter().any(|s| {
        s.contains("anthropic.claude")
            || s.contains("anthropic/claude")
            || s.contains("claude")
    })
}

fn model_match_candidates(id: &str, name: &str) -> Vec<String> {
    let values = if name.is_empty() {
        vec![id.to_string()]
    } else {
        vec![id.to_string(), name.to_string()]
    };
    values
        .into_iter()
        .flat_map(|v| {
            let lower = v.to_lowercase();
            let normalized = lower.replace(|c: char| c == ' ' || c == '_' || c == '.' || c == ':', "-");
            if lower == normalized {
                vec![lower]
            } else {
                vec![lower, normalized]
            }
        })
        .collect()
}

fn json_value_to_document(value: &serde_json::Value) -> Document {
    match value {
        serde_json::Value::Null => Document::Null,
        serde_json::Value::Bool(b) => Document::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Document::Number(aws_smithy_types::Number::NegInt(i))
            } else if let Some(f) = n.as_f64() {
                Document::Number(aws_smithy_types::Number::Float(f))
            } else {
                Document::Null
            }
        }
        serde_json::Value::String(s) => Document::String(s.clone()),
        serde_json::Value::Array(arr) => {
            Document::Array(arr.iter().map(json_value_to_document).collect())
        }
        serde_json::Value::Object(obj) => {
            let map = obj
                .iter()
                .map(|(k, v)| (k.clone(), json_value_to_document(v)))
                .collect();
            Document::Object(map)
        }
    }
}
