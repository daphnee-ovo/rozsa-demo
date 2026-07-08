use crate::config::{AgentContext, AgentLoopConfig, ShouldStopContext};
use crate::events::AgentEvent;
use crate::messages::AgentMessage;
use crate::tool::ToolExecutionMode;
use rozsa_model::event_stream::{EventStream, EventStreamSender, create_event_stream};
use rozsa_model::types::{
    AssistantMessage, ContentBlock, Context as ModelContext, Message, StopReason, StreamEvent,
    ThinkingLevel, ToolCall, ToolResultMessage,
};
use std::sync::Arc;
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
        tracing::warn!("agent_loop_continue called with empty context — nothing to continue");
        let (sender, stream) = create_event_stream();
        sender.push(AgentEvent::AgentStart);
        sender.push(AgentEvent::AgentEnd { messages: vec![] });
        return stream;
    }

    if let Some(last) = context.messages.last() {
        if let Some(Message::Assistant(_)) = last.as_standard() {
            tracing::warn!(
                "agent_loop_continue called with assistant as last message — nothing to continue"
            );
            let (sender, stream) = create_event_stream();
            sender.push(AgentEvent::AgentStart);
            sender.push(AgentEvent::AgentEnd { messages: vec![] });
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
    let mut turn_count: u32 = 0;
    let mut auth_retried = false;

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

    let mut pending_messages: Vec<AgentMessage> = config
        .get_steering_messages
        .as_ref()
        .map_or_else(Vec::new, |f| f());

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
                emit.push(AgentEvent::TurnEnd {
                    message: aborted_assistant_message(),
                    tool_results: vec![],
                });
                emit.push(AgentEvent::AgentEnd {
                    messages: new_messages,
                });
                return;
            };

            new_messages.push(assistant_agent_message(message.clone()));

            // Auth retry: if 401/auth error and get_api_key hook exists, try refresh once
            if message.stop_reason == StopReason::Error && !auth_retried {
                if let Some(ref err_msg) = message.error_message {
                    let err_lower = err_msg.to_lowercase();
                    if err_lower.contains("401")
                        || err_lower.contains("unauthorized")
                        || err_lower.contains("authentication")
                    {
                        if let Some(ref refresh) = config.get_api_key {
                            auth_retried = true;
                            // Call the hook to refresh credentials (provider is extracted from model)
                            let provider = message.provider.as_str();
                            if let Some(_new_key) = refresh(provider) {
                                tracing::info!(
                                    "Auth error detected, refreshed credentials for provider '{}', retrying...",
                                    provider
                                );
                                // Remove the error message from context and new_messages
                                context.messages.pop();
                                new_messages.pop();
                                // Retry by continuing the loop
                                continue;
                            } else {
                                tracing::warn!(
                                    "Auth error detected but get_api_key returned None for provider '{}'",
                                    provider
                                );
                            }
                        }
                    }
                }
            }

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

            let mut tool_calls: Vec<ToolCall> = message
                .content
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::ToolCall(tc) => Some(tc.clone()),
                    _ => None,
                })
                .collect();

            // When model hit max_tokens (Length), the last tool call may have
            // truncated arguments. Drop any tool call whose arguments are not
            // a valid JSON object (strings indicate unparsed/truncated JSON).
            if message.stop_reason == StopReason::Length && !tool_calls.is_empty() {
                let last_idx = tool_calls.len() - 1;
                if let Some(last) = tool_calls.get(last_idx) {
                    if !last.arguments.is_null() && !last.arguments.is_object() {
                        tool_calls.pop();
                    }
                }
            }

            let mut tool_results: Vec<ToolResultMessage> = Vec::new();
            has_more_tool_calls = false;

            if !tool_calls.is_empty() {
                let batch = execute_tool_batch(
                    &tool_calls,
                    &message,
                    &context,
                    &config,
                    &emit,
                    signal.as_ref(),
                )
                .await;
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

            if let Some(ref prepare_next) = config.prepare_next_turn {
                let stop_ctx = ShouldStopContext {
                    message: &message,
                    tool_results: &tool_results,
                    context: &context,
                    new_messages: &new_messages,
                };
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
                let stop_ctx = ShouldStopContext {
                    message: &message,
                    tool_results: &tool_results,
                    context: &context,
                    new_messages: &new_messages,
                };
                if should_stop(&stop_ctx) {
                    emit.push(AgentEvent::AgentEnd {
                        messages: new_messages,
                    });
                    return;
                }
            }

            turn_count += 1;
            if let Some(max) = config.max_turns {
                if turn_count >= max {
                    emit.push(AgentEvent::AgentEnd {
                        messages: new_messages,
                    });
                    return;
                }
            }

            pending_messages = config
                .get_steering_messages
                .as_ref()
                .map_or_else(Vec::new, |f| f());
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

    loop {
        let event = match signal {
            Some(token) => {
                tokio::select! {
                    _ = token.cancelled() => {
                        if added_partial {
                            context.messages.pop();
                        }
                        return None;
                    }
                    event = stream.next() => event,
                }
            }
            None => stream.next().await,
        };

        let Some(event) = event else {
            break;
        };

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

    // Stream ended without Done/Error — remove partial message from context if one was added
    if added_partial {
        context.messages.pop();
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

fn aborted_assistant_message() -> AssistantMessage {
    AssistantMessage {
        content: vec![],
        api: rozsa_model::types::Api::AnthropicMessages,
        provider: rozsa_model::types::Provider::Anthropic,
        model: String::new(),
        response_model: None,
        response_id: None,
        usage: rozsa_model::types::Usage::default(),
        stop_reason: StopReason::Aborted,
        error_message: None,
        timestamp: 0,
    }
}

struct ToolBatchResult {
    messages: Vec<ToolResultMessage>,
    terminate: bool,
}

struct FinalizedToolCall {
    tool_call: ToolCall,
    content: Vec<ContentBlock>,
    details: serde_json::Value,
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

        let finalized =
            execute_single_tool(call, assistant_message, context, config, emit, signal).await;

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
    ToolBatchResult {
        messages,
        terminate,
    }
}

enum PreparedEntry {
    Immediate(FinalizedToolCall),
    Pending {
        call: ToolCall,
        handle: tokio::task::JoinHandle<(
            ToolCall,
            Result<crate::tool::ToolResult, crate::tool::ToolError>,
        )>,
    },
}

async fn execute_parallel(
    tool_calls: &[ToolCall],
    assistant_message: &AssistantMessage,
    context: &AgentContext,
    config: &AgentLoopConfig,
    emit: &EventStreamSender<AgentEvent>,
    signal: Option<&CancellationToken>,
) -> ToolBatchResult {
    let semaphore = Arc::new(tokio::sync::Semaphore::new(10));
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
                signal: signal.cloned(),
            };
            if let Some(result) = before(ctx).await {
                if result.block {
                    let reason = result
                        .reason
                        .unwrap_or_else(|| "Tool execution was blocked".to_string());
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
        let emit_clone = emit.clone();
        let schema = tool.parameters_schema().clone();
        let semaphore_clone = semaphore.clone();
        let handle = tokio::spawn(async move {
            let _permit = semaphore_clone
                .acquire_owned()
                .await
                .expect("semaphore closed");

            let args = if call_clone.arguments.is_null() {
                serde_json::Value::Object(Default::default())
            } else {
                let args = tool_clone.prepare_arguments(call_clone.arguments.clone());
                crate::coerce::coerce_arguments(&schema, args)
            };

            let tool_call_id = call_clone.id.clone();
            let tool_name = call_clone.name.clone();
            let emit_for_update = emit_clone.clone();
            let on_update = move |partial: crate::tool::ToolResult| {
                emit_for_update.push(AgentEvent::ToolExecutionUpdate {
                    tool_call_id: tool_call_id.clone(),
                    tool_name: tool_name.clone(),
                    partial_result: partial,
                });
            };

            let exec_result = tool_clone
                .execute(&call_clone.id, args, signal_clone, Some(&on_update))
                .await;
            (call_clone, exec_result)
        });
        entries.push(PreparedEntry::Pending {
            call: call.clone(),
            handle,
        });

        if is_cancelled(signal) {
            break;
        }
    }

    // Await in source order (like TS Promise.all) — emit tool_execution_end per tool
    let mut finalized_calls: Vec<FinalizedToolCall> = Vec::new();
    let mut entries_iter = entries.into_iter();
    let mut remaining_entries: Vec<PreparedEntry> = Vec::new();

    for entry in entries_iter.by_ref() {
        // If cancelled, collect remaining entries and abort their handles
        if is_cancelled(signal) {
            remaining_entries.push(entry);
            remaining_entries.extend(entries_iter);
            break;
        }

        let finalized = match entry {
            PreparedEntry::Immediate(f) => f,
            PreparedEntry::Pending { call, handle } => match handle.await {
                Ok((call, exec_result)) => {
                    let (content, details, is_error, terminate) = match exec_result {
                        Ok(result) => (result.content, result.details, false, result.terminate),
                        Err(err) => (
                            vec![ContentBlock::Text {
                                text: format!("{}", err),
                                signature: None,
                            }],
                            serde_json::Value::Null,
                            true,
                            false,
                        ),
                    };

                    let mut f = FinalizedToolCall {
                        tool_call: call.clone(),
                        content,
                        details,
                        is_error,
                        terminate,
                    };

                    if let Some(ref after) = config.post_tool_use {
                        apply_post_tool_use(&mut f, after.as_ref(), assistant_message, context);
                    }

                    emit.push(AgentEvent::ToolExecutionEnd {
                        tool_call_id: f.tool_call.id.clone(),
                        tool_name: f.tool_call.name.clone(),
                        result: finalized_to_result_message(&f),
                    });

                    f
                }
                Err(join_error) => {
                    let error_msg = format!("Tool execution panicked: {}", join_error);
                    let f = FinalizedToolCall {
                        tool_call: call.clone(),
                        content: vec![ContentBlock::Text {
                            text: error_msg,
                            signature: None,
                        }],
                        details: serde_json::Value::Null,
                        is_error: true,
                        terminate: false,
                    };
                    emit.push(AgentEvent::ToolExecutionEnd {
                        tool_call_id: call.id.clone(),
                        tool_name: call.name.clone(),
                        result: finalized_to_result_message(&f),
                    });
                    f
                }
            },
        };
        finalized_calls.push(finalized);
    }

    // Abort remaining pending handles if cancellation occurred
    for entry in remaining_entries {
        match entry {
            PreparedEntry::Immediate(f) => {
                finalized_calls.push(f);
            }
            PreparedEntry::Pending { call, handle } => {
                handle.abort();
                let f = FinalizedToolCall {
                    tool_call: call.clone(),
                    content: vec![ContentBlock::Text {
                        text: "Tool execution was cancelled".to_string(),
                        signature: None,
                    }],
                    details: serde_json::Value::Null,
                    is_error: true,
                    terminate: false,
                };
                emit.push(AgentEvent::ToolExecutionEnd {
                    tool_call_id: call.id.clone(),
                    tool_name: call.name.clone(),
                    result: finalized_to_result_message(&f),
                });
                finalized_calls.push(f);
            }
        }
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

    ToolBatchResult {
        messages,
        terminate,
    }
}

async fn execute_single_tool(
    call: &ToolCall,
    assistant_message: &AssistantMessage,
    context: &AgentContext,
    config: &AgentLoopConfig,
    emit: &EventStreamSender<AgentEvent>,
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
            signal: signal.cloned(),
        };
        if let Some(result) = before(ctx).await {
            if result.block {
                let reason = result
                    .reason
                    .unwrap_or_else(|| "Tool execution was blocked".to_string());
                return make_error_finalized(call, reason);
            }
        }
    }

    let args = if call.arguments.is_null() {
        serde_json::Value::Object(Default::default())
    } else {
        let args = tool.prepare_arguments(call.arguments.clone());
        crate::coerce::coerce_arguments(tool.parameters_schema(), args)
    };

    let emit_clone = emit.clone();
    let tool_call_id = call.id.clone();
    let tool_name = call.name.clone();
    let on_update = move |partial: crate::tool::ToolResult| {
        emit_clone.push(AgentEvent::ToolExecutionUpdate {
            tool_call_id: tool_call_id.clone(),
            tool_name: tool_name.clone(),
            partial_result: partial,
        });
    };

    let exec_result = tool
        .execute(&call.id, args, signal.cloned(), Some(&on_update))
        .await;

    let (content, details, is_error, terminate) = match exec_result {
        Ok(result) => (result.content, result.details, false, result.terminate),
        Err(err) => (
            vec![ContentBlock::Text {
                text: format!("{}", err),
                signature: None,
            }],
            serde_json::Value::Null,
            true,
            false,
        ),
    };

    let mut finalized = FinalizedToolCall {
        tool_call: call.clone(),
        content,
        details,
        is_error,
        terminate,
    };

    if let Some(ref after) = config.post_tool_use {
        apply_post_tool_use(&mut finalized, after.as_ref(), assistant_message, context);
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
        details: serde_json::Value::Null,
        is_error: true,
        terminate: false,
    }
}

fn finalized_to_result_message(f: &FinalizedToolCall) -> ToolResultMessage {
    ToolResultMessage {
        tool_call_id: f.tool_call.id.clone(),
        tool_name: f.tool_call.name.clone(),
        content: f.content.clone(),
        details: f.details.clone(),
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

fn apply_post_tool_use(
    f: &mut FinalizedToolCall,
    after: &dyn Fn(&crate::config::PostToolUseContext) -> Option<crate::config::PostToolUseResult>,
    assistant_message: &AssistantMessage,
    context: &AgentContext,
) {
    let ctx = crate::config::PostToolUseContext {
        assistant_message: assistant_message.clone(),
        tool_call_id: f.tool_call.id.clone(),
        tool_name: f.tool_call.name.clone(),
        args: f.tool_call.arguments.clone(),
        result: crate::tool::ToolResult {
            content: f.content.clone(),
            details: f.details.clone(),
            terminate: f.terminate,
        },
        is_error: f.is_error,
        context: context.clone(),
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| after(&ctx)));
    match result {
        Ok(Some(override_result)) => {
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
        Ok(None) => {}
        Err(_) => {
            tracing::error!(
                "post_tool_use hook panicked for tool '{}'",
                f.tool_call.name
            );
        }
    }
}
