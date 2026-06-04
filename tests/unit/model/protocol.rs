use rozsa_model::protocol::{
    BridgeOutput, event_to_bridge_output, parse_input_line, provider_request,
};
use rozsa_model::types::{
    Api, AssistantMessage, Provider, StopReason, StreamEvent, Usage, UsageCost,
};
use serde_json::{Value, json};

fn minimal_usage() -> Usage {
    Usage {
        input: 1,
        output: 2,
        cache_read: 0,
        cache_write: 0,
        total_tokens: 3,
        cost: UsageCost {
            input: 0.0,
            output: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
            total: 0.0,
        },
    }
}

fn assistant_message() -> AssistantMessage {
    AssistantMessage {
        content: Vec::new(),
        api: Api::OpenAICompletions,
        provider: Provider::OpenAI,
        model: "gpt-test".to_string(),
        response_model: Some("gpt-test-response".to_string()),
        response_id: Some("resp_1".to_string()),
        usage: minimal_usage(),
        stop_reason: StopReason::Stop,
        error_message: None,
        timestamp: 123,
    }
}

#[test]
fn parses_ts_bridge_request_into_provider_request() {
    let line = json!({
        "type": "request",
        "id": "req_1",
        "method": "streamSimple",
        "model": {
            "id": "gpt-test",
            "name": "GPT Test",
            "api": "openai-completions",
            "provider": "openai",
            "baseUrl": "https://api.openai.com/v1",
            "reasoning": true,
            "input": ["text", "image"],
            "cost": { "input": 1.0, "output": 2.0, "cacheRead": 0.1, "cacheWrite": 0.2 },
            "contextWindow": 128000,
            "maxTokens": 4096,
            "headers": { "x-test": "yes" },
            "thinkingLevelMap": { "high": "high", "xhigh": null },
            "compat": { "maxTokensField": "max_completion_tokens" }
        },
        "context": {
            "systemPrompt": "You are concise.",
            "messages": [{ "role": "user", "content": "hello", "timestamp": 10 }],
            "tools": [{ "name": "lookup", "description": "Lookup data", "parameters": { "type": "object" } }]
        },
        "options": {
            "temperature": 0.2,
            "maxTokens": 99,
            "apiKey": "test-key",
            "transport": "sse",
            "cacheRetention": "none",
            "sessionId": "session-1",
            "headers": { "x-request": "yes" },
            "timeoutMs": 1000,
            "maxRetries": 2,
            "maxRetryDelayMs": 300,
            "reasoning": "high",
            "toolChoice": "required",
            "thinkingBudgets": { "high": 2048 }
        }
    })
    .to_string();

    let input = parse_input_line(&line).expect("bridge input should parse");
    let request = provider_request(input)
        .expect("provider request should convert")
        .expect("request should not be cancel");

    assert_eq!(request.id, "req_1");
    assert_eq!(request.model.api, Api::OpenAICompletions);
    assert_eq!(request.model.provider, Provider::OpenAI);
    assert_eq!(request.context.messages.len(), 1);
    assert_eq!(request.context.tools.len(), 1);
    assert_eq!(request.options.base.max_tokens, Some(99));
    assert_eq!(request.options.base.api_key.as_deref(), Some("test-key"));
    assert_eq!(request.options.tool_choice, Some(json!("required")));
}

#[test]
fn serializes_rust_done_event_to_ts_shape() {
    let output = event_to_bridge_output(
        "req_1",
        StreamEvent::Done {
            reason: StopReason::Stop,
            message: assistant_message(),
        },
    );

    let BridgeOutput::Event { id, event } = output else {
        panic!("expected event output");
    };
    assert_eq!(id, "req_1");
    assert_eq!(event["type"], Value::String("done".to_string()));
    assert_eq!(event["reason"], Value::String("stop".to_string()));
    assert_eq!(
        event["message"]["api"],
        Value::String("openai-completions".to_string())
    );
    assert_eq!(
        event["message"]["provider"],
        Value::String("openai".to_string())
    );
    assert_eq!(event["message"]["usage"]["totalTokens"], Value::from(3));
}

#[test]
fn parses_and_serializes_nvidia_provider_id() {
    let line = json!({
        "type": "request",
        "id": "req_nvidia",
        "method": "streamSimple",
        "model": {
            "id": "meta/llama-3.1-70b-instruct",
            "name": "Llama 3.1 70B",
            "api": "openai-completions",
            "provider": "nvidia",
            "baseUrl": "https://integrate.api.nvidia.com/v1"
        },
        "context": {
            "messages": [{ "role": "user", "content": "hello" }]
        }
    })
    .to_string();

    let input = parse_input_line(&line).expect("bridge input should parse");
    let request = provider_request(input)
        .expect("provider request should convert")
        .expect("request should not be cancel");

    assert_eq!(request.model.provider, Provider::Nvidia);

    let mut output = assistant_message();
    output.provider = Provider::Nvidia;
    let BridgeOutput::Event { event, .. } = event_to_bridge_output(
        "req_nvidia",
        StreamEvent::Done {
            reason: StopReason::Stop,
            message: output,
        },
    ) else {
        panic!("expected event output");
    };

    assert_eq!(
        event["message"]["provider"],
        Value::String("nvidia".to_string())
    );
}
