use super::protocol::*;
use crate::config::AgentContext;
use crate::events::AgentEvent;
use crate::messages::AgentMessage;
use rozsa_model::types::{
    Api, AssistantMessage, CacheRetention, ContentBlock, Message, Model, ModelCost, Provider,
    SimpleStreamOptions, StopReason, StreamOptions, ThinkingLevel, Transport, Usage, UsageCost,
    UserContent, UserMessage,
};

fn make_test_model() -> Model {
    Model {
        id: "test-model".to_string(),
        name: "Test Model".to_string(),
        api: Api::AnthropicMessages,
        provider: Provider::Anthropic,
        base_url: "https://api.test.com".to_string(),
        reasoning: false,
        input_modalities: vec![],
        cost: ModelCost {
            input: 0.0,
            output: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
        },
        context_window: 8000,
        max_tokens: 4000,
        thinking_level_map: None,
        headers: None,
        compat: None,
    }
}

fn make_test_assistant_message() -> AssistantMessage {
    AssistantMessage {
        content: vec![],
        api: Api::AnthropicMessages,
        provider: Provider::Anthropic,
        model: "test-model".to_string(),
        response_model: None,
        response_id: None,
        usage: Usage {
            input: 0, output: 0, cache_read: 0, cache_write: 0, total_tokens: 0,
            cost: UsageCost { input: 0.0, output: 0.0, cache_read: 0.0, cache_write: 0.0, total: 0.0 },
        },
        stop_reason: StopReason::Stop,
        error_message: None,
        timestamp: 0,
    }
}

fn make_test_context() -> AgentContext {
    AgentContext {
        system_prompt: Some("test".to_string()),
        messages: vec![],
        tools: vec![],
    }
}

fn make_test_stream_options() -> SimpleStreamOptions {
    SimpleStreamOptions {
        base: StreamOptions {
            max_tokens: Some(1000),
            temperature: None,
            api_key: None,
            transport: Transport::Auto,
            cache_retention: CacheRetention::None,
            session_id: None,
            headers: None,
            timeout_ms: None,
            max_retries: None,
            max_retry_delay_ms: None,
            metadata: None,
        },
        reasoning: Some(ThinkingLevel::Medium),
        thinking_budgets: None,
        tool_choice: None,
    }
}

#[test]
fn bridge_input_start_run_roundtrip() {
    let input = BridgeInput::StartRun {
        version: PROTOCOL_VERSION,
        run_id: "test-run-1".to_string(),
        mode: RunMode::Prompt {
            prompts: vec![AgentMessage::standard(Message::User(UserMessage {
                content: UserContent::Text("test prompt".to_string()),
                display_text: None,
                timestamp: 0,
            }))],
            context: AgentContext {
                system_prompt: None,
                messages: vec![],
                tools: vec![],
            },
        },
        config: BridgeConfig {
            model: make_test_model(),
            reasoning: Some(ThinkingLevel::Medium),
            stream_options: make_test_stream_options(),
            tool_execution: crate::tool::ToolExecutionMode::Sequential,
        },
    };

    let json = serde_json::to_string(&input).expect("serialize BridgeInput::StartRun");
    let decoded: BridgeInput = serde_json::from_str(&json).expect("deserialize BridgeInput::StartRun");

    match (input, decoded) {
        (
            BridgeInput::StartRun {
                run_id: rid1,
                version: v1,
                ..
            },
            BridgeInput::StartRun {
                run_id: rid2,
                version: v2,
                ..
            },
        ) => {
            assert_eq!(rid1, rid2);
            assert_eq!(v1, v2);
        }
        _ => panic!("Decoded variant mismatch"),
    }
}

#[test]
fn bridge_input_cancel_roundtrip() {
    let input = BridgeInput::Cancel {
        version: PROTOCOL_VERSION,
        run_id: "test-run-cancel".to_string(),
    };

    let json = serde_json::to_string(&input).expect("serialize BridgeInput::Cancel");
    let decoded: BridgeInput = serde_json::from_str(&json).expect("deserialize BridgeInput::Cancel");

    match (input, decoded) {
        (
            BridgeInput::Cancel {
                run_id: rid1,
                version: v1,
            },
            BridgeInput::Cancel {
                run_id: rid2,
                version: v2,
            },
        ) => {
            assert_eq!(rid1, rid2);
            assert_eq!(v1, v2);
        }
        _ => panic!("Decoded variant mismatch"),
    }
}

