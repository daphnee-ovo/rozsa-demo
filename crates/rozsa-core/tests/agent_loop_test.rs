use rozsa_core::agent_loop::*;
use rozsa_core::config::{AgentContext, AgentLoopConfig, TurnUpdate};
use rozsa_core::events::AgentEvent;
use rozsa_core::messages::AgentMessage;
use rozsa_core::tool::{Tool, ToolError, ToolExecutionMode, ToolResult};
use rozsa_model::event_stream::{EventStream, create_event_stream};
use rozsa_model::types::{
    Api, CacheRetention, ContentBlock, InputModality, Message, ModelCost, Provider,
    SimpleStreamOptions, StopReason, StreamEvent, StreamOptions, ToolCall, Transport, Usage,
    UsageCost, UserContent, UserMessage,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

fn config_with_stream(
    make_stream: impl Fn() -> EventStream<StreamEvent> + Send + Sync + 'static,
) -> AgentLoopConfig {
    AgentLoopConfig {
        model: model(),
        reasoning: None,
        stream_options: stream_options(),
        model_stream: Box::new(move |_model, _context, _options| make_stream()),
        convert_to_llm: Box::new(|messages| {
            messages
                .iter()
                .filter_map(AgentMessage::as_standard)
                .cloned()
                .collect()
        }),
        transform_context: None,
        get_api_key: None,
        should_stop_after_turn: None,
        prepare_next_turn: None,
        get_steering_messages: None,
        get_follow_up_messages: None,
        max_turns: None,
        tool_execution: ToolExecutionMode::Sequential,
        pre_tool_use: None,
        post_tool_use: None,
        tools: vec![],
    }
}

fn empty_context() -> AgentContext {
    AgentContext {
        system_prompt: Some("system".to_string()),
        messages: Vec::new(),
        tools: Vec::new(),
    }
}

fn assistant_message(content: Vec<ContentBlock>) -> rozsa_model::types::AssistantMessage {
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

fn model() -> rozsa_model::types::Model {
    rozsa_model::types::Model {
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

fn stream_options() -> SimpleStreamOptions {
    SimpleStreamOptions {
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
    }
}

fn event_name(event: &AgentEvent) -> &'static str {
    match event {
        AgentEvent::AgentStart => "agent_start",
        AgentEvent::AgentEnd { .. } => "agent_end",
        AgentEvent::TurnStart => "turn_start",
        AgentEvent::TurnEnd { .. } => "turn_end",
        AgentEvent::MessageStart { .. } => "message_start",
        AgentEvent::MessageUpdate { .. } => "message_update",
        AgentEvent::MessageEnd { .. } => "message_end",
        AgentEvent::ToolExecutionStart { .. } => "tool_execution_start",
        AgentEvent::ToolExecutionUpdate { .. } => "tool_execution_update",
        AgentEvent::ToolExecutionEnd { .. } => "tool_execution_end",
    }
}

struct FakeTool {
    name: String,
    response: String,
    schema: serde_json::Value,
}

#[async_trait::async_trait]
impl Tool for FakeTool {
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> &str {
        "fake tool"
    }
    fn label(&self) -> &str {
        "fake"
    }
    fn parameters_schema(&self) -> &serde_json::Value {
        &self.schema
    }
    async fn execute(
        &self,
        _tool_call_id: &str,
        _params: serde_json::Value,
        _signal: Option<CancellationToken>,
        _on_update: Option<&(dyn Fn(ToolResult) + Send + Sync)>,
    ) -> Result<ToolResult, ToolError> {
        Ok(ToolResult {
            content: vec![ContentBlock::Text {
                text: self.response.clone(),
                signature: None,
            }],
            details: serde_json::Value::Null,
            terminate: false,
        })
    }
}

fn assistant_agent_message(message: rozsa_model::types::AssistantMessage) -> AgentMessage {
    AgentMessage::standard(Message::Assistant(message))
}

#[tokio::test]
async fn no_tool_prompt_loop_emits_core_event_order() {
    let final_message = assistant_message(vec![ContentBlock::Text {
        text: "hi".to_string(),
        signature: None,
    }]);
    let final_for_stream = final_message.clone();
    let config = config_with_stream(move || {
        let (sender, stream) = create_event_stream();
        let message = final_for_stream.clone();
        tokio::spawn(async move {
            sender.push(StreamEvent::Start {
                partial: assistant_message(Vec::new()),
            });
            sender.push(StreamEvent::TextDelta {
                content_index: 0,
                delta: "hi".to_string(),
                partial: message.clone(),
            });
            sender.push(StreamEvent::Done {
                reason: StopReason::Stop,
                message,
            });
        });
        stream
    });
    let prompt = AgentMessage::standard(Message::User(UserMessage {
        content: UserContent::Text("hello".to_string()),
        display_text: None,
        timestamp: 1,
    }));
    let mut stream = agent_loop(vec![prompt], empty_context(), config, None);
    let mut events = Vec::new();

    while let Some(event) = stream.next().await {
        events.push(event);
    }

    assert_eq!(
        events.iter().map(event_name).collect::<Vec<_>>(),
        vec![
            "agent_start",
            "turn_start",
            "message_start",
            "message_end",
            "message_start",
            "message_update",
            "message_end",
            "turn_end",
            "agent_end",
        ]
    );
    let AgentEvent::TurnEnd {
        message,
        tool_results,
    } = &events[7]
    else {
        panic!("expected turn_end");
    };
    assert_eq!(message.content.len(), 1);
    assert!(tool_results.is_empty());
}

#[test]
fn core_events_are_json_serializable() {
    let event = AgentEvent::MessageEnd {
        message: AgentMessage::standard(Message::Assistant(assistant_message(Vec::new()))),
    };
    let encoded = serde_json::to_value(event).expect("serialize event");
    assert_eq!(encoded["type"], "message_end");
    assert_eq!(encoded["message"]["kind"], "standard");
}

#[tokio::test]
async fn steering_messages_injected_before_assistant_response() {
    use std::sync::Mutex;
    let steering_count = Arc::new(Mutex::new(0));
    let steering_count_clone = steering_count.clone();

    let config = {
        let mut base = config_with_stream(|| {
            let (sender, stream) = create_event_stream();
            let message = assistant_message(vec![ContentBlock::Text {
                text: "response".to_string(),
                signature: None,
            }]);
            tokio::spawn(async move {
                sender.push(StreamEvent::Start {
                    partial: assistant_message(Vec::new()),
                });
                sender.push(StreamEvent::Done {
                    reason: StopReason::Stop,
                    message,
                });
            });
            stream
        });
        base.get_steering_messages = Some(Box::new(move || {
            let mut count = steering_count_clone.lock().unwrap();
            *count += 1;
            if *count == 1 {
                vec![AgentMessage::standard(Message::User(UserMessage {
                    content: UserContent::Text("steering message".to_string()),
                    display_text: None,
                    timestamp: 2,
                }))]
            } else {
                vec![]
            }
        }));
        base
    };

    let prompt = AgentMessage::standard(Message::User(UserMessage {
        content: UserContent::Text("hello".to_string()),
        display_text: None,
        timestamp: 1,
    }));

    let mut stream = agent_loop(vec![prompt], empty_context(), config, None);
    let mut events = Vec::new();

    while let Some(event) = stream.next().await {
        events.push(event);
    }

    assert!(*steering_count.lock().unwrap() >= 1);

    let mut found_steering = false;
    let mut found_assistant = false;
    for event in &events {
        if let AgentEvent::MessageEnd { message } = event {
            if let Some(Message::User(u)) = message.as_standard() {
                if matches!(u.content, UserContent::Text(ref s) if s == "steering message") {
                    found_steering = true;
                }
            }
            if let Some(Message::Assistant(_)) = message.as_standard() {
                assert!(
                    found_steering,
                    "steering message should appear before assistant"
                );
                found_assistant = true;
            }
        }
    }
    assert!(found_steering && found_assistant);
}

#[tokio::test]
async fn follow_up_messages_trigger_new_turn() {
    use std::sync::Mutex;
    let follow_up_count = Arc::new(Mutex::new(0));
    let follow_up_count_clone = follow_up_count.clone();
    let turn_count = Arc::new(Mutex::new(0));
    let turn_count_clone = turn_count.clone();

    let config = {
        let mut base = config_with_stream(move || {
            let (sender, stream) = create_event_stream();
            let turn_num = {
                let mut count = turn_count_clone.lock().unwrap();
                *count += 1;
                *count
            };
            let text = format!("response {}", turn_num);
            let message = assistant_message(vec![ContentBlock::Text {
                text,
                signature: None,
            }]);
            tokio::spawn(async move {
                sender.push(StreamEvent::Start {
                    partial: assistant_message(Vec::new()),
                });
                sender.push(StreamEvent::Done {
                    reason: StopReason::Stop,
                    message,
                });
            });
            stream
        });
        base.get_follow_up_messages = Some(Box::new(move || {
            let mut count = follow_up_count_clone.lock().unwrap();
            *count += 1;
            if *count == 1 {
                vec![AgentMessage::standard(Message::User(UserMessage {
                    content: UserContent::Text("follow-up".to_string()),
                    display_text: None,
                    timestamp: 3,
                }))]
            } else {
                vec![]
            }
        }));
        base
    };

    let prompt = AgentMessage::standard(Message::User(UserMessage {
        content: UserContent::Text("hello".to_string()),
        display_text: None,
        timestamp: 1,
    }));

    let mut stream = agent_loop(vec![prompt], empty_context(), config, None);
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }

    let event_names: Vec<_> = events.iter().map(event_name).collect();
    let turn_starts = event_names.iter().filter(|&&n| n == "turn_start").count();
    assert_eq!(turn_starts, 2, "should have 2 turns due to follow-up");
    assert_eq!(*turn_count.lock().unwrap(), 2);
}

#[tokio::test]
async fn follow_up_empty_agent_ends_normally() {
    use std::sync::Mutex;
    let follow_up_count = Arc::new(Mutex::new(0));
    let follow_up_count_clone = follow_up_count.clone();

    let config = {
        let mut base = config_with_stream(|| {
            let (sender, stream) = create_event_stream();
            let message = assistant_message(vec![ContentBlock::Text {
                text: "response".to_string(),
                signature: None,
            }]);
            tokio::spawn(async move {
                sender.push(StreamEvent::Start {
                    partial: assistant_message(Vec::new()),
                });
                sender.push(StreamEvent::Done {
                    reason: StopReason::Stop,
                    message,
                });
            });
            stream
        });
        base.get_follow_up_messages = Some(Box::new(move || {
            let mut count = follow_up_count_clone.lock().unwrap();
            *count += 1;
            vec![]
        }));
        base
    };

    let prompt = AgentMessage::standard(Message::User(UserMessage {
        content: UserContent::Text("hello".to_string()),
        display_text: None,
        timestamp: 1,
    }));

    let mut stream = agent_loop(vec![prompt], empty_context(), config, None);
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }

    let event_names: Vec<_> = events.iter().map(event_name).collect();
    assert!(event_names.contains(&"agent_end"));
    assert_eq!(*follow_up_count.lock().unwrap(), 1);
}

#[tokio::test]
async fn should_stop_after_turn_ends_agent() {
    let config = {
        let mut base = config_with_stream(|| {
            let (sender, stream) = create_event_stream();
            let message = assistant_message(vec![ContentBlock::Text {
                text: "response".to_string(),
                signature: None,
            }]);
            tokio::spawn(async move {
                sender.push(StreamEvent::Start {
                    partial: assistant_message(Vec::new()),
                });
                sender.push(StreamEvent::Done {
                    reason: StopReason::Stop,
                    message,
                });
            });
            stream
        });
        base.should_stop_after_turn = Some(Box::new(|_ctx| true));
        base
    };

    let prompt = AgentMessage::standard(Message::User(UserMessage {
        content: UserContent::Text("hello".to_string()),
        display_text: None,
        timestamp: 1,
    }));

    let mut stream = agent_loop(vec![prompt], empty_context(), config, None);
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }

    let event_names: Vec<_> = events.iter().map(event_name).collect();
    assert!(event_names.contains(&"agent_end"));
    let turn_ends = event_names.iter().filter(|&&n| n == "turn_end").count();
    assert_eq!(turn_ends, 1, "should have exactly one turn before stopping");
}

