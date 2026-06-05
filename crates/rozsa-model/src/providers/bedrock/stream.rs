//! Bedrock ConverseStream event → unified StreamEvent mapping.

use aws_sdk_bedrockruntime::types::{
    ContentBlockDelta, ContentBlockStart, ConverseStreamOutput,
    ReasoningContentBlockDelta, StopReason as BedrockStopReason,
};

use crate::event_stream::EventStreamSender;
use crate::providers::common::calculate_cost;
use crate::types::{
    AssistantMessage, ContentBlock, Model, StopReason, StreamEvent, ToolCall, Usage, UsageCost,
};

enum BlockState {
    Text {
        content_index: usize,
        text: String,
    },
    Thinking {
        content_index: usize,
        text: String,
        signature: String,
    },
    ToolCall {
        content_index: usize,
        id: String,
        name: String,
        partial_json: String,
    },
}

pub async fn consume_stream(
    mut stream: aws_sdk_bedrockruntime::primitives::event_stream::EventReceiver<
        ConverseStreamOutput,
        aws_sdk_bedrockruntime::types::error::ConverseStreamOutputError,
    >,
    model: &Model,
    mut output: AssistantMessage,
    sender: &EventStreamSender<StreamEvent>,
) {
    let mut blocks: Vec<BlockState> = Vec::new();

    loop {
        match stream.recv().await {
            Ok(Some(event)) => {
                handle_event(event, &mut blocks, model, &mut output, sender);
            }
            Ok(None) => break,
            Err(err) => {
                output.stop_reason = StopReason::Error;
                output.error_message = Some(format!("{err}"));
                sender.push(StreamEvent::Error {
                    reason: StopReason::Error,
                    error: output,
                });
                return;
            }
        }
    }

    sender.push(StreamEvent::Done {
        reason: output.stop_reason,
        message: output,
    });
}

fn handle_event(
    event: ConverseStreamOutput,
    blocks: &mut Vec<BlockState>,
    model: &Model,
    output: &mut AssistantMessage,
    sender: &EventStreamSender<StreamEvent>,
) {
    match event {
        ConverseStreamOutput::MessageStart(_) => {
            sender.push(StreamEvent::Start {
                partial: output.clone(),
            });
        }
        ConverseStreamOutput::ContentBlockStart(event) => {
            handle_content_block_start(event.start(), blocks, output, sender);
        }
        ConverseStreamOutput::ContentBlockDelta(event) => {
            let block_index = event.content_block_index();
            if let Some(delta) = event.delta() {
                handle_content_block_delta(delta, block_index, blocks, output, sender);
            }
        }
        ConverseStreamOutput::ContentBlockStop(event) => {
            let block_index = event.content_block_index();
            handle_content_block_stop(block_index, blocks, output, sender);
        }
        ConverseStreamOutput::MessageStop(event) => {
            output.stop_reason = map_stop_reason(event.stop_reason());
        }
        ConverseStreamOutput::Metadata(event) => {
            if let Some(usage) = event.usage() {
                output.usage = Usage {
                    input: usage.input_tokens() as u64,
                    output: usage.output_tokens() as u64,
                    cache_read: usage.cache_read_input_tokens().unwrap_or(0) as u64,
                    cache_write: usage.cache_write_input_tokens().unwrap_or(0) as u64,
                    total_tokens: usage.total_tokens() as u64,
                    cost: UsageCost {
                        input: 0.0,
                        output: 0.0,
                        cache_read: 0.0,
                        cache_write: 0.0,
                        total: 0.0,
                    },
                };
                calculate_cost(model, &mut output.usage);
            }
        }
        _ => {}
    }
}

fn handle_content_block_start(
    start: Option<&ContentBlockStart>,
    blocks: &mut Vec<BlockState>,
    output: &mut AssistantMessage,
    sender: &EventStreamSender<StreamEvent>,
) {
    let Some(start) = start else { return };

    match start {
        ContentBlockStart::ToolUse(tool_use) => {
            let content_index = output.content.len();
            output.content.push(ContentBlock::ToolCall(ToolCall {
                id: tool_use.tool_use_id().to_string(),
                name: tool_use.name().to_string(),
                arguments: serde_json::Value::Object(Default::default()),
            }));
            blocks.push(BlockState::ToolCall {
                content_index,
                id: tool_use.tool_use_id().to_string(),
                name: tool_use.name().to_string(),
                partial_json: String::new(),
            });
            sender.push(StreamEvent::ToolCallStart {
                content_index,
                partial: output.clone(),
            });
        }
        _ => {}
    }
}