#[test]
fn bridge_input_tool_result_roundtrip() {
    let input = BridgeInput::ToolResult {
        version: PROTOCOL_VERSION,
        run_id: "test-run-tool".to_string(),
        request_id: "req-123".to_string(),
        result: ToolHostResult {
            content: vec![ContentBlock::Text {
                text: "tool result".to_string(),
                signature: None,
            }],
            is_error: false,
            terminate: false,
        },
    };

    let json = serde_json::to_string(&input).expect("serialize BridgeInput::ToolResult");
    let decoded: BridgeInput = serde_json::from_str(&json).expect("deserialize BridgeInput::ToolResult");

    match (input, decoded) {
        (
            BridgeInput::ToolResult {
                run_id: rid1,
                request_id: reqid1,
                result: res1,
                ..
            },
            BridgeInput::ToolResult {
                run_id: rid2,
                request_id: reqid2,
                result: res2,
                ..
            },
        ) => {
            assert_eq!(rid1, rid2);
            assert_eq!(reqid1, reqid2);
            assert_eq!(res1.is_error, res2.is_error);
            assert_eq!(res1.terminate, res2.terminate);
        }
        _ => panic!("Decoded variant mismatch"),
    }
}

#[test]
fn bridge_output_agent_event_roundtrip() {
    let output = BridgeOutput::agent_event(
        "test-run",
        AgentEvent::MessageEnd {
            message: AgentMessage::standard(Message::Assistant(AssistantMessage {
                content: vec![],
                api: Api::AnthropicMessages,
                provider: Provider::Anthropic,
                model: "test".to_string(),
                response_model: None,
                response_id: None,
                usage: Usage {
                    input: 0,
                    output: 0,
                    cache_read: 0,
                    cache_write: 0,
                    total_tokens: 0,
                    cost: UsageCost {
                        input: 0.0,
                        output: 0.0,
                        cache_read: 0.0,
                        cache_write: 0.0,
                        total: 0.0,
                    },
                },
                stop_reason: StopReason::Stop,
                error_message: None,
                timestamp: 0,
            })),
        },
    );

    let json = serde_json::to_string(&output).expect("serialize BridgeOutput::AgentEvent");
    let decoded: BridgeOutput = serde_json::from_str(&json).expect("deserialize BridgeOutput::AgentEvent");

    match (output, decoded) {
        (
            BridgeOutput::AgentEvent {
                run_id: rid1,
                version: v1,
                ..
            },
            BridgeOutput::AgentEvent {
                run_id: rid2,
                version: v2,
                ..
            },
        ) => {
            assert_eq!(rid1, rid2);
            assert_eq!(v1, v2);
        }
        _ => panic!("Decoded variant mismatch"),
    }
}

#[test]
fn bridge_output_tool_request_roundtrip() {
    let output = BridgeOutput::tool_request(
        "test-run",
        "req-456",
        "tool-call-789",
        "test_tool",
        serde_json::json!({"param": "value"}),
        make_test_assistant_message(),
        make_test_context(),
    );

    let json = serde_json::to_string(&output).expect("serialize BridgeOutput::ToolRequest");
    let decoded: BridgeOutput = serde_json::from_str(&json).expect("deserialize BridgeOutput::ToolRequest");

    match (output, decoded) {
        (
            BridgeOutput::ToolRequest {
                run_id: rid1,
                request_id: reqid1,
                tool_call_id: tcid1,
                tool_name: tn1,
                ..
            },
            BridgeOutput::ToolRequest {
                run_id: rid2,
                request_id: reqid2,
                tool_call_id: tcid2,
                tool_name: tn2,
                ..
            },
        ) => {
            assert_eq!(rid1, rid2);
            assert_eq!(reqid1, reqid2);
            assert_eq!(tcid1, tcid2);
            assert_eq!(tn1, tn2);
        }
        _ => panic!("Decoded variant mismatch"),
    }
}

#[test]
fn bridge_output_run_done_roundtrip() {
    let output = BridgeOutput::run_done("test-run-done");

    let json = serde_json::to_string(&output).expect("serialize BridgeOutput::RunDone");
    let decoded: BridgeOutput = serde_json::from_str(&json).expect("deserialize BridgeOutput::RunDone");

    match (output, decoded) {
        (
            BridgeOutput::RunDone {
                run_id: rid1,
                version: v1,
            },
            BridgeOutput::RunDone {
                run_id: rid2,
                version: v2,
            },
        ) => {
            assert_eq!(rid1, rid2);
            assert_eq!(v1, v2);
        }
        _ => panic!("Decoded variant mismatch"),
    }
}

#[test]
fn bridge_output_run_error_roundtrip() {
    let output = BridgeOutput::run_error("test-run-err", "test error message");

    let json = serde_json::to_string(&output).expect("serialize BridgeOutput::RunError");
    let decoded: BridgeOutput = serde_json::from_str(&json).expect("deserialize BridgeOutput::RunError");

    match (output, decoded) {
        (
            BridgeOutput::RunError {
                run_id: rid1,
                error: err1,
                ..
            },
            BridgeOutput::RunError {
                run_id: rid2,
                error: err2,
                ..
            },
        ) => {
            assert_eq!(rid1, rid2);
            assert_eq!(err1, err2);
        }
        _ => panic!("Decoded variant mismatch"),
    }
}