#[tokio::test]
async fn prepare_next_turn_updates_model() {
    use std::sync::Mutex;
    let prepare_count = Arc::new(Mutex::new(0));
    let prepare_count_clone = prepare_count.clone();
    let prepare_count_clone2 = prepare_count.clone();
    let initial_model_id = Arc::new(Mutex::new(String::from("mock")));
    let model_id_for_check = initial_model_id.clone();

    let config = {
        let mut base = config_with_stream(move || {
            let (sender, stream) = create_event_stream();
            let message = assistant_message(vec![ContentBlock::Text {
                text: "response".to_string(),
                signature: None,
            }]);
            tokio::spawn(async move {
                sender.push(StreamEvent::Start {
                    partial: assistant_message(Vec::new()),
                });
                sender.push(StreamEvent::Done {
                    reason: StopReason::Stop,
                    message,
                });
            });
            stream
        });
        base.prepare_next_turn = Some(Box::new(move |_ctx| {
            let mut count = prepare_count_clone.lock().unwrap();
            *count += 1;
            if *count == 1 {
                let mut new_model = model();
                new_model.id = "updated-model".to_string();
                *model_id_for_check.lock().unwrap() = new_model.id.clone();
                Some(TurnUpdate {
                    context: None,
                    model: Some(new_model),
                    thinking_level: None,
                })
            } else {
                None
            }
        }));
        base.get_follow_up_messages = Some(Box::new(move || {
            let count = prepare_count_clone2.lock().unwrap();
            if *count == 1 {
                vec![AgentMessage::standard(Message::User(UserMessage {
                    content: UserContent::Text("continue".to_string()),
                    display_text: None,
                    timestamp: 3,
                }))]
            } else {
                vec![]
            }
        }));
        base
    };

    let prompt = AgentMessage::standard(Message::User(UserMessage {
        content: UserContent::Text("hello".to_string()),
        display_text: None,
        timestamp: 1,
    }));

    let mut stream = agent_loop(vec![prompt], empty_context(), config, None);
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }

    assert_eq!(
        *prepare_count.lock().unwrap(),
        2,
        "prepare_next_turn should be called twice"
    );
    assert_eq!(*initial_model_id.lock().unwrap(), "updated-model");
}

