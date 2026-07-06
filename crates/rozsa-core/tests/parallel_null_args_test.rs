// 回归测试：并行执行路径下 tool call arguments 为 null 时应规范化为 {}，不报错。
// 修复前 execute_parallel 直接传 Value::Null 导致 serde 反序列化失败。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rozsa_core::agent_loop::agent_loop;
use rozsa_core::config::{AgentContext, AgentLoopConfig};
use rozsa_core::messages::AgentMessage;
use rozsa_core::tool::{Tool, ToolError, ToolExecutionMode, ToolResult};
use rozsa_model::event_stream::create_event_stream;
use rozsa_model::types::{
    Api, CacheRetention, ContentBlock, InputModality, Message, Model, ModelCost, Provider,
    SimpleStreamOptions, StopReason, StreamEvent, StreamOptions, ToolCall, Transport, Usage,
    UsageCost, UserContent, UserMessage,
};
use tokio_util::sync::CancellationToken;

struct AssertNonNullTool {
    schema: serde_json::Value,
    received_non_null: Arc<AtomicBool>,
}

#[async_trait::async_trait]
impl Tool for AssertNonNullTool {
    fn name(&self) -> &str {
        "null_check"
    }
    fn description(&self) -> &str {
        "asserts params not null"
    }
    fn label(&self) -> &str {
        "null_check"
    }
    fn parameters_schema(&self) -> &serde_json::Value {
        &self.schema
    }
    async fn execute(
        &self,
        _tool_call_id: &str,
        params: serde_json::Value,
        _signal: Option<CancellationToken>,
        _on_update: Option<&(dyn Fn(ToolResult) + Send + Sync)>,
    ) -> Result<ToolResult, ToolError> {
        assert!(
            !params.is_null(),
            "params should be normalized to {{}}, got null"
        );
        self.received_non_null.store(true, Ordering::SeqCst);
        Ok(ToolResult {
            content: vec![ContentBlock::Text {
                text: "ok".to_string(),
                signature: None,
            }],
            details: serde_json::Value::Null,
            terminate: false,
        })
    }
}

fn make_model() -> Model {
    Model {
        id: "mock".to_string(),
        name: "mock".to_string(),
        api: Api::OpenAIResponses,
        provider: Provider::OpenAI,
        base_url: "https://example.invalid".to_string(),
        reasoning: false,
        input_modalities: vec![InputModality::Text],
        cost: ModelCost {
            input: 0.0,
            output: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
        },
        context_window: 8192,
        max_tokens: 2048,
        thinking_level_map: None,
        headers: None,
        compat: None,
    }
}

fn make_assistant_message(content: Vec<ContentBlock>) -> rozsa_model::types::AssistantMessage {
    rozsa_model::types::AssistantMessage {
        content,
        api: Api::OpenAIResponses,
        provider: Provider::OpenAI,
        model: "mock".to_string(),
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
        timestamp: 1,
    }
}

#[tokio::test]
async fn parallel_null_arguments_normalized_to_empty_object() {
    let received = Arc::new(AtomicBool::new(false));
    let tool: Arc<dyn Tool> = Arc::new(AssertNonNullTool {
        schema: serde_json::json!({ "type": "object" }),
        received_non_null: received.clone(),
    });

    let config = {
        let model = make_model();
        let stream_options = SimpleStreamOptions {
            base: StreamOptions {
                temperature: None,
                max_tokens: None,
                api_key: None,
                transport: Transport::Sse,
                cache_retention: CacheRetention::None,
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
        };

        AgentLoopConfig {
            model,
            reasoning: None,
            stream_options,
            model_stream: Box::new(move |_model, _context, _options| {
                let (sender, stream) = create_event_stream();
                let message = make_assistant_message(vec![ContentBlock::ToolCall(ToolCall {
                    id: "call_null".to_string(),
                    name: "null_check".to_string(),
                    arguments: serde_json::Value::Null,
                })]);
                tokio::spawn(async move {
                    sender.push(StreamEvent::Start {
                        partial: make_assistant_message(Vec::new()),
                    });
                    sender.push(StreamEvent::Done {
                        reason: StopReason::Stop,
                        message,
                    });
                });
                stream
            }),
            convert_to_llm: Box::new(|messages| {
                messages
                    .iter()
                    .filter_map(AgentMessage::as_standard)
                    .cloned()
                    .collect()
            }),
            transform_context: None,
            get_api_key: None,
            should_stop_after_turn: Some(Box::new(|_| true)),
            prepare_next_turn: None,
            get_steering_messages: None,
            get_follow_up_messages: None,
            max_turns: None,
            tool_execution: ToolExecutionMode::Parallel,
            pre_tool_use: None,
            post_tool_use: None,
            tools: vec![tool],
        }
    };

    let context = AgentContext {
        system_prompt: Some("test".to_string()),
        messages: Vec::new(),
        tools: Vec::new(),
    };

    let prompt = AgentMessage::standard(Message::User(UserMessage {
        content: UserContent::Text("test".to_string()),
        display_text: None,
        timestamp: 1,
    }));

    let mut stream = agent_loop(vec![prompt], context, config, None);
    while let Some(_event) = stream.next().await {}

    assert!(
        received.load(Ordering::SeqCst),
        "tool should have been called with non-null params"
    );
}
