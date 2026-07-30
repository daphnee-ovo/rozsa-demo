//! Unit tests for the Bedrock Converse Stream provider.
//!
//! Tests cover payload construction, cache points, thinking parameters,
//! and stream event mapping. Live AWS calls are not made; tests verify
//! the payload building and event parsing logic.

use serde_json::json;

use rozsa_model::providers::bedrock::BedrockProvider;
use rozsa_model::providers::bedrock::payload::{
    build_converse_stream_input, is_anthropic_claude_model,
};
use rozsa_model::registry::ApiProvider;
use rozsa_model::types::{
    Api, CacheRetention, ContentBlock, Context, InputModality, Message, Model, ModelCost, Provider,
    SimpleStreamOptions, StreamOptions, ThinkingEffort, ToolCall, ToolSchema, Transport,
    UserContent, UserMessage,
};

fn bedrock_model(id: &str, name: &str, reasoning: bool) -> Model {
    Model {
        id: id.to_string(),
        name: name.to_string(),
        api: Api::BedrockConverseStream,
        provider: Provider::AmazonBedrock,
        base_url: "https://bedrock-runtime.us-east-1.amazonaws.com".to_string(),
        reasoning,
        input_modalities: vec![InputModality::Text, InputModality::Image],
        cost: ModelCost {
            input: 3.0,
            output: 15.0,
            cache_read: 0.3,
            cache_write: 3.75,
        },
        context_window: 200_000,
        max_tokens: 8192,
        thinking_effort_map: None,
        headers: None,
        compat: None,
    }
}

fn test_options() -> SimpleStreamOptions {
    SimpleStreamOptions {
        base: StreamOptions {
            temperature: None,
            max_tokens: None,
            api_key: None,
            transport: Transport::Sse,
            cache_retention: CacheRetention::Short,
            session_id: None,
            headers: None,
            timeout_ms: None,
            max_retries: None,
            max_retry_delay_ms: None,
            metadata: None,
        },
        reasoning: None,
        thinking_effort_budgets: None,
        tool_choice: None,
    }
}

fn basic_context() -> Context {
    Context {
        system_prompt: Some("You are a helpful assistant.".to_string()),
        messages: vec![Message::User(UserMessage {
            content: UserContent::Text("Hello".to_string()),
            display_text: None,
            timestamp: 0,
        })],
        tools: vec![],
    }
}

#[test]
fn provider_returns_correct_api() {
    let provider = BedrockProvider::new();
    assert_eq!(provider.api(), &Api::BedrockConverseStream);
}

#[test]
fn is_claude_model_detection() {
    let claude_model = bedrock_model(
        "anthropic.claude-3-5-sonnet-20241022-v2:0",
        "Claude 3.5 Sonnet",
        true,
    );
    assert!(is_anthropic_claude_model(&claude_model));

    let nova_model = bedrock_model("amazon.nova-2-lite-v1:0", "Nova 2 Lite", false);
    assert!(!is_anthropic_claude_model(&nova_model));

    let arn_model = bedrock_model(
        "arn:aws:bedrock:us-east-1:123456:inference-profile/my-profile",
        "Claude",
        true,
    );
    assert!(is_anthropic_claude_model(&arn_model));
}

#[test]
fn payload_basic_messages() {
    let model = bedrock_model(
        "anthropic.claude-3-5-sonnet-20241022-v2:0",
        "Claude 3.5 Sonnet",
        true,
    );
    let context = basic_context();
    let options = test_options();

    let input = build_converse_stream_input(&model, &context, &options).unwrap();

    assert_eq!(input.messages.len(), 1);
    assert!(input.system.is_some());
    assert!(input.inference_config.is_some());
    assert!(input.tool_config.is_none());
}

#[test]
fn payload_system_prompt_has_cache_point_for_claude() {
    // Claude 3.5 Haiku supports cache points.
    let model = bedrock_model(
        "anthropic.claude-3-5-haiku-20241022-v1:0",
        "Claude 3.5 Haiku",
        false,
    );
    let context = basic_context();
    let options = test_options();

    let input = build_converse_stream_input(&model, &context, &options).unwrap();

    let system = input.system.unwrap();
    // System prompt text + cache point = 2 blocks.
    assert_eq!(system.len(), 2);
}

#[test]
fn payload_no_cache_point_for_non_claude() {
    let model = bedrock_model("amazon.nova-2-lite-v1:0", "Nova 2 Lite", false);
    let context = basic_context();
    let options = test_options();

    let input = build_converse_stream_input(&model, &context, &options).unwrap();

    let system = input.system.unwrap();
    // Only text block, no cache point.
    assert_eq!(system.len(), 1);
}

#[test]
fn payload_no_cache_point_when_retention_none() {
    let model = bedrock_model(
        "anthropic.claude-3-5-sonnet-20241022-v2:0",
        "Claude 3.5 Sonnet",
        true,
    );
    let context = basic_context();
    let mut options = test_options();
    options.base.cache_retention = CacheRetention::None;

    let input = build_converse_stream_input(&model, &context, &options).unwrap();

    let system = input.system.unwrap();
    assert_eq!(system.len(), 1);
}