#[tokio::test]
async fn multi_turn_with_tools() {
    let tool = Arc::new(FakeTool {
        name: "test_tool".to_string(),
        response: "tool result".to_string(),
        schema: serde_json::json!({ "type": "object" }),
    }) as Arc<dyn Tool>;

    use std::sync::Mutex;
    let call_count = Arc::new(Mutex::new(0));
    let call_count_clone = call_count.clone();

    let config = {
        let mut base = config_with_stream(move || {
            let (sender, stream) = create_event_stream();
            let mut count = call_count_clone.lock().unwrap();
            *count += 1;
            let turn = *count;

            let message = if turn == 1 {
                assistant_message(vec![ContentBlock::ToolCall(ToolCall {
                    id: "call1".to_string(),
                    name: "test_tool".to_string(),
                    arguments: serde_json::json!({}),
                })])
            } else {
                assistant_message(vec![ContentBlock::Text {
                    text: "final response".to_string(),
                    signature: None,
                }])
            };

            tokio::spawn(async move {
                sender.push(StreamEvent::Start {
                    partial: assistant_message(Vec::new()),
                });
                sender.push(StreamEvent::Done {
                    reason: StopReason::Stop,
                    message,
                });
            });
            stream
        });
        base.tools = vec![tool];
        base
    };

    let prompt = AgentMessage::standard(Message::User(UserMessage {
        content: UserContent::Text("hello".to_string()),
        display_text: None,
        timestamp: 1,
    }));

    let mut stream = agent_loop(vec![prompt], empty_context(), config, None);
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }

    let event_names: Vec<_> = events.iter().map(event_name).collect();
    assert!(event_names.contains(&"tool_execution_start"));
    assert!(event_names.contains(&"tool_execution_end"));

    let turn_starts = event_names.iter().filter(|&&n| n == "turn_start").count();
    assert_eq!(
        turn_starts, 2,
        "should have 2 turns: tool call then final response"
    );

    let mut found_tool_result = false;
    let mut found_final_response = false;
    for event in &events {
        if let AgentEvent::MessageEnd { message } = event {
            if let Some(Message::ToolResult(_)) = message.as_standard() {
                found_tool_result = true;
            }
            if let Some(Message::Assistant(a)) = message.as_standard() {
                for block in &a.content {
                    if let ContentBlock::Text { text, .. } = block {
                        if text == "final response" {
                            found_final_response = true;
                        }
                    }
                }
            }
        }
    }
    assert!(found_tool_result, "tool result should appear in messages");
    assert!(
        found_final_response,
        "second-turn assistant text should be visible via MessageEnd"
    );

    // Verify AgentEnd.messages contains the final response
    let agent_end = events
        .iter()
        .find_map(|e| {
            if let AgentEvent::AgentEnd { messages } = e {
                Some(messages)
            } else {
                None
            }
        })
        .expect("should have AgentEnd");
    let has_final_in_end = agent_end.iter().any(|m| {
        if let Some(Message::Assistant(a)) = m.as_standard() {
            a.content
                .iter()
                .any(|b| matches!(b, ContentBlock::Text { text, .. } if text == "final response"))
        } else {
            false
        }
    });
    assert!(
        has_final_in_end,
        "AgentEnd.messages must contain the final assistant response"
    );
}

