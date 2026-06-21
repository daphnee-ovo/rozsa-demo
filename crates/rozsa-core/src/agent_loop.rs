use crate::config::{AgentContext, AgentLoopConfig};
use crate::events::AgentEvent;
use crate::messages::AgentMessage;
use rozsa_model::event_stream::{EventStream, EventStreamSender, create_event_stream};
use rozsa_model::types::{AssistantMessage, Context as ModelContext, Message, StreamEvent};
use tokio_util::sync::CancellationToken;

pub fn agent_loop(
    prompts: Vec<AgentMessage>,
    context: AgentContext,
    config: AgentLoopConfig,
    signal: Option<CancellationToken>,
) -> EventStream<AgentEvent> {
    let (sender, stream) = create_event_stream();
    tokio::spawn(async move {
        run_loop(prompts, context, config, sender, signal).await;
    });
    stream
}

pub fn agent_loop_continue(
    context: AgentContext,
    config: AgentLoopConfig,
    signal: Option<CancellationToken>,
) -> EventStream<AgentEvent> {
    let (sender, stream) = create_event_stream();
    tokio::spawn(async move {
        run_loop(vec![], context, config, sender, signal).await;
    });
    stream
}

async fn run_loop(
    prompts: Vec<AgentMessage>,
    mut context: AgentContext,
    config: AgentLoopConfig,
    emit: EventStreamSender<AgentEvent>,
    signal: Option<CancellationToken>,
) {
    let mut new_messages = Vec::new();

    emit.push(AgentEvent::AgentStart);
    emit.push(AgentEvent::TurnStart);

    for prompt in prompts {
        emit.push(AgentEvent::MessageStart {
            message: prompt.clone(),
        });
        emit.push(AgentEvent::MessageEnd {
            message: prompt.clone(),
        });
        context.messages.push(prompt.clone());
        new_messages.push(prompt);
    }

    if is_cancelled(signal.as_ref()) {
        emit.push(AgentEvent::AgentEnd {
            messages: new_messages,
        });
        return;
    }

    let model_context = build_model_context(&context, &config);
    let model_stream = (config.model_stream)(&config.model, &model_context, &config.stream_options);
    let Some(message) = stream_assistant_response(model_stream, &emit, signal.as_ref()).await
    else {
        emit.push(AgentEvent::AgentEnd {
            messages: new_messages,
        });
        return;
    };

    let agent_message = assistant_agent_message(message.clone());
    context.messages.push(agent_message.clone());
    new_messages.push(agent_message);

    emit.push(AgentEvent::TurnEnd {
        message,
        tool_results: Vec::new(),
    });
    emit.push(AgentEvent::AgentEnd {
        messages: new_messages,
    });
}

fn build_model_context(context: &AgentContext, config: &AgentLoopConfig) -> ModelContext {
    let messages = if let Some(transform_context) = &config.transform_context {
        transform_context(&context.messages)
    } else {
        context.messages.clone()
    };

    ModelContext {
        system_prompt: context.system_prompt.clone(),
        messages: (config.convert_to_llm)(&messages),
        tools: context.tools.clone(),
    }
}

async fn stream_assistant_response(
    mut stream: EventStream<StreamEvent>,
    emit: &EventStreamSender<AgentEvent>,
    signal: Option<&CancellationToken>,
) -> Option<AssistantMessage> {
    let mut started = false;

    while let Some(event) = stream.next().await {
        if is_cancelled(signal) {
            return None;
        }

        match event {
            StreamEvent::Start { partial } => {
                started = true;
                emit.push(AgentEvent::MessageStart {
                    message: assistant_agent_message(partial),
                });
            }
            StreamEvent::Done { message, .. } => {
                if !started {
                    emit.push(AgentEvent::MessageStart {
                        message: assistant_agent_message(message.clone()),
                    });
                }
                emit.push(AgentEvent::MessageEnd {
                    message: assistant_agent_message(message.clone()),
                });
                return Some(message);
            }
            StreamEvent::Error { error, .. } => {
                if !started {
                    emit.push(AgentEvent::MessageStart {
                        message: assistant_agent_message(error.clone()),
                    });
                }
                emit.push(AgentEvent::MessageEnd {
                    message: assistant_agent_message(error.clone()),
                });
                return Some(error);
            }
            event => {
                let Some(partial) = stream_event_partial(&event) else {
                    continue;
                };
                emit.push(AgentEvent::MessageUpdate {
                    message: assistant_agent_message(partial),
                    stream_event: event,
                });
            }
        }
    }

    None
}

fn stream_event_partial(event: &StreamEvent) -> Option<AssistantMessage> {
    match event {
        StreamEvent::TextStart { partial, .. }
        | StreamEvent::TextDelta { partial, .. }
        | StreamEvent::TextEnd { partial, .. }
        | StreamEvent::ThinkingStart { partial, .. }
        | StreamEvent::ThinkingDelta { partial, .. }
        | StreamEvent::ThinkingEnd { partial, .. }
        | StreamEvent::ToolCallStart { partial, .. }
        | StreamEvent::ToolCallDelta { partial, .. }
        | StreamEvent::ToolCallEnd { partial, .. } => Some(partial.clone()),
        StreamEvent::Start { .. } | StreamEvent::Done { .. } | StreamEvent::Error { .. } => None,
    }
}

fn assistant_agent_message(message: AssistantMessage) -> AgentMessage {
    AgentMessage::standard(Message::Assistant(message))
}

fn is_cancelled(signal: Option<&CancellationToken>) -> bool {
    signal.is_some_and(CancellationToken::is_cancelled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rozsa_model::event_stream::create_event_stream;
    use rozsa_model::types::{
        Api, CacheRetention, ContentBlock, InputModality, ModelCost, Provider, SimpleStreamOptions,
        StopReason, StreamOptions, Transport, Usage, UsageCost, UserContent, UserMessage,
    };

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

    fn config_with_stream(
        make_stream: impl Fn() -> EventStream<StreamEvent> + Send + Sync + 'static,
    ) -> AgentLoopConfig {
        AgentLoopConfig {
            model: model(),
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
            tool_execution: crate::tool::ToolExecutionMode::Sequential,
            before_tool_call: None,
            after_tool_call: None,
        }
    }

    fn empty_context() -> AgentContext {
        AgentContext {
            system_prompt: Some("system".to_string()),
            messages: Vec::new(),
            tools: Vec::new(),
        }
    }

    fn assistant_message(content: Vec<ContentBlock>) -> AssistantMessage {
        AssistantMessage {
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
}
