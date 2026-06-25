use crate::config::{AgentContext, AgentLoopConfig, ShouldStopContext};
use crate::events::AgentEvent;
use crate::messages::AgentMessage;
use crate::tool::ToolExecutionMode;
use rozsa_model::event_stream::{EventStream, EventStreamSender, create_event_stream};
use rozsa_model::types::{
    AssistantMessage, ContentBlock, Context as ModelContext, Message, StopReason, StreamEvent,
    ThinkingLevel, ToolCall, ToolResultMessage,
};
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
    if context.messages.is_empty() {
        let (sender, stream) = create_event_stream();
        sender.push(AgentEvent::AgentStart);
        sender.push(AgentEvent::AgentEnd {
            messages: vec![],
        });
        return stream;
    }

    if let Some(last) = context.messages.last() {
        if let Some(Message::Assistant(_)) = last.as_standard() {
            let (sender, stream) = create_event_stream();
            sender.push(AgentEvent::AgentStart);
            sender.push(AgentEvent::AgentEnd {
                messages: vec![],
            });
            return stream;
        }
    }

    let (sender, stream) = create_event_stream();
    tokio::spawn(async move {
        run_loop(vec![], context, config, sender, signal).await;
    });
    stream
}

async fn run_loop(
    prompts: Vec<AgentMessage>,
    mut context: AgentContext,
    mut config: AgentLoopConfig,
    emit: EventStreamSender<AgentEvent>,
    signal: Option<CancellationToken>,
) {
    let mut new_messages = Vec::new();
    let mut first_turn = true;

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

    let mut pending_messages: Vec<AgentMessage> =
        config.get_steering_messages.as_ref().map_or_else(Vec::new, |f| f());

    loop {
        let mut has_more_tool_calls = true;

        while has_more_tool_calls || !pending_messages.is_empty() {
            if is_cancelled(signal.as_ref()) {
                emit.push(AgentEvent::AgentEnd {
                    messages: new_messages,
                });
                return;
            }

            if !first_turn {
                emit.push(AgentEvent::TurnStart);
            } else {
                first_turn = false;
            }

            if !pending_messages.is_empty() {
                for msg in std::mem::take(&mut pending_messages) {
                    emit.push(AgentEvent::MessageStart {
                        message: msg.clone(),
                    });
                    emit.push(AgentEvent::MessageEnd {
                        message: msg.clone(),
                    });
                    context.messages.push(msg.clone());
                    new_messages.push(msg);
                }
            }

            let model_context = build_model_context(&context, &config);
            let model_stream =
                (config.model_stream)(&config.model, &model_context, &config.stream_options);
            let Some(message) =
                stream_assistant_response(model_stream, &emit, signal.as_ref(), &mut context).await
            else {
                emit.push(AgentEvent::AgentEnd {
                    messages: new_messages,
                });
                return;
            };

            new_messages.push(assistant_agent_message(message.clone()));

            if matches!(message.stop_reason, StopReason::Error | StopReason::Aborted) {
                emit.push(AgentEvent::TurnEnd {
                    message,
                    tool_results: vec![],
                });
                emit.push(AgentEvent::AgentEnd {
                    messages: new_messages,
                });
                return;
            }

            let tool_calls: Vec<ToolCall> = message
                .content
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::ToolCall(tc) => Some(tc.clone()),
                    _ => None,
                })
                .collect();

            let mut tool_results: Vec<ToolResultMessage> = Vec::new();
            has_more_tool_calls = false;

            if !tool_calls.is_empty() {
                let batch =
                    execute_tool_batch(&tool_calls, &message, &context, &config, &emit, signal.as_ref()).await;
                has_more_tool_calls = !batch.terminate;
                tool_results = batch.messages;

                for result in &tool_results {
                    let msg = AgentMessage::standard(Message::ToolResult(result.clone()));
                    context.messages.push(msg.clone());
                    new_messages.push(msg);
                }
            }

            emit.push(AgentEvent::TurnEnd {
                message: message.clone(),
                tool_results: tool_results.clone(),
            });

            let stop_ctx = ShouldStopContext {
                message: message.clone(),
                tool_results,
                context: context.clone(),
                new_messages: new_messages.clone(),
            };

            if let Some(ref prepare_next) = config.prepare_next_turn {
                if let Some(update) = prepare_next(&stop_ctx) {
                    if let Some(new_ctx) = update.context {
                        context = new_ctx;
                    }
                    if let Some(new_model) = update.model {
                        config.model = new_model;
                    }
                    match update.thinking_level {
                        Some(ThinkingLevel::Off) => config.reasoning = None,
                        Some(level) => config.reasoning = Some(level),
                        None => {}
                    }
                }
            }

            if let Some(ref should_stop) = config.should_stop_after_turn {
                if should_stop(&stop_ctx) {
                    emit.push(AgentEvent::AgentEnd {
                        messages: new_messages,
                    });
                    return;
                }
            }

            pending_messages =
                config.get_steering_messages.as_ref().map_or_else(Vec::new, |f| f());
        }

        let follow_ups = config
            .get_follow_up_messages
            .as_ref()
            .map_or_else(Vec::new, |f| f());

        if !follow_ups.is_empty() {
            pending_messages = follow_ups;
            continue;
        }

        break;
    }

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
    context: &mut AgentContext,
) -> Option<AssistantMessage> {
    let mut added_partial = false;

    while let Some(event) = stream.next().await {
        if is_cancelled(signal) {
            return None;
        }

        match event {
            StreamEvent::Start { partial } => {
                added_partial = true;
                let msg = assistant_agent_message(partial);
                context.messages.push(msg.clone());
                emit.push(AgentEvent::MessageStart { message: msg });
            }
            StreamEvent::Done { message, .. } => {
                let final_msg = assistant_agent_message(message.clone());
                if added_partial {
                    *context.messages.last_mut().unwrap() = final_msg.clone();
                } else {
                    context.messages.push(final_msg.clone());
                    emit.push(AgentEvent::MessageStart {
                        message: final_msg.clone(),
                    });
                }
                emit.push(AgentEvent::MessageEnd { message: final_msg });
                return Some(message);
            }
            StreamEvent::Error { error, .. } => {
                let final_msg = assistant_agent_message(error.clone());
                if added_partial {
                    *context.messages.last_mut().unwrap() = final_msg.clone();
                } else {
                    context.messages.push(final_msg.clone());
                    emit.push(AgentEvent::MessageStart {
                        message: final_msg.clone(),
                    });
                }
                emit.push(AgentEvent::MessageEnd { message: final_msg });
                return Some(error);
            }
            event => {
                let Some(partial) = stream_event_partial(&event) else {
                    continue;
                };
                if added_partial {
                    *context.messages.last_mut().unwrap() =
                        assistant_agent_message(partial.clone());
                }
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

struct ToolBatchResult {
    messages: Vec<ToolResultMessage>,
    terminate: bool,
}

struct FinalizedToolCall {
    tool_call: ToolCall,
    content: Vec<ContentBlock>,
    is_error: bool,
    terminate: bool,
}

async fn execute_tool_batch(
    tool_calls: &[ToolCall],
    assistant_message: &AssistantMessage,
    context: &AgentContext,
    config: &AgentLoopConfig,
    emit: &EventStreamSender<AgentEvent>,
    signal: Option<&CancellationToken>,
) -> ToolBatchResult {
    let has_sequential = tool_calls.iter().any(|tc| {
        config
            .tools
            .iter()
            .find(|t| t.name() == tc.name)
            .and_then(|t| t.execution_mode())
            == Some(ToolExecutionMode::Sequential)
    });

    if config.tool_execution == ToolExecutionMode::Sequential || has_sequential {
        execute_sequential(tool_calls, assistant_message, context, config, emit, signal).await
    } else {
        execute_parallel(tool_calls, assistant_message, context, config, emit, signal).await
    }
}

async fn execute_sequential(
    tool_calls: &[ToolCall],
    assistant_message: &AssistantMessage,
    context: &AgentContext,
    config: &AgentLoopConfig,
    emit: &EventStreamSender<AgentEvent>,
    signal: Option<&CancellationToken>,
) -> ToolBatchResult {
    let mut finalized_calls: Vec<FinalizedToolCall> = Vec::new();
    let mut messages: Vec<ToolResultMessage> = Vec::new();

    for call in tool_calls {
        if is_cancelled(signal) {
            break;
        }

        emit.push(AgentEvent::ToolExecutionStart {
            tool_call_id: call.id.clone(),
            tool_name: call.name.clone(),
            args: call.arguments.clone(),
        });

        let finalized = execute_single_tool(call, assistant_message, context, config, signal).await;

        let result_msg = finalized_to_result_message(&finalized);
        emit.push(AgentEvent::ToolExecutionEnd {
            tool_call_id: call.id.clone(),
            tool_name: call.name.clone(),
            result: result_msg.clone(),
        });
        emit.push(AgentEvent::MessageStart {
            message: AgentMessage::standard(Message::ToolResult(result_msg.clone())),
        });
        emit.push(AgentEvent::MessageEnd {
            message: AgentMessage::standard(Message::ToolResult(result_msg.clone())),
        });
        messages.push(result_msg);

        finalized_calls.push(finalized);
    }

    let terminate = should_terminate_batch(&finalized_calls);
    ToolBatchResult { messages, terminate }
}

enum PreparedEntry {
    Immediate(FinalizedToolCall),
    Pending(tokio::task::JoinHandle<(ToolCall, Result<crate::tool::ToolResult, crate::tool::ToolError>)>),
}

async fn execute_parallel(
    tool_calls: &[ToolCall],
    assistant_message: &AssistantMessage,
    context: &AgentContext,
    config: &AgentLoopConfig,
    emit: &EventStreamSender<AgentEvent>,
    signal: Option<&CancellationToken>,
) -> ToolBatchResult {
    let mut entries: Vec<PreparedEntry> = Vec::new();

    for call in tool_calls {
        if is_cancelled(signal) {
            break;
        }

        emit.push(AgentEvent::ToolExecutionStart {
            tool_call_id: call.id.clone(),
            tool_name: call.name.clone(),
            args: call.arguments.clone(),
        });

        let tool = config.tools.iter().find(|t| t.name() == call.name);
        let Some(tool) = tool else {
            let finalized = make_error_finalized(call, format!("Tool '{}' not found", call.name));
            emit.push(AgentEvent::ToolExecutionEnd {
                tool_call_id: call.id.clone(),
                tool_name: call.name.clone(),
                result: finalized_to_result_message(&finalized),
            });
            entries.push(PreparedEntry::Immediate(finalized));
            continue;
        };

        if let Some(ref before) = config.pre_tool_use {
            let ctx = crate::config::PreToolUseContext {
                assistant_message: assistant_message.clone(),
                tool_call_id: call.id.clone(),
                tool_name: call.name.clone(),
                args: call.arguments.clone(),
                context: context.clone(),
            };
            if let Some(result) = before(ctx).await {
                if result.block {
                    let reason = result.reason.unwrap_or_else(|| "Tool execution was blocked".to_string());
                    let finalized = make_error_finalized(call, reason);
                    emit.push(AgentEvent::ToolExecutionEnd {
                        tool_call_id: call.id.clone(),
                        tool_name: call.name.clone(),
                        result: finalized_to_result_message(&finalized),
                    });
                    entries.push(PreparedEntry::Immediate(finalized));
                    continue;
                }
            }
        }

        let tool_clone = tool.clone();
        let call_clone = call.clone();
        let signal_clone = signal.cloned();
        let handle = tokio::spawn(async move {
            let args = if call_clone.arguments.is_null() {
                serde_json::Value::Object(Default::default())
            } else {
                call_clone.arguments.clone()
            };
            let exec_result = tool_clone
                .execute(
                    &call_clone.id,
                    args,
                    signal_clone,
                    None,
                )
                .await;
            (call_clone, exec_result)
        });
        entries.push(PreparedEntry::Pending(handle));

        if is_cancelled(signal) {
            break;
        }
    }

    // Await in source order (like TS Promise.all) — emit tool_execution_end per tool
    let mut finalized_calls: Vec<FinalizedToolCall> = Vec::new();
    for entry in entries {
        let finalized = match entry {
            PreparedEntry::Immediate(f) => f,
            PreparedEntry::Pending(handle) => {
                let Ok((call, exec_result)) = handle.await else {
                    continue;
                };

                let (content, is_error, terminate) = match exec_result {
                    Ok(result) => (result.content, false, result.terminate),
                    Err(err) => (
                        vec![ContentBlock::Text {
                            text: format!("{}", err),
                            signature: None,
                        }],
                        true,
                        false,
                    ),
                };

                let mut f = FinalizedToolCall {
                    tool_call: call.clone(),
                    content,
                    is_error,
                    terminate,
                };

                if let Some(ref after) = config.post_tool_use {
                    let ctx = crate::config::PostToolUseContext {
                        assistant_message: assistant_message.clone(),
                        tool_call_id: call.id.clone(),
                        tool_name: call.name.clone(),
                        args: call.arguments.clone(),
                        result: crate::tool::ToolResult {
                            content: f.content.clone(),
                            details: serde_json::Value::Null,
                            terminate: f.terminate,
                        },
                        is_error: f.is_error,
                        context: context.clone(),
                    };
                    if let Some(override_result) = after(&ctx) {
                        if let Some(c) = override_result.content {
                            f.content = c;
                        }
                        if let Some(e) = override_result.is_error {
                            f.is_error = e;
                        }
                        if let Some(t) = override_result.terminate {
                            f.terminate = t;
                        }
                    }
                }

                emit.push(AgentEvent::ToolExecutionEnd {
                    tool_call_id: f.tool_call.id.clone(),
                    tool_name: f.tool_call.name.clone(),
                    result: finalized_to_result_message(&f),
                });

                f
            }
        };
        finalized_calls.push(finalized);
    }

    // Emit tool result messages in source order (after all tool_execution_end)
    let terminate = should_terminate_batch(&finalized_calls);
    let messages: Vec<ToolResultMessage> = finalized_calls
        .iter()
        .map(finalized_to_result_message)
        .collect();

    for msg in &messages {
        emit.push(AgentEvent::MessageStart {
            message: AgentMessage::standard(Message::ToolResult(msg.clone())),
        });
        emit.push(AgentEvent::MessageEnd {
            message: AgentMessage::standard(Message::ToolResult(msg.clone())),
        });
    }

    ToolBatchResult { messages, terminate }
}

async fn execute_single_tool(
    call: &ToolCall,
    assistant_message: &AssistantMessage,
    context: &AgentContext,
    config: &AgentLoopConfig,
    signal: Option<&CancellationToken>,
) -> FinalizedToolCall {
    let tool = config.tools.iter().find(|t| t.name() == call.name);
    let Some(tool) = tool else {
        return make_error_finalized(call, format!("Tool '{}' not found", call.name));
    };

    if let Some(ref before) = config.pre_tool_use {
        let ctx = crate::config::PreToolUseContext {
            assistant_message: assistant_message.clone(),
            tool_call_id: call.id.clone(),
            tool_name: call.name.clone(),
            args: call.arguments.clone(),
            context: context.clone(),
        };
        if let Some(result) = before(ctx).await {
            if result.block {
                let reason = result.reason.unwrap_or_else(|| "Tool execution was blocked".to_string());
                return make_error_finalized(call, reason);
            }
        }
    }

    let args = if call.arguments.is_null() {
        serde_json::Value::Object(Default::default())
    } else {
        call.arguments.clone()
    };

    let exec_result = tool
        .execute(&call.id, args, signal.cloned(), None)
        .await;

    let (content, is_error, terminate) = match exec_result {
        Ok(result) => (result.content, false, result.terminate),
        Err(err) => (
            vec![ContentBlock::Text {
                text: format!("{}", err),
                signature: None,
            }],
            true,
            false,
        ),
    };

    let mut finalized = FinalizedToolCall {
        tool_call: call.clone(),
        content,
        is_error,
        terminate,
    };

    if let Some(ref after) = config.post_tool_use {
        let ctx = crate::config::PostToolUseContext {
            assistant_message: assistant_message.clone(),
            tool_call_id: call.id.clone(),
            tool_name: call.name.clone(),
            args: call.arguments.clone(),
            result: crate::tool::ToolResult {
                content: finalized.content.clone(),
                details: serde_json::Value::Null,
                terminate: finalized.terminate,
            },
            is_error: finalized.is_error,
            context: context.clone(),
        };
        if let Some(override_result) = after(&ctx) {
            if let Some(c) = override_result.content {
                finalized.content = c;
            }
            if let Some(e) = override_result.is_error {
                finalized.is_error = e;
            }
            if let Some(t) = override_result.terminate {
                finalized.terminate = t;
            }
        }
    }

    finalized
}

fn make_error_finalized(call: &ToolCall, message: String) -> FinalizedToolCall {
    FinalizedToolCall {
        tool_call: call.clone(),
        content: vec![ContentBlock::Text {
            text: message,
            signature: None,
        }],
        is_error: true,
        terminate: false,
    }
}

fn finalized_to_result_message(f: &FinalizedToolCall) -> ToolResultMessage {
    ToolResultMessage {
        tool_call_id: f.tool_call.id.clone(),
        tool_name: f.tool_call.name.clone(),
        content: f.content.clone(),
        is_error: f.is_error,
        timestamp: current_timestamp_ms(),
    }
}

fn current_timestamp_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn should_terminate_batch(calls: &[FinalizedToolCall]) -> bool {
    !calls.is_empty() && calls.iter().all(|f| f.terminate)
}

fn is_cancelled(signal: Option<&CancellationToken>) -> bool {
    signal.is_some_and(CancellationToken::is_cancelled)
}
