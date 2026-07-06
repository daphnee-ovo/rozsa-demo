//! Anthropic Messages SSE stream parser.
//!
//! Internal Framework:
//! stream.rs
//! ├── consume_anthropic_stream()  — reqwest Response → StreamEvent sequence
//! ├── AnthropicSseParser         — line-based SSE decoder
//! └── map_stop_reason()          — Anthropic stop_reason → StopReason
//!
//! Related Docs:
//! - [Migration Plan](../../../../docs/model/rozsa-model-migration.md)

use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::Value;

use crate::event_stream::EventStreamSender;
use crate::providers::common::{ProviderError, ProviderResult, calculate_cost};
use crate::types::{
    AssistantMessage, ContentBlock, Model, StopReason, StreamEvent, ToolCall, ToolSchema,
};

use super::payload::{from_claude_code_name, normalize_tool_call_id};

/// Consume an Anthropic SSE stream and emit normalized StreamEvents.
pub async fn consume_anthropic_stream(
    response: reqwest::Response,
    model: &Model,
    output: &mut AssistantMessage,
    sender: &EventStreamSender<StreamEvent>,
    is_oauth: bool,
    tools: &[ToolSchema],
) -> ProviderResult<()> {
    let mut parser = AnthropicSseParser::new();
    let mut stream = response.bytes_stream();
    let mut saw_message_start = false;
    let mut saw_message_stop = false;

    // Tracks the provider-side content_block index → our content vec index
    let mut block_index_map: Vec<usize> = Vec::new();
    // Partial JSON accumulators for tool call arguments
    let mut tool_json_bufs: Vec<String> = Vec::new();

    sender.push(StreamEvent::Start {
        partial: output.clone(),
    });

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(ProviderError::Http)?;
        let text = String::from_utf8_lossy(&chunk);

        for event in parser.feed(&text)? {
            match event.event_type.as_str() {
                "error" => {
                    return Err(ProviderError::Parse(format!(
                        "Anthropic stream error: {}",
                        event.data
                    )));
                }
                "message_start" => {
                    saw_message_start = true;
                    let data: MessageStartData = parse_event_data(&event.data)?;
                    output.response_id = data.message.id;
                    if let Some(usage) = data.message.usage {
                        output.usage.input = usage.input_tokens.unwrap_or(0);
                        output.usage.output = usage.output_tokens.unwrap_or(0);
                        output.usage.cache_read = usage.cache_read_input_tokens.unwrap_or(0);
                        output.usage.cache_write = usage.cache_creation_input_tokens.unwrap_or(0);
                        output.usage.total_tokens = output.usage.input
                            + output.usage.output
                            + output.usage.cache_read
                            + output.usage.cache_write;
                        calculate_cost(model, &mut output.usage);
                    }
                }
                "content_block_start" => {
                    let data: ContentBlockStartData = parse_event_data(&event.data)?;
                    let our_index = output.content.len();
                    // Ensure block_index_map covers provider index
                    while block_index_map.len() <= data.index {
                        block_index_map.push(0);
                    }
                    block_index_map[data.index] = our_index;

                    match data.content_block.block_type.as_str() {
                        "text" => {
                            output.content.push(ContentBlock::Text {
                                text: String::new(),
                                signature: None,
                            });
                            tool_json_bufs.push(String::new());
                            sender.push(StreamEvent::TextStart {
                                content_index: our_index,
                                partial: output.clone(),
                            });
                        }
                        "thinking" => {
                            output.content.push(ContentBlock::Thinking {
                                thinking: String::new(),
                                signature: Some(String::new()),
                                redacted: false,
                            });
                            tool_json_bufs.push(String::new());
                            sender.push(StreamEvent::ThinkingStart {
                                content_index: our_index,
                                partial: output.clone(),
                            });
                        }
                        "redacted_thinking" => {
                            let sig = data.content_block.data.unwrap_or_default();
                            output.content.push(ContentBlock::Thinking {
                                thinking: "[Reasoning redacted]".to_string(),
                                signature: Some(sig),
                                redacted: true,
                            });
                            tool_json_bufs.push(String::new());
                            sender.push(StreamEvent::ThinkingStart {
                                content_index: our_index,
                                partial: output.clone(),
                            });
                        }
                        "tool_use" => {
                            let id =
                                normalize_tool_call_id(&data.content_block.id.unwrap_or_default());
                            let raw_name = data.content_block.name.unwrap_or_default();
                            let name = if is_oauth {
                                from_claude_code_name(&raw_name, tools)
                            } else {
                                raw_name
                            };
                            output.content.push(ContentBlock::ToolCall(ToolCall {
                                id,
                                name,
                                arguments: Value::Object(Default::default()),
                            }));
                            tool_json_bufs.push(String::new());
                            sender.push(StreamEvent::ToolCallStart {
                                content_index: our_index,
                                partial: output.clone(),
                            });
                        }
                        _ => {
                            tool_json_bufs.push(String::new());
                        }
                    }
                }
                "content_block_delta" => {
                    let data: ContentBlockDeltaData = parse_event_data(&event.data)?;
                    let our_index = block_index_map.get(data.index).copied().unwrap_or(0);

                    match data.delta.delta_type.as_str() {
                        "text_delta" => {
                            let delta_text = data.delta.text.unwrap_or_default();
                            if let Some(ContentBlock::Text { text, .. }) =
                                output.content.get_mut(our_index)
                            {
                                text.push_str(&delta_text);
                            }
                            sender.push(StreamEvent::TextDelta {
                                content_index: our_index,
                                delta: delta_text,
                                partial: output.clone(),
                            });
                        }
                        "thinking_delta" => {
                            let delta_text = data.delta.thinking.unwrap_or_default();
                            if let Some(ContentBlock::Thinking { thinking, .. }) =
                                output.content.get_mut(our_index)
                            {
                                thinking.push_str(&delta_text);
                            }
                            sender.push(StreamEvent::ThinkingDelta {
                                content_index: our_index,
                                delta: delta_text,
                                partial: output.clone(),
                            });
                        }
                        "input_json_delta" => {
                            let partial_json = data.delta.partial_json.unwrap_or_default();
                            if let Some(buf) = tool_json_bufs.get_mut(our_index) {
                                buf.push_str(&partial_json);
                                if let Some(ContentBlock::ToolCall(tc)) =
                                    output.content.get_mut(our_index)
                                {
                                    tc.arguments = parse_streaming_json(buf);
                                }
                            }
                            sender.push(StreamEvent::ToolCallDelta {
                                content_index: our_index,
                                delta: partial_json,
                                partial: output.clone(),
                            });
                        }
                        "signature_delta" => {
                            let sig_text = data.delta.signature.unwrap_or_default();
                            if let Some(ContentBlock::Thinking { signature, .. }) =
                                output.content.get_mut(our_index)
                            {
                                if let Some(s) = signature {
                                    s.push_str(&sig_text);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                "content_block_stop" => {
                    let data: ContentBlockStopData = parse_event_data(&event.data)?;
                    let our_index = block_index_map.get(data.index).copied().unwrap_or(0);

                    // Finalize tool call arguments before borrowing for match
                    let is_tool_call = matches!(
                        output.content.get(our_index),
                        Some(ContentBlock::ToolCall(_))
                    );
                    if is_tool_call {
                        if let Some(buf) = tool_json_bufs.get(our_index) {
                            if let Some(ContentBlock::ToolCall(tc_mut)) =
                                output.content.get_mut(our_index)
                            {
                                tc_mut.arguments = parse_streaming_json(buf);
                            }
                        }
                    }

                    match output.content.get(our_index) {
                        Some(ContentBlock::Text { text, .. }) => {
                            sender.push(StreamEvent::TextEnd {
                                content_index: our_index,
                                content: text.clone(),
                                partial: output.clone(),
                            });
                        }
                        Some(ContentBlock::Thinking { thinking, .. }) => {
                            sender.push(StreamEvent::ThinkingEnd {
                                content_index: our_index,
                                content: thinking.clone(),
                                partial: output.clone(),
                            });
                        }
                        Some(ContentBlock::ToolCall(tc)) => {
                            sender.push(StreamEvent::ToolCallEnd {
                                content_index: our_index,
                                tool_call: tc.clone(),
                                partial: output.clone(),
                            });
                        }
                        _ => {}
                    }
                }
                "message_delta" => {
                    let data: MessageDeltaData = parse_event_data(&event.data)?;
                    if let Some(reason) = data.delta.stop_reason {
                        output.stop_reason = map_stop_reason(&reason);
                    }
                    if let Some(usage) = data.usage {
                        if let Some(v) = usage.input_tokens {
                            output.usage.input = v;
                        }
                        if let Some(v) = usage.output_tokens {
                            output.usage.output = v;
                        }
                        if let Some(v) = usage.cache_read_input_tokens {
                            output.usage.cache_read = v;
                        }
                        if let Some(v) = usage.cache_creation_input_tokens {
                            output.usage.cache_write = v;
                        }
                        output.usage.total_tokens = output.usage.input
                            + output.usage.output
                            + output.usage.cache_read
                            + output.usage.cache_write;
                        calculate_cost(model, &mut output.usage);
                    }
                }
                "message_stop" => {
                    saw_message_stop = true;
                }
                _ => {}
            }
        }
    }

    // Flush remaining SSE buffer
    for event in parser.finish()? {
        if event.event_type == "message_stop" {
            saw_message_stop = true;
        }
    }

    if saw_message_start && !saw_message_stop {
        return Err(ProviderError::Parse(
            "Anthropic stream ended before message_stop".to_string(),
        ));
    }

    Ok(())
}

/// Map Anthropic stop_reason string to normalized StopReason.
pub fn map_stop_reason(reason: &str) -> StopReason {
    match reason {
        "end_turn" | "pause_turn" | "stop_sequence" => StopReason::Stop,
        "max_tokens" => StopReason::Length,
        "tool_use" => StopReason::ToolUse,
        "refusal" | "sensitive" => StopReason::Error,
        _ => StopReason::Error,
    }
}

// --- SSE Parser ---

struct SseEvent {
    event_type: String,
    data: String,
}

struct AnthropicSseParser {
    buffer: String,
    current_event: Option<String>,
    data_lines: Vec<String>,
}

impl AnthropicSseParser {
    fn new() -> Self {
        Self {
            buffer: String::new(),
            current_event: None,
            data_lines: Vec::new(),
        }
    }

    fn feed(&mut self, input: &str) -> ProviderResult<Vec<SseEvent>> {
        let mut events = Vec::new();
        self.buffer.push_str(input);

        while let Some(newline_pos) = self.buffer.find('\n') {
            let line = self.buffer[..newline_pos]
                .trim_end_matches('\r')
                .to_string();
            self.buffer.drain(..=newline_pos);
            self.process_line(&line, &mut events);
        }

        Ok(events)
    }

    fn finish(&mut self) -> ProviderResult<Vec<SseEvent>> {
        let mut events = Vec::new();
        if !self.buffer.is_empty() {
            let line = std::mem::take(&mut self.buffer);
            self.process_line(line.trim_end_matches('\r'), &mut events);
        }
        self.flush(&mut events);
        Ok(events)
    }

    fn process_line(&mut self, line: &str, events: &mut Vec<SseEvent>) {
        if line.is_empty() {
            self.flush(events);
            return;
        }
        if line.starts_with(':') {
            return;
        }
        if let Some(value) = line.strip_prefix("event:") {
            self.current_event = Some(value.trim_start().to_string());
        } else if let Some(value) = line.strip_prefix("data:") {
            self.data_lines.push(value.trim_start().to_string());
        }
    }

    fn flush(&mut self, events: &mut Vec<SseEvent>) {
        if self.current_event.is_none() && self.data_lines.is_empty() {
            return;
        }
        let event_type = self.current_event.take().unwrap_or_default();
        let data = self.data_lines.join("\n");
        self.data_lines.clear();
        events.push(SseEvent { event_type, data });
    }
}

// --- Deserialization types ---

#[derive(Deserialize)]
struct MessageStartData {
    message: MessageStartMessage,
}

#[derive(Deserialize)]
struct MessageStartMessage {
    id: Option<String>,
    usage: Option<RawUsage>,
}

#[derive(Deserialize)]
struct ContentBlockStartData {
    index: usize,
    content_block: ContentBlockInfo,
}

#[derive(Deserialize)]
struct ContentBlockInfo {
    #[serde(rename = "type")]
    block_type: String,
    id: Option<String>,
    name: Option<String>,
    data: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    input: Option<Value>,
}

#[derive(Deserialize)]
struct ContentBlockDeltaData {
    index: usize,
    delta: DeltaInfo,
}

#[derive(Deserialize)]
struct DeltaInfo {
    #[serde(rename = "type")]
    delta_type: String,
    text: Option<String>,
    thinking: Option<String>,
    partial_json: Option<String>,
    signature: Option<String>,
}

#[derive(Deserialize)]
struct ContentBlockStopData {
    index: usize,
}

#[derive(Deserialize)]
struct MessageDeltaData {
    delta: MessageDeltaInfo,
    usage: Option<RawUsage>,
}

#[derive(Deserialize)]
struct MessageDeltaInfo {
    stop_reason: Option<String>,
}

#[derive(Deserialize)]
struct RawUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
}

// --- Helpers ---

fn parse_event_data<T: for<'de> Deserialize<'de>>(data: &str) -> ProviderResult<T> {
    serde_json::from_str(data).map_err(|e| {
        ProviderError::Parse(format!("Failed to parse Anthropic event: {e}; data={data}"))
    })
}

/// Best-effort parse of partial JSON (tool call arguments accumulating incrementally).
fn parse_streaming_json(input: &str) -> Value {
    if input.is_empty() {
        return Value::Object(Default::default());
    }
    serde_json::from_str(input).unwrap_or_else(|_| {
        // Try with closing brace for partial objects
        let patched = format!("{input}}}");
        serde_json::from_str(&patched).unwrap_or(Value::Object(Default::default()))
    })
}
