use serde_json::json;

use rozsa_model::providers::common::create_output;
use rozsa_model::providers::openai_completions::{
    MaxTokensField, OpenAICompletionsProvider, SseParser, ThinkingFormat,
    build_chat_completions_payload, normalize_chat_chunks, parse_sse_chunks, request_headers,
    resolve_compat, should_retry_status,
};
use rozsa_model::registry::{ApiProvider, get_provider, register_provider};
use rozsa_model::types::{
    Api, CacheRetention, Context, InputModality, Message, Model, ModelCost, Provider,
    SimpleStreamOptions, StreamEvent, StreamOptions, ToolSchema, Transport, UserContent,
    UserMessage,
};

fn test_model(base_url: &str) -> Model {
    Model {
        id: "test-model".to_string(),
        name: "Test Model".to_string(),
        api: Api::OpenAICompletions,
        provider: Provider::OpenAI,
        base_url: base_url.to_string(),
        reasoning: false,
        input_modalities: vec![InputModality::Text],
        cost: ModelCost {
            input: 1.0,
            output: 2.0,
            cache_read: 0.5,
            cache_write: 1.5,
        },
        context_window: 128_000,
        max_tokens: 16_384,
        thinking_level_map: None,
        headers: None,
        compat: None,
    }
}

fn test_options() -> SimpleStreamOptions {
    SimpleStreamOptions {
        base: StreamOptions {
            temperature: None,
            max_tokens: None,
            api_key: Some("test-key".to_string()),
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
        thinking_budgets: None,
        tool_choice: None,
    }
}

#[test]
fn builds_openai_chat_payload_with_tools() {
    let model = test_model("http://127.0.0.1/v1");
    let context = Context {
        system_prompt: Some("Be direct.".to_string()),
        messages: vec![Message::User(UserMessage {
            content: UserContent::Text("hello".to_string()),
            display_text: None,
            timestamp: 1,
        })],
        tools: vec![ToolSchema {
            name: "lookup".to_string(),
            description: "Lookup value".to_string(),
            parameters: json!({ "type": "object", "properties": {} }),
        }],
    };

    let payload = build_chat_completions_payload(&model, &context, &test_options());

    assert_eq!(payload["model"], "test-model");
    assert_eq!(payload["messages"][0]["role"], "system");
    assert_eq!(payload["messages"][1]["role"], "user");
    assert_eq!(payload["tools"][0]["function"]["name"], "lookup");
}

#[test]
fn builds_proxy_specific_payload_fields() {
    let mut model = test_model("https://openrouter.ai/api/v1");
    model.provider = Provider::OpenRouter;
    model.reasoning = true;
    model.id = "anthropic/claude-test".to_string();
    model.compat = Some(json!({
        "openRouterRouting": { "only": ["anthropic"] },
        "zaiToolStream": true,
        "cacheControlFormat": "anthropic"
    }));
    let mut options = test_options();
    options.base.cache_retention = CacheRetention::Long;
    options.tool_choice = Some(json!("required"));
    let context = Context {
        system_prompt: Some("Cache me.".to_string()),
        messages: vec![Message::User(UserMessage {
            content: UserContent::Text("hello".to_string()),
            display_text: None,
            timestamp: 1,
        })],
        tools: vec![ToolSchema {
            name: "lookup".to_string(),
            description: "Lookup value".to_string(),
            parameters: json!({ "type": "object" }),
        }],
    };

    let payload = build_chat_completions_payload(&model, &context, &options);

    assert_eq!(payload["tool_choice"], "required");
    assert_eq!(payload["tool_stream"], true);
    assert_eq!(payload["provider"]["only"], json!(["anthropic"]));
    assert_eq!(
        payload["messages"][0]["content"][0]["cache_control"]["ttl"],
        "1h"
    );
    assert_eq!(payload["tools"][0]["cache_control"]["ttl"], "1h");
}

#[test]
fn builds_vercel_gateway_routing_payload() {
    let mut model = test_model("https://ai-gateway.vercel.sh/v1");
    model.compat = Some(json!({
        "vercelGatewayRouting": { "only": ["openai"], "order": ["openai", "anthropic"] }
    }));

    let payload = build_chat_completions_payload(
        &model,
        &Context {
            system_prompt: None,
            messages: vec![Message::User(UserMessage {
                content: UserContent::Text("hello".to_string()),
                display_text: None,
                timestamp: 1,
            })],
            tools: Vec::new(),
        },
        &test_options(),
    );

    assert_eq!(
        payload["providerOptions"]["gateway"]["only"],
        json!(["openai"])
    );
    assert_eq!(
        payload["providerOptions"]["gateway"]["order"],
        json!(["openai", "anthropic"])
    );
}

#[test]
fn parses_data_events_and_done_marker() {
    let input = concat!(
        "data: {\"id\":\"one\",\"choices\":[{\"delta\":{\"content\":\"hi\"},\"finish_reason\":null}]}\n\n",
        "data: [DONE]\n\n",
    );

    let chunks = parse_sse_chunks(input).unwrap();

    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].id.as_deref(), Some("one"));
    assert_eq!(chunks[0].choices[0].delta.content.as_deref(), Some("hi"));
}

