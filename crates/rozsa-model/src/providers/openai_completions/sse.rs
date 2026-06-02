//! SSE parsing helpers for OpenAI-compatible Chat Completions streams.

use serde::Deserialize;

use crate::providers::common::{ProviderError, ProviderResult};

/// Minimal streamed chat completion chunk accepted by the parser.
#[derive(Debug, Clone, Deserialize)]
pub struct ChatChunk {
    pub id: Option<String>,
    pub model: Option<String>,
    #[serde(default)]
    pub choices: Vec<Choice>,
    pub usage: Option<RawUsage>,
}

/// First-choice delta and finish reason from a streamed chunk.
#[derive(Debug, Clone, Deserialize)]
pub struct Choice {
    #[serde(default)]
    pub delta: Delta,
    pub finish_reason: Option<String>,
    pub usage: Option<RawUsage>,
}

/// Delta payload emitted by OpenAI-compatible streaming endpoints.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Delta {
    pub content: Option<String>,
    pub reasoning_content: Option<String>,
    pub reasoning: Option<String>,
    pub reasoning_text: Option<String>,
    pub tool_calls: Option<Vec<ToolCallDelta>>,
}

/// Incremental tool call data keyed by provider stream index.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolCallDelta {
    pub index: Option<usize>,
    pub id: Option<String>,
    pub function: Option<ToolCallFunctionDelta>,
}

/// Incremental function tool call fields.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolCallFunctionDelta {
    pub name: Option<String>,
    pub arguments: Option<String>,
}

/// Usage fields returned by OpenAI-compatible stream chunks.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RawUsage {
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub prompt_cache_hit_tokens: Option<u64>,
    pub prompt_tokens_details: Option<PromptTokenDetails>,
}

/// Provider-specific prompt token detail fields.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PromptTokenDetails {
    pub cached_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
}

/// Parse raw SSE bytes into JSON chat chunks.
pub fn parse_sse_chunks(input: &str) -> ProviderResult<Vec<ChatChunk>> {
    let mut parser = SseParser::new();
    let mut chunks = parser.feed(input)?;
    chunks.extend(parser.finish()?);
    Ok(chunks)
}

/// Incremental parser for OpenAI-compatible `data:` SSE events.
pub struct SseParser {
    line_buffer: String,
    event_data: Vec<String>,
}

impl SseParser {
    /// Create an empty parser for one provider response stream.
    pub fn new() -> Self {
        Self {
            line_buffer: String::new(),
            event_data: Vec::new(),
        }
    }

    /// Feed one UTF-8 text fragment and return completed chat chunks.
    pub fn feed(&mut self, input: &str) -> ProviderResult<Vec<ChatChunk>> {
        let mut chunks = Vec::new();
        self.line_buffer.push_str(input);

        while let Some(line_end) = self.line_buffer.find('\n') {
            let line = self.line_buffer[..line_end]
                .trim_end_matches('\r')
                .to_string();
            self.line_buffer.drain(..=line_end);
            self.parse_line(&line, &mut chunks)?;
        }

        Ok(chunks)
    }

    /// Finish parsing at end of response and flush the last event.
    pub fn finish(&mut self) -> ProviderResult<Vec<ChatChunk>> {
        let mut chunks = Vec::new();
        if !self.line_buffer.is_empty() {
            let line = std::mem::take(&mut self.line_buffer);
            self.parse_line(line.trim_end_matches('\r'), &mut chunks)?;
        }
        flush_event(&mut self.event_data, &mut chunks)?;
        Ok(chunks)
    }

    /// Parse one complete SSE line into accumulated event state.
    fn parse_line(&mut self, line: &str, chunks: &mut Vec<ChatChunk>) -> ProviderResult<()> {
        if line.is_empty() {
            flush_event(&mut self.event_data, chunks)?;
            return Ok(());
        }
        if let Some(data) = line.strip_prefix("data:") {
            self.event_data.push(data.trim_start().to_string());
        }
        Ok(())
    }
}

impl Default for SseParser {
    /// Create an empty parser with no buffered data.
    fn default() -> Self {
        Self::new()
    }
}

/// Flush accumulated `data:` lines into a parsed chat chunk unless it is `[DONE]`.
fn flush_event(event_data: &mut Vec<String>, chunks: &mut Vec<ChatChunk>) -> ProviderResult<()> {
    if event_data.is_empty() {
        return Ok(());
    }
    let data = event_data.join("\n");
    event_data.clear();
    if data == "[DONE]" {
        return Ok(());
    }
    let chunk = serde_json::from_str::<ChatChunk>(&data)
        .map_err(|error| ProviderError::Parse(format!("invalid OpenAI chat chunk: {error}")))?;
    chunks.push(chunk);
    Ok(())
}