#[tokio::test]
async fn continue_empty_context_emits_agent_end_immediately() {
    let context = empty_context();
    let config = config_with_stream(|| {
        panic!("should not stream");
    });
    let mut stream = agent_loop_continue(context, config, None);
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }
    assert_eq!(
        events.iter().map(event_name).collect::<Vec<_>>(),
        vec!["agent_start", "agent_end"]
    );
}

#[tokio::test]
async fn continue_last_message_assistant_emits_agent_end_immediately() {
    let mut context = empty_context();
    context
        .messages
        .push(assistant_agent_message(assistant_message(vec![
            ContentBlock::Text {
                text: "hi".to_string(),
                signature: None,
            },
        ])));
    let config = config_with_stream(|| {
        panic!("should not stream");
    });
    let mut stream = agent_loop_continue(context, config, None);
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }
    assert_eq!(
        events.iter().map(event_name).collect::<Vec<_>>(),
        vec!["agent_start", "agent_end"]
    );
}

#[tokio::test]
async fn continue_valid_last_user_message_streams_response() {
    let mut context = empty_context();
    context
        .messages
        .push(AgentMessage::standard(Message::User(UserMessage {
            content: UserContent::Text("hello".to_string()),
            display_text: None,
            timestamp: 1,
        })));
    let config = config_with_stream(|| {
        let (sender, stream) = create_event_stream();
        let message = assistant_message(vec![ContentBlock::Text {
            text: "response".to_string(),
            signature: None,
        }]);
        tokio::spawn(async move {
            sender.push(StreamEvent::Start {
                partial: assistant_message(Vec::new()),
            });
            sender.push(StreamEvent::Done {
                reason: StopReason::Stop,
                message,
            });
        });
        stream
    });
    let mut stream = agent_loop_continue(context, config, None);
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }
    let names: Vec<_> = events.iter().map(event_name).collect();
    assert!(names.contains(&"message_end"));
    assert!(names.contains(&"agent_end"));
}

