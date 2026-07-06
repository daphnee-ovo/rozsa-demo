// SSE parser for OpenAI Responses API (/v1/responses streaming).
//
// Internal Framework:
// sse.rs
// ├── ResponsesSseParser          — incremental SSE frame parser
// │   ├── feed()                  — accept text fragment, return parsed events
// │   └── finish()                — flush remaining buffer at stream end
// └── map_raw_to_event()          — convert RawResponsesStreamEvent → ResponseEvent
//
// Reference:
// - codex-rs codex-api/src/sse/responses.rs (event type mapping)
// - crates/rozsa-model/src/providers/openai_completions/sse.rs (line-buffer pattern)

use serde::Deserialize;

use super::types::{RawResponsesStreamEvent, ResponseEvent, ResponseItem, TokenUsage};
use crate::providers::common::ProviderError;

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// Incremental SSE parser for the OpenAI Responses API stream.
///
/// Buffers incoming text, splits on double-newline frame boundaries, extracts
/// `event:` and `data:` fields from each frame, deserializes JSON into
/// `RawResponsesStreamEvent`, then maps to high-level `ResponseEvent`.
pub struct ResponsesSseParser {
    /// Accumulated text not yet consumed as complete lines.
    line_buffer: String,
    /// Current frame's `event:` field value.
    current_event_type: Option<String>,
    /// Accumulated `data:` lines for the current frame.
    current_data: Vec<String>,
}

impl ResponsesSseParser {
    /// Create a new parser with empty buffers.
    pub fn new() -> Self {
        Self {
            line_buffer: String::new(),
            current_event_type: None,
            current_data: Vec::new(),
        }
    }

    /// Feed a UTF-8 text fragment from the network stream.
    ///
    /// Returns all `ResponseEvent`s that could be fully parsed from complete
    /// SSE frames. May return an empty vec if the input ends mid-frame.
    pub fn feed(&mut self, text: &str) -> Result<Vec<ResponseEvent>, ProviderError> {
        let mut events = Vec::new();
        self.line_buffer.push_str(text);

        while let Some(line_end) = self.line_buffer.find('\n') {
            let line = self.line_buffer[..line_end]
                .trim_end_matches('\r')
                .to_string();
            self.line_buffer.drain(..=line_end);
            self.parse_line(&line, &mut events)?;
        }

        Ok(events)
    }

    /// Flush any remaining buffered data at end of stream.
    pub fn finish(&mut self) -> Result<Vec<ResponseEvent>, ProviderError> {
        let mut events = Vec::new();

        if !self.line_buffer.is_empty() {
            let line = std::mem::take(&mut self.line_buffer);
            self.parse_line(line.trim_end_matches('\r'), &mut events)?;
        }
        self.flush_frame(&mut events)?;

        Ok(events)
    }

    /// Process a single complete line within the SSE stream.
    fn parse_line(
        &mut self,
        line: &str,
        events: &mut Vec<ResponseEvent>,
    ) -> Result<(), ProviderError> {
        // Empty line = frame boundary (double newline separator).
        if line.is_empty() {
            self.flush_frame(events)?;
            return Ok(());
        }

        // SSE comment lines start with ':' — skip.
        if line.starts_with(':') {
            return Ok(());
        }

        if let Some(event_type) = line.strip_prefix("event:") {
            self.current_event_type = Some(event_type.trim_start().to_string());
        } else if let Some(data) = line.strip_prefix("data:") {
            self.current_data.push(data.trim_start().to_string());
        }
        // Ignore other field names (id:, retry:, etc.)

        Ok(())
    }

    /// Attempt to parse the accumulated frame into a ResponseEvent.
    fn flush_frame(&mut self, events: &mut Vec<ResponseEvent>) -> Result<(), ProviderError> {
        if self.current_data.is_empty() {
            self.current_event_type = None;
            return Ok(());
        }

        let data = self.current_data.join("\n");
        self.current_data.clear();
        self.current_event_type = None;

        // The Responses API sends [DONE] as the terminal frame.
        if data == "[DONE]" {
            return Ok(());
        }

        let raw = serde_json::from_str::<RawResponsesStreamEvent>(&data)
            .map_err(|e| ProviderError::Parse(format!("invalid Responses SSE payload: {e}")))?;

        if let Some(event) = map_raw_to_event(raw)? {
            events.push(event);
        }

        Ok(())
    }
}