#[test]
fn parses_sse_events_incrementally() {
    let mut parser = SseParser::new();
    let mut chunks = parser
        .feed("data: {\"id\":\"one\",\"choices\":[{\"delta\":{\"content\":\"he")
        .unwrap();
    assert!(chunks.is_empty());

    chunks.extend(
        parser
            .feed("llo\"},\"finish_reason\":null}]}\n\ndata: [DONE]\n\n")
            .unwrap(),
    );
    chunks.extend(parser.finish().unwrap());

    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].id.as_deref(), Some("one"));
    assert_eq!(chunks[0].choices[0].delta.content.as_deref(), Some("hello"));
}

#[test]
fn normalizes_openai_compatible_sse_into_stream_events() {
    let model = test_model("http://127.0.0.1/v1");
    let body = concat!(
        "data: {\"id\":\"chatcmpl_1\",\"model\":\"served-model\",\"choices\":[{\"delta\":{\"content\":\"hello\"},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":2,\"prompt_tokens_details\":{\"cached_tokens\":1}}}\n\n",
        "data: [DONE]\n\n",
    );
    let chunks = parse_sse_chunks(body).unwrap();

    let events = normalize_chat_chunks(
        &model,
        create_output(&model, Api::OpenAICompletions),
        chunks,
    )
    .unwrap();

    let mut saw_text = false;
    let mut done = None;
    for event in events {
        match event {
            StreamEvent::TextDelta { delta, .. } if delta == "hello" => saw_text = true,
            StreamEvent::Done { message, .. } => {
                done = Some(message);
                break;
            }
            StreamEvent::Error { error, .. } => {
                panic!("unexpected stream error: {:?}", error.error_message)
            }
            _ => {}
        }
    }

    let message = done.expect("done event");
    assert!(saw_text);
    assert_eq!(message.response_id.as_deref(), Some("chatcmpl_1"));
    assert_eq!(message.response_model.as_deref(), Some("served-model"));
    assert_eq!(message.usage.input, 2);
    assert_eq!(message.usage.cache_read, 1);
    assert_eq!(message.usage.output, 2);
}

#[test]
fn provider_can_be_registered_for_program_use() {
    register_provider(Box::new(OpenAICompletionsProvider::new()));

    assert!(get_provider(&Api::OpenAICompletions).is_some());
}

#[test]
fn detects_non_standard_max_tokens_field() {
    let model = test_model("https://api.together.ai/v1");

    let compat = resolve_compat(&model);

    assert_eq!(compat.max_tokens_field, MaxTokensField::MaxTokens);
    assert_eq!(compat.thinking_format, ThinkingFormat::Together);
}