#[tokio::test]
async fn unknown_tool_returns_error_result() {
    let config = {
        let mut base = config_with_stream(|| {
            let (sender, stream) = create_event_stream();
            let message = assistant_message(vec![ContentBlock::ToolCall(ToolCall {
                id: "call1".to_string(),
                name: "nonexistent".to_string(),
                arguments: serde_json::json!({}),
            })]);
            tokio::spawn(async move {
                sender.push(StreamEvent::Start {
                    partial: assistant_message(Vec::new()),
                });
                sender.push(StreamEvent::Done {
                    reason: StopReason::Stop,
                    message,
                });
            });
            stream
        });
        base.should_stop_after_turn = Some(Box::new(|_| true));
        base
    };

    let prompt = AgentMessage::standard(Message::User(UserMessage {
        content: UserContent::Text("hello".to_string()),
        display_text: None,
        timestamp: 1,
    }));
    let mut stream = agent_loop(vec![prompt], empty_context(), config, None);
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }

    let mut found_error_result = false;
    for event in &events {
        if let AgentEvent::ToolExecutionEnd { result, .. } = event {
            if result.is_error {
                found_error_result = true;
            }
        }
    }
    assert!(
        found_error_result,
        "unknown tool should produce error result"
    );
}

#[tokio::test]
async fn pre_tool_use_blocks_tool() {
    let tool = Arc::new(FakeTool {
        name: "test_tool".to_string(),
        response: "should not see this".to_string(),
        schema: serde_json::json!({ "type": "object" }),
    }) as Arc<dyn Tool>;

    let config = {
        let mut base = config_with_stream(|| {
            let (sender, stream) = create_event_stream();
            let message = assistant_message(vec![ContentBlock::ToolCall(ToolCall {
                id: "call1".to_string(),
                name: "test_tool".to_string(),
                arguments: serde_json::json!({}),
            })]);
            tokio::spawn(async move {
                sender.push(StreamEvent::Start {
                    partial: assistant_message(Vec::new()),
                });
                sender.push(StreamEvent::Done {
                    reason: StopReason::Stop,
                    message,
                });
            });
            stream
        });
        base.tools = vec![tool];
        base.pre_tool_use = Some(Box::new(|_ctx| {
            Box::pin(async {
                Some(rozsa_core::config::PreToolUseResult {
                    block: true,
                    reason: Some("Permission denied".to_string()),
                })
            })
        }));
        base.should_stop_after_turn = Some(Box::new(|_| true));
        base
    };

    let prompt = AgentMessage::standard(Message::User(UserMessage {
        content: UserContent::Text("hello".to_string()),
        display_text: None,
        timestamp: 1,
    }));
    let mut stream = agent_loop(vec![prompt], empty_context(), config, None);
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }

    let mut found_blocked = false;
    for event in &events {
        if let AgentEvent::ToolExecutionEnd { result, .. } = event {
            if result.is_error {
                let has_deny_text = result
                    .content
                    .iter()
                    .any(|b| matches!(b, ContentBlock::Text { text, .. } if text.contains("Permission denied")));
                if has_deny_text {
                    found_blocked = true;
                }
            }
        }
    }
    assert!(
        found_blocked,
        "blocked tool should produce error with reason"
    );
}

#[tokio::test]
async fn post_tool_use_overrides_result() {
    let tool = Arc::new(FakeTool {
        name: "test_tool".to_string(),
        response: "original result".to_string(),
        schema: serde_json::json!({ "type": "object" }),
    }) as Arc<dyn Tool>;

    let config = {
        let mut base = config_with_stream(|| {
            let (sender, stream) = create_event_stream();
            let message = assistant_message(vec![ContentBlock::ToolCall(ToolCall {
                id: "call1".to_string(),
                name: "test_tool".to_string(),
                arguments: serde_json::json!({}),
            })]);
            tokio::spawn(async move {
                sender.push(StreamEvent::Start {
                    partial: assistant_message(Vec::new()),
                });
                sender.push(StreamEvent::Done {
                    reason: StopReason::Stop,
                    message,
                });
            });
            stream
        });
        base.tools = vec![tool];
        base.post_tool_use = Some(Box::new(|_ctx| {
            Some(rozsa_core::config::PostToolUseResult {
                content: Some(vec![ContentBlock::Text {
                    text: "overridden".to_string(),
                    signature: None,
                }]),
                is_error: None,
                terminate: Some(true),
            })
        }));
        base
    };

    let prompt = AgentMessage::standard(Message::User(UserMessage {
        content: UserContent::Text("hello".to_string()),
        display_text: None,
        timestamp: 1,
    }));
    let mut stream = agent_loop(vec![prompt], empty_context(), config, None);
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }

    let mut found_override = false;
    for event in &events {
        if let AgentEvent::ToolExecutionEnd { result, .. } = event {
            let has_override = result
                .content
                .iter()
                .any(|b| matches!(b, ContentBlock::Text { text, .. } if text == "overridden"));
            if has_override {
                found_override = true;
            }
        }
    }
    assert!(
        found_override,
        "post_tool_use should override result content"
    );
}