#[test]
fn payload_tools_converted() {
    let model = bedrock_model(
        "anthropic.claude-3-5-sonnet-20241022-v2:0",
        "Claude 3.5 Sonnet",
        true,
    );
    let context = Context {
        system_prompt: Some("System".to_string()),
        messages: vec![Message::User(UserMessage {
            content: UserContent::Text("Use the tool".to_string()),
            display_text: None,
            timestamp: 0,
        })],
        tools: vec![ToolSchema {
            name: "get_weather".to_string(),
            description: "Get weather".to_string(),
            parameters: json!({"type": "object", "properties": {"city": {"type": "string"}}}),
        }],
    };
    let options = test_options();

    let input = build_converse_stream_input(&model, &context, &options).unwrap();

    assert!(input.tool_config.is_some());
}

#[test]
fn payload_thinking_adaptive_for_opus_4() {
    let model = bedrock_model("anthropic.claude-opus-4-6-v1:0", "Claude Opus 4.6", true);
    let context = basic_context();
    let mut options = test_options();
    options.reasoning = Some(ThinkingEffort::High);

    let input = build_converse_stream_input(&model, &context, &options).unwrap();

    let fields = input.additional_model_request_fields.unwrap();
    let doc_str = format!("{fields:?}");
    assert!(doc_str.contains("adaptive"));
    assert!(doc_str.contains("high"));
}

#[test]
fn payload_thinking_budget_for_older_claude() {
    let model = bedrock_model(
        "anthropic.claude-3-7-sonnet-20250219-v1:0",
        "Claude 3.7 Sonnet",
        true,
    );
    let context = basic_context();
    let mut options = test_options();
    options.reasoning = Some(ThinkingEffort::Medium);

    let input = build_converse_stream_input(&model, &context, &options).unwrap();

    let fields = input.additional_model_request_fields.unwrap();
    let doc_str = format!("{fields:?}");
    assert!(doc_str.contains("enabled"));
    assert!(doc_str.contains("budget_tokens"));
}

#[test]
fn payload_no_thinking_when_reasoning_none() {
    let model = bedrock_model("anthropic.claude-opus-4-6-v1:0", "Claude Opus 4.6", true);
    let context = basic_context();
    let options = test_options();

    let input = build_converse_stream_input(&model, &context, &options).unwrap();

    assert!(input.additional_model_request_fields.is_none());
}

#[test]
fn payload_no_thinking_for_non_reasoning_model() {
    let model = bedrock_model(
        "anthropic.claude-3-5-haiku-20241022-v1:0",
        "Claude 3.5 Haiku",
        false,
    );
    let context = basic_context();
    let mut options = test_options();
    options.reasoning = Some(ThinkingEffort::High);

    let input = build_converse_stream_input(&model, &context, &options).unwrap();

    assert!(input.additional_model_request_fields.is_none());
}

#[test]
fn payload_consecutive_tool_results_merged() {
    let model = bedrock_model(
        "anthropic.claude-3-5-sonnet-20241022-v2:0",
        "Claude 3.5 Sonnet",
        true,
    );
    let context = Context {
        system_prompt: None,
        messages: vec![
            Message::User(UserMessage {
                content: UserContent::Text("Use tools".to_string()),
                display_text: None,
                timestamp: 0,
            }),
            Message::Assistant(rozsa_model::types::AssistantMessage {
                content: vec![
                    ContentBlock::ToolCall(ToolCall {
                        id: "tc1".to_string(),
                        name: "tool1".to_string(),
                        arguments: json!({}),
                    }),
                    ContentBlock::ToolCall(ToolCall {
                        id: "tc2".to_string(),
                        name: "tool2".to_string(),
                        arguments: json!({}),
                    }),
                ],
                api: Api::BedrockConverseStream,
                provider: Provider::AmazonBedrock,
                model: "test".to_string(),
                response_model: None,
                response_id: None,
                usage: rozsa_model::types::Usage {
                    input: 0,
                    output: 0,
                    cache_read: 0,
                    cache_write: 0,
                    total_tokens: 0,
                    cost: rozsa_model::types::UsageCost {
                        input: 0.0,
                        output: 0.0,
                        cache_read: 0.0,
                        cache_write: 0.0,
                        total: 0.0,
                    },
                },
                stop_reason: rozsa_model::types::StopReason::ToolUse,
                error_message: None,
                timestamp: 0,
            }),
            Message::ToolResult(rozsa_model::types::ToolResultMessage {
                tool_call_id: "tc1".to_string(),
                tool_name: "tool1".to_string(),
                content: vec![ContentBlock::Text {
                    text: "result1".to_string(),
                    signature: None,
                }],
                details: serde_json::Value::Null,
                is_error: false,
                timestamp: 0,
            }),
            Message::ToolResult(rozsa_model::types::ToolResultMessage {
                tool_call_id: "tc2".to_string(),
                tool_name: "tool2".to_string(),
                content: vec![ContentBlock::Text {
                    text: "result2".to_string(),
                    signature: None,
                }],
                details: serde_json::Value::Null,
                is_error: false,
                timestamp: 0,
            }),
        ],
        tools: vec![],
    };
    let options = test_options();

    let input = build_converse_stream_input(&model, &context, &options).unwrap();

    // user + assistant + user(2 tool results merged) = 3 messages.
    assert_eq!(input.messages.len(), 3);
}