impl Default for ResponsesSseParser {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Event mapping
// ---------------------------------------------------------------------------

/// Map a deserialized raw stream event to a typed `ResponseEvent`.
///
/// Returns `Ok(None)` for unrecognized event types (skipped silently).
fn map_raw_to_event(raw: RawResponsesStreamEvent) -> Result<Option<ResponseEvent>, ProviderError> {
    match raw.kind.as_str() {
        "response.created" => Ok(Some(ResponseEvent::Created)),

        "response.output_item.added" => {
            let item = parse_response_item(raw.item, "output_item.added")?;
            Ok(item.map(|i| ResponseEvent::OutputItemAdded { item: i }))
        }

        "response.output_item.done" => {
            let item = parse_response_item(raw.item, "output_item.done")?;
            Ok(item.map(|i| ResponseEvent::OutputItemDone { item: i }))
        }

        "response.output_text.delta" => Ok(Some(ResponseEvent::OutputTextDelta {
            delta: raw.delta.unwrap_or_default(),
        })),

        "response.function_call_arguments.delta" => {
            Ok(Some(ResponseEvent::FunctionCallArgsDelta {
                item_id: raw.item_id,
                call_id: raw.call_id,
                delta: raw.delta.unwrap_or_default(),
            }))
        }

        "response.reasoning_summary_text.delta" => Ok(Some(ResponseEvent::ReasoningSummaryDelta {
            delta: raw.delta.unwrap_or_default(),
            summary_index: raw.summary_index,
        })),

        "response.reasoning_text.delta" => Ok(Some(ResponseEvent::ReasoningContentDelta {
            delta: raw.delta.unwrap_or_default(),
            content_index: raw.content_index,
        })),

        "response.completed" => parse_completed(raw.response),

        "response.failed" => parse_failed(raw.response),

        "response.incomplete" => {
            let reason = raw.response.as_ref().and_then(|resp| {
                resp.get("incomplete_details")
                    .and_then(|d| d.get("reason"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            });
            Ok(Some(ResponseEvent::Incomplete { reason }))
        }

        // 未知事件类型静默跳过
        _ => Ok(None),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse a ResponseItem from an optional JSON Value.
fn parse_response_item(
    item: Option<serde_json::Value>,
    context: &str,
) -> Result<Option<ResponseItem>, ProviderError> {
    match item {
        Some(val) => {
            let parsed = serde_json::from_value::<ResponseItem>(val).map_err(|e| {
                ProviderError::Parse(format!("failed to parse ResponseItem from {context}: {e}"))
            })?;
            Ok(Some(parsed))
        }
        None => Ok(None),
    }
}

/// Parse response.completed → ResponseEvent::Completed with token usage.
fn parse_completed(
    response: Option<serde_json::Value>,
) -> Result<Option<ResponseEvent>, ProviderError> {
    let resp = match response {
        Some(v) => v,
        None => {
            return Ok(Some(ResponseEvent::Completed {
                response_id: String::new(),
                usage: None,
            }));
        }
    };

    let response_id = resp
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let usage = resp.get("usage").and_then(|u| parse_token_usage(u));

    Ok(Some(ResponseEvent::Completed { response_id, usage }))
}

/// Extract TokenUsage from a usage JSON object.
fn parse_token_usage(usage_val: &serde_json::Value) -> Option<TokenUsage> {
    #[derive(Deserialize)]
    struct RawUsage {
        #[serde(default)]
        input_tokens: u64,
        #[serde(default)]
        output_tokens: u64,
        #[serde(default)]
        total_tokens: u64,
        #[serde(default)]
        input_tokens_details: Option<InputTokenDetails>,
        #[serde(default)]
        output_tokens_details: Option<OutputTokenDetails>,
    }

    #[derive(Deserialize)]
    struct InputTokenDetails {
        #[serde(default)]
        cached_tokens: Option<u64>,
    }

    #[derive(Deserialize)]
    struct OutputTokenDetails {
        #[serde(default)]
        reasoning_tokens: Option<u64>,
    }

    let raw: RawUsage = serde_json::from_value(usage_val.clone()).ok()?;

    Some(TokenUsage {
        input_tokens: raw.input_tokens,
        output_tokens: raw.output_tokens,
        total_tokens: raw.total_tokens,
        cached_tokens: raw.input_tokens_details.and_then(|d| d.cached_tokens),
        reasoning_tokens: raw.output_tokens_details.and_then(|d| d.reasoning_tokens),
    })
}

/// Parse response.failed → ResponseEvent::Failed with error code and message.
fn parse_failed(
    response: Option<serde_json::Value>,
) -> Result<Option<ResponseEvent>, ProviderError> {
    let resp = match response {
        Some(v) => v,
        None => {
            return Ok(Some(ResponseEvent::Failed {
                error_code: "unknown".to_string(),
                error_message: "response.failed event with no response payload".to_string(),
            }));
        }
    };

    let error_obj = resp.get("error");

    let error_code = error_obj
        .and_then(|e| e.get("code"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let error_message = error_obj
        .and_then(|e| e.get("message"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown error")
        .to_string();

    Ok(Some(ResponseEvent::Failed {
        error_code,
        error_message,
    }))
}