#[tokio::test]
async fn all_tools_terminate_ends_tool_loop() {
    struct TerminateTool {
        schema: serde_json::Value,
    }

    #[async_trait::async_trait]
    impl Tool for TerminateTool {
        fn name(&self) -> &str {
            "term_tool"
        }
        fn description(&self) -> &str {
            "terminates"
        }
        fn label(&self) -> &str {
            "term"
        }
        fn parameters_schema(&self) -> &serde_json::Value {
            &self.schema
        }
        async fn execute(
            &self,
            _tool_call_id: &str,
            _params: serde_json::Value,
            _signal: Option<CancellationToken>,
            _on_update: Option<&(dyn Fn(ToolResult) + Send + Sync)>,
        ) -> Result<ToolResult, ToolError> {
            Ok(ToolResult {
                content: vec![ContentBlock::Text {
                    text: "done".to_string(),
                    signature: None,
                }],
                details: serde_json::Value::Null,
                terminate: true,
            })
        }
    }

    use std::sync::Mutex;
    let stream_count = Arc::new(Mutex::new(0));
    let stream_count_clone = stream_count.clone();
    let tool: Arc<dyn Tool> = Arc::new(TerminateTool {
        schema: serde_json::json!({ "type": "object" }),
    });

    let config = {
        let mut base = config_with_stream(move || {
            let (sender, stream) = create_event_stream();
            let mut count = stream_count_clone.lock().unwrap();
            *count += 1;
            let message = assistant_message(vec![ContentBlock::ToolCall(ToolCall {
                id: "call1".to_string(),
                name: "term_tool".to_string(),
                arguments: serde_json::json!({}),
            })]);
            tokio::spawn(async move {
                sender.push(StreamEvent::Start {
                    partial: assistant_message(Vec::new()),
                });
                sender.push(StreamEvent::Done {
                    reason: StopReason::Stop,
                    message,
                });
            });
            stream
        });
        base.tools = vec![tool];
        base
    };

    let prompt = AgentMessage::standard(Message::User(UserMessage {
        content: UserContent::Text("hello".to_string()),
        display_text: None,
        timestamp: 1,
    }));
    let mut stream = agent_loop(vec![prompt], empty_context(), config, None);
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }

    assert_eq!(
        *stream_count.lock().unwrap(),
        1,
        "terminate=true should stop tool loop, so only 1 model call"
    );
}

#[tokio::test]
async fn error_stop_reason_exits_immediately() {
    let config = config_with_stream(|| {
        let (sender, stream) = create_event_stream();
        let mut message = assistant_message(vec![ContentBlock::Text {
            text: "error occurred".to_string(),
            signature: None,
        }]);
        message.stop_reason = StopReason::Error;
        message.error_message = Some("rate limit".to_string());
        tokio::spawn(async move {
            sender.push(StreamEvent::Error {
                reason: StopReason::Error,
                error: message,
            });
        });
        stream
    });

    let prompt = AgentMessage::standard(Message::User(UserMessage {
        content: UserContent::Text("hello".to_string()),
        display_text: None,
        timestamp: 1,
    }));
    let mut stream = agent_loop(vec![prompt], empty_context(), config, None);
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }

    let names: Vec<_> = events.iter().map(event_name).collect();
    assert!(names.contains(&"turn_end"));
    assert!(names.contains(&"agent_end"));
    assert_eq!(
        names.iter().filter(|&&n| n == "turn_start").count(),
        1,
        "should have only one turn"
    );
}