#[tokio::test]
async fn missing_key_becomes_stream_error() {
    let mut model = test_model("http://127.0.0.1/v1");
    model.provider = Provider::Custom("custom-openai".to_string());
    let provider = OpenAICompletionsProvider::new();
    let mut options = test_options();
    options.base.api_key = None;
    let mut stream = provider.stream_simple(
        &model,
        &Context {
            system_prompt: None,
            messages: Vec::new(),
            tools: Vec::new(),
        },
        &options,
    );

    let mut error = None;
    while let Some(event) = stream.next().await {
        if let StreamEvent::Error { error: message, .. } = event {
            error = message.error_message;
            break;
        }
    }

    assert!(
        error
            .unwrap()
            .contains("No API key for provider: custom-openai")
    );
}

#[test]
fn marks_transient_statuses_as_retryable() {
    assert!(should_retry_status(reqwest::StatusCode::TOO_MANY_REQUESTS));
    assert!(should_retry_status(
        reqwest::StatusCode::INTERNAL_SERVER_ERROR
    ));
    assert!(!should_retry_status(reqwest::StatusCode::BAD_REQUEST));
}

#[test]
fn sends_copilot_dynamic_headers() {
    let mut model = test_model("https://api.githubcopilot.com/v1");
    model.provider = Provider::Custom("github-copilot".to_string());
    let context = Context {
        system_prompt: None,
        messages: vec![
            Message::User(UserMessage {
                content: UserContent::Text("first".to_string()),
                display_text: None,
                timestamp: 1,
            }),
            Message::Assistant(rozsa_model::types::AssistantMessage {
                content: Vec::new(),
                api: Api::OpenAICompletions,
                provider: model.provider.clone(),
                model: model.id.clone(),
                response_model: None,
                response_id: None,
                usage: rozsa_model::providers::common::empty_usage(),
                stop_reason: rozsa_model::types::StopReason::Stop,
                error_message: None,
                timestamp: 2,
            }),
        ],
        tools: Vec::new(),
    };

    let headers = request_headers(&model, &context, &test_options(), "test-key").unwrap();

    assert_eq!(headers["X-Initiator"], "agent");
    assert_eq!(headers["Openai-Intent"], "conversation-edits");
    assert_eq!(headers["Authorization"], "Bearer test-key");
}

#[test]
fn cloudflare_gateway_uses_gateway_auth_header() {
    let mut model = test_model("https://gateway.ai.cloudflare.com/v1/account/gateway/compat");
    model.provider = Provider::CloudflareAIGateway;

    let headers = request_headers(
        &model,
        &Context {
            system_prompt: None,
            messages: vec![Message::User(UserMessage {
                content: UserContent::Text("hello".to_string()),
                display_text: None,
                timestamp: 1,
            })],
            tools: Vec::new(),
        },
        &test_options(),
        "test-key",
    )
    .unwrap();

    assert_eq!(headers["cf-aig-authorization"], "Bearer test-key");
    assert!(!headers.contains_key("Authorization"));
}

#[tokio::test]
#[ignore = "requires real provider credentials and may incur cost"]
async fn live_openai_completions_smoke_when_enabled() {
    if std::env::var("ROZSA_MODEL_LIVE_OPENAI_COMPLETIONS")
        .ok()
        .as_deref()
        != Some("1")
    {
        return;
    }
    let mut model = test_model(
        &std::env::var("ROZSA_MODEL_LIVE_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com/v1".to_string()),
    );
    model.id =
        std::env::var("ROZSA_MODEL_LIVE_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string());
    model.provider = Provider::OpenAI;
    let mut options = test_options();
    options.base.api_key = Some(
        std::env::var("ROZSA_MODEL_LIVE_API_KEY")
            .or_else(|_| std::env::var("OPENAI_API_KEY"))
            .expect("ROZSA_MODEL_LIVE_API_KEY or OPENAI_API_KEY is required"),
    );
    options.base.max_tokens = Some(16);

    let events = rozsa_model::providers::openai_completions::run_openai_chat_stream(
        &model,
        &Context {
            system_prompt: None,
            messages: vec![Message::User(UserMessage {
                content: UserContent::Text("Say ok.".to_string()),
                display_text: None,
                timestamp: 1,
            })],
            tools: Vec::new(),
        },
        &options,
        create_output(&model, Api::OpenAICompletions),
    )
    .await
    .unwrap();

    assert!(matches!(events.last(), Some(StreamEvent::Done { .. })));
}