fn handle_content_block_delta(
    delta: &ContentBlockDelta,
    block_index: i32,
    blocks: &mut Vec<BlockState>,
    output: &mut AssistantMessage,
    sender: &EventStreamSender<StreamEvent>,
) {
    match delta {
        ContentBlockDelta::Text(text) => {
            let existing = blocks.iter().find(|b| matches!(b, BlockState::Text { .. }));
            if existing.is_none() {
                let content_index = output.content.len();
                output.content.push(ContentBlock::Text {
                    text: String::new(),
                    signature: None,
                });
                blocks.push(BlockState::Text {
                    content_index,
                    text: String::new(),
                });
                sender.push(StreamEvent::TextStart {
                    content_index,
                    partial: output.clone(),
                });
            }
            if let Some(BlockState::Text {
                content_index,
                text: accumulated,
            }) = blocks
                .iter_mut()
                .rev()
                .find(|b| matches!(b, BlockState::Text { .. }))
            {
                accumulated.push_str(text);
                if let Some(ContentBlock::Text {
                    text: content_text, ..
                }) = output.content.get_mut(*content_index)
                {
                    content_text.push_str(text);
                }
                sender.push(StreamEvent::TextDelta {
                    content_index: *content_index,
                    delta: text.clone(),
                    partial: output.clone(),
                });
            }
        }
        ContentBlockDelta::ReasoningContent(reasoning) => {
            handle_reasoning_delta(reasoning, block_index, blocks, output, sender);
        }
        ContentBlockDelta::ToolUse(tool_use_delta) => {
            let input = tool_use_delta.input();
            if let Some(BlockState::ToolCall {
                content_index,
                partial_json,
                ..
            }) = blocks
                .iter_mut()
                .rev()
                .find(|b| matches!(b, BlockState::ToolCall { .. }))
            {
                partial_json.push_str(input);
                sender.push(StreamEvent::ToolCallDelta {
                    content_index: *content_index,
                    delta: input.to_string(),
                    partial: output.clone(),
                });
            }
        }
        _ => {}
    }
}

fn handle_reasoning_delta(
    reasoning: &ReasoningContentBlockDelta,
    _block_index: i32,
    blocks: &mut Vec<BlockState>,
    output: &mut AssistantMessage,
    sender: &EventStreamSender<StreamEvent>,
) {
    match reasoning {
        ReasoningContentBlockDelta::Text(text) => {
            let existing = blocks
                .iter()
                .find(|b| matches!(b, BlockState::Thinking { .. }));
            if existing.is_none() {
                let content_index = output.content.len();
                output.content.push(ContentBlock::Thinking {
                    thinking: String::new(),
                    signature: None,
                    redacted: false,
                });
                blocks.push(BlockState::Thinking {
                    content_index,
                    text: String::new(),
                    signature: String::new(),
                });
                sender.push(StreamEvent::ThinkingStart {
                    content_index,
                    partial: output.clone(),
                });
            }
            if let Some(BlockState::Thinking {
                content_index,
                text: accumulated,
                ..
            }) = blocks
                .iter_mut()
                .rev()
                .find(|b| matches!(b, BlockState::Thinking { .. }))
            {
                accumulated.push_str(text);
                if let Some(ContentBlock::Thinking {
                    thinking: content_thinking,
                    ..
                }) = output.content.get_mut(*content_index)
                {
                    content_thinking.push_str(text);
                }
                sender.push(StreamEvent::ThinkingDelta {
                    content_index: *content_index,
                    delta: text.clone(),
                    partial: output.clone(),
                });
            }
        }
        ReasoningContentBlockDelta::Signature(sig) => {
            if let Some(BlockState::Thinking { signature, .. }) = blocks
                .iter_mut()
                .rev()
                .find(|b| matches!(b, BlockState::Thinking { .. }))
            {
                signature.push_str(sig);
            }
        }
        ReasoningContentBlockDelta::RedactedContent(_) => {
            let existing = blocks
                .iter()
                .find(|b| matches!(b, BlockState::Thinking { .. }));
            if existing.is_none() {
                let content_index = output.content.len();
                output.content.push(ContentBlock::Thinking {
                    thinking: String::new(),
                    signature: None,
                    redacted: true,
                });
                blocks.push(BlockState::Thinking {
                    content_index,
                    text: String::new(),
                    signature: String::new(),
                });
                sender.push(StreamEvent::ThinkingStart {
                    content_index,
                    partial: output.clone(),
                });
            }
        }
        _ => {}
    }
}

fn handle_content_block_stop(
    _block_index: i32,
    blocks: &mut Vec<BlockState>,
    output: &mut AssistantMessage,
    sender: &EventStreamSender<StreamEvent>,
) {
    let Some(block) = blocks.pop() else { return };

    match block {
        BlockState::Text {
            content_index,
            text,
        } => {
            sender.push(StreamEvent::TextEnd {
                content_index,
                content: text,
                partial: output.clone(),
            });
        }
        BlockState::Thinking {
            content_index,
            text,
            signature,
        } => {
            if let Some(ContentBlock::Thinking {
                signature: sig_field,
                ..
            }) = output.content.get_mut(content_index)
            {
                if !signature.is_empty() {
                    *sig_field = Some(signature);
                }
            }
            sender.push(StreamEvent::ThinkingEnd {
                content_index,
                content: text,
                partial: output.clone(),
            });
        }
        BlockState::ToolCall {
            content_index,
            id,
            name,
            partial_json,
        } => {
            let arguments: serde_json::Value =
                serde_json::from_str(&partial_json).unwrap_or_default();
            let tool_call = ToolCall {
                id,
                name,
                arguments: arguments.clone(),
            };
            if let Some(ContentBlock::ToolCall(tc)) = output.content.get_mut(content_index) {
                tc.arguments = arguments;
            }
            sender.push(StreamEvent::ToolCallEnd {
                content_index,
                tool_call,
                partial: output.clone(),
            });
        }
    }
}

fn map_stop_reason(reason: &BedrockStopReason) -> StopReason {
    match reason {
        BedrockStopReason::EndTurn | BedrockStopReason::StopSequence => StopReason::Stop,
        BedrockStopReason::MaxTokens | BedrockStopReason::ModelContextWindowExceeded => {
            StopReason::Length
        }
        BedrockStopReason::ToolUse => StopReason::ToolUse,
        _ => StopReason::Error,
    }
}