#[tokio::test]
async fn parallel_panic_produces_error_result_not_silent_drop() {
    struct PanicTool {
        schema: serde_json::Value,
    }

    #[async_trait::async_trait]
    impl Tool for PanicTool {
        fn name(&self) -> &str {
            "panic_tool"
        }
        fn description(&self) -> &str {
            "panics"
        }
        fn label(&self) -> &str {
            "panic"
        }
        fn parameters_schema(&self) -> &serde_json::Value {
            &self.schema
        }
        async fn execute(
            &self,
            _tool_call_id: &str,
            _params: serde_json::Value,
            _signal: Option<CancellationToken>,
            _on_update: Option<&(dyn Fn(ToolResult) + Send + Sync)>,
        ) -> Result<ToolResult, rozsa_core::tool::ToolError> {
            panic!("deliberate panic in tool");
        }
    }

    let panic_tool: Arc<dyn Tool> = Arc::new(PanicTool {
        schema: serde_json::json!({ "type": "object" }),
    });
    let ok_tool: Arc<dyn Tool> = Arc::new(FakeTool {
        name: "ok_tool".to_string(),
        response: "ok".to_string(),
        schema: serde_json::json!({ "type": "object" }),
    });

    let config = {
        let mut base = config_with_stream(|| {
            let (sender, stream) = create_event_stream();
            let message = assistant_message(vec![
                ContentBlock::ToolCall(ToolCall {
                    id: "call_panic".to_string(),
                    name: "panic_tool".to_string(),
                    arguments: serde_json::json!({}),
                }),
                ContentBlock::ToolCall(ToolCall {
                    id: "call_ok".to_string(),
                    name: "ok_tool".to_string(),
                    arguments: serde_json::json!({}),
                }),
            ]);
            tokio::spawn(async move {
                sender.push(StreamEvent::Start {
                    partial: assistant_message(Vec::new()),
                });
                sender.push(StreamEvent::Done {
                    reason: StopReason::Stop,
                    message,
                });
            });
            stream
        });
        base.tools = vec![panic_tool, ok_tool];
        base.tool_execution = ToolExecutionMode::Parallel;
        base.should_stop_after_turn = Some(Box::new(|_| true));
        base
    };

    let prompt = AgentMessage::standard(Message::User(UserMessage {
        content: UserContent::Text("hello".to_string()),
        display_text: None,
        timestamp: 1,
    }));
    let mut stream = agent_loop(vec![prompt], empty_context(), config, None);
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }

    // Both ToolExecutionStart events should have matching ToolExecutionEnd events
    let starts: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let AgentEvent::ToolExecutionStart { tool_call_id, .. } = e {
                Some(tool_call_id.clone())
            } else {
                None
            }
        })
        .collect();
    let ends: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let AgentEvent::ToolExecutionEnd { tool_call_id, .. } = e {
                Some(tool_call_id.clone())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(starts.len(), 2, "should have 2 tool execution starts");
    assert_eq!(
        ends.len(),
        2,
        "should have 2 tool execution ends (panic recovered)"
    );
    assert_eq!(starts, ends, "start/end ids should match in order");

    // The panicked tool should produce an error result
    let panic_result = events
        .iter()
        .find_map(|e| {
            if let AgentEvent::ToolExecutionEnd {
                tool_call_id,
                result,
                ..
            } = e
            {
                if tool_call_id == "call_panic" {
                    Some(result)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .expect("should have end event for panic tool");
    assert!(panic_result.is_error);
    let has_panic_msg = panic_result
        .content
        .iter()
        .any(|b| matches!(b, ContentBlock::Text { text, .. } if text.contains("panicked")));
    assert!(has_panic_msg, "error should mention panic");

    // AgentEnd should be reached
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AgentEvent::AgentEnd { .. }))
    );
}

#[tokio::test]
async fn cancel_during_model_stream_stops_waiting_for_next_event() {
    let config = config_with_stream(|| {
        let (sender, stream) = create_event_stream();
        tokio::spawn(async move {
            let partial = assistant_message(vec![ContentBlock::Text {
                text: "partial text".to_string(),
                signature: None,
            }]);
            sender.push(StreamEvent::Start {
                partial: assistant_message(Vec::new()),
            });
            sender.push(StreamEvent::TextDelta {
                content_index: 0,
                delta: "partial text".to_string(),
                partial,
            });
            std::future::pending::<()>().await;
        });
        stream
    });
    let prompt = AgentMessage::standard(Message::User(UserMessage {
        content: UserContent::Text("hello".to_string()),
        display_text: None,
        timestamp: 1,
    }));
    let cancel_token = CancellationToken::new();
    let cancel_clone = cancel_token.clone();

    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        cancel_clone.cancel();
    });

    let mut stream = agent_loop(vec![prompt], empty_context(), config, Some(cancel_token));
    let events = tokio::time::timeout(tokio::time::Duration::from_secs(1), async {
        let mut events = Vec::new();
        while let Some(event) = stream.next().await {
            events.push(event);
        }
        events
    })
    .await
    .expect("agent loop should stop when model stream is cancelled");

    let turn_end = events
        .iter()
        .find_map(|event| {
            if let AgentEvent::TurnEnd { message, .. } = event {
                Some(message)
            } else {
                None
            }
        })
        .expect("cancelled stream should emit a turn end");
    assert_eq!(turn_end.stop_reason, StopReason::Aborted);
    assert!(
        turn_end.content.iter().any(
            |block| matches!(block, ContentBlock::Text { text, .. } if text == "partial text")
        )
    );

    let persisted = events
        .iter()
        .find_map(|event| {
            if let AgentEvent::AgentEnd { messages } = event {
                messages.iter().find_map(|msg| {
                    if let Some(Message::Assistant(message)) = msg.as_standard() {
                        Some(message)
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        })
        .expect("cancelled partial assistant message should be included in AgentEnd");
    assert_eq!(persisted.stop_reason, StopReason::Aborted);
    assert!(
        persisted.content.iter().any(
            |block| matches!(block, ContentBlock::Text { text, .. } if text == "partial text")
        )
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, AgentEvent::AgentEnd { .. }))
    );
}

#[tokio::test]
async fn cancel_during_permission_hook_stops_without_a_permission_response() {
    let mut config = config_with_stream(|| {
        let (sender, stream) = create_event_stream();
        sender.push(StreamEvent::Done {
            reason: StopReason::ToolUse,
            message: assistant_message(vec![ContentBlock::ToolCall(ToolCall {
                id: "approval-call".to_string(),
                name: "write".to_string(),
                arguments: serde_json::json!({}),
            })]),
        });
        stream
    });
    config.tools = vec![Arc::new(FakeTool {
        name: "write".to_string(),
        response: "unreachable".to_string(),
        schema: serde_json::json!({ "type": "object" }),
    })];
    config.pre_tool_use = Some(Box::new(|_| {
        Box::pin(std::future::pending::<
            Option<rozsa_core::config::PreToolUseResult>,
        >())
    }));
    let prompt = AgentMessage::standard(Message::User(UserMessage {
        content: UserContent::Text("write a file".to_string()),
        display_text: None,
        timestamp: 1,
    }));
    let cancel_token = CancellationToken::new();
    let cancel_clone = cancel_token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        cancel_clone.cancel();
    });

    let mut stream = agent_loop(vec![prompt], empty_context(), config, Some(cancel_token));
    let events = tokio::time::timeout(tokio::time::Duration::from_secs(1), async {
        let mut events = Vec::new();
        while let Some(event) = stream.next().await {
            events.push(event);
        }
        events
    })
    .await
    .expect("agent loop should not wait for permission after cancellation");

    assert!(events.iter().any(|event| matches!(
        event,
        AgentEvent::ToolExecutionEnd { result, .. } if result.is_error
    )));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, AgentEvent::AgentEnd { .. }))
    );
}

#[tokio::test]
async fn parallel_cancel_aborts_pending_handles() {
    struct SlowTool {
        name: String,
        schema: serde_json::Value,
    }

    #[async_trait::async_trait]
    impl Tool for SlowTool {
        fn name(&self) -> &str {
            &self.name
        }
        fn description(&self) -> &str {
            "slow tool"
        }
        fn label(&self) -> &str {
            "slow"
        }
        fn parameters_schema(&self) -> &serde_json::Value {
            &self.schema
        }
        async fn execute(
            &self,
            _tool_call_id: &str,
            _params: serde_json::Value,
            _signal: Option<CancellationToken>,
            _on_update: Option<&(dyn Fn(ToolResult) + Send + Sync)>,
        ) -> Result<ToolResult, ToolError> {
            // Simulate an external tool or OS prompt that never observes the token.
            std::future::pending().await
        }
    }

    let tool1: Arc<dyn Tool> = Arc::new(SlowTool {
        name: "slow1".to_string(),
        schema: serde_json::json!({ "type": "object" }),
    });
    let tool2: Arc<dyn Tool> = Arc::new(SlowTool {
        name: "slow2".to_string(),
        schema: serde_json::json!({ "type": "object" }),
    });

    let config = {
        let mut base = config_with_stream(|| {
            let (sender, stream) = create_event_stream();
            let message = assistant_message(vec![
                ContentBlock::ToolCall(ToolCall {
                    id: "call1".to_string(),
                    name: "slow1".to_string(),
                    arguments: serde_json::json!({}),
                }),
                ContentBlock::ToolCall(ToolCall {
                    id: "call2".to_string(),
                    name: "slow2".to_string(),
                    arguments: serde_json::json!({}),
                }),
            ]);
            tokio::spawn(async move {
                sender.push(StreamEvent::Start {
                    partial: assistant_message(Vec::new()),
                });
                sender.push(StreamEvent::Done {
                    reason: StopReason::Stop,
                    message,
                });
            });
            stream
        });
        base.tools = vec![tool1, tool2];
        base.tool_execution = ToolExecutionMode::Parallel;
        base.should_stop_after_turn = Some(Box::new(|_| true));
        base
    };

    let prompt = AgentMessage::standard(Message::User(UserMessage {
        content: UserContent::Text("hello".to_string()),
        display_text: None,
        timestamp: 1,
    }));

    let cancel_token = CancellationToken::new();
    let cancel_clone = cancel_token.clone();

    // Cancel after a short delay
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        cancel_clone.cancel();
    });

    let mut stream = agent_loop(vec![prompt], empty_context(), config, Some(cancel_token));
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }

    // Both tools should have start events
    let starts: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let AgentEvent::ToolExecutionStart { tool_call_id, .. } = e {
                Some(tool_call_id.clone())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(starts.len(), 2, "should have 2 tool execution starts");

    // Both tools should have end events (cancelled tools produce error results)
    let ends: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let AgentEvent::ToolExecutionEnd {
                tool_call_id,
                result,
                ..
            } = e
            {
                Some((tool_call_id.clone(), result.is_error))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        ends.len(),
        2,
        "should have 2 tool execution ends (cancelled tools produce errors)"
    );

    // At least one should be an error (from cancellation)
    let error_count = ends.iter().filter(|(_, is_error)| *is_error).count();
    assert!(
        error_count >= 1,
        "at least one tool should have error result from cancellation"
    );

    // Agent should complete
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AgentEvent::AgentEnd { .. }))
    );
}
