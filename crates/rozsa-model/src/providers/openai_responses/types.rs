// OpenAI Responses API type definitions (POST /v1/responses).
//
// Internal Framework:
// types.rs
// ├── ResponsesApiRequest        — top-level request body
// ├── Reasoning                   — reasoning effort/summary config
// ├── ResponseItem                — tagged union of input/output items
// ├── ContentItem                 — text/image content within a message
// ├── ReasoningSummaryPart        — single reasoning summary segment
// ├── ResponseEvent               — parsed SSE event (manually constructed)
// ├── TokenUsage                  — token consumption breakdown
// └── RawResponsesStreamEvent     — raw SSE JSON payload for parsing
//
// Reference:
// - codex-rs codex-api/src/common.rs (request types)
// - codex-rs protocol/src/models.rs (ResponseItem)
// - codex-rs codex-api/src/sse/responses.rs (SSE events)

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Request
// ---------------------------------------------------------------------------

/// Request body for POST /v1/responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponsesApiRequest {
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    pub input: Vec<ResponseItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<serde_json::Value>>,
    #[serde(default = "default_tool_choice")]
    pub tool_choice: String,
    pub parallel_tool_calls: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Reasoning>,
    pub store: bool,
    pub stream: bool,
    pub include: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_key: Option<String>,
}

fn default_tool_choice() -> String {
    "auto".to_string()
}

/// Reasoning configuration (effort level and summary style).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reasoning {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

// ---------------------------------------------------------------------------
// Items (input/output)
// ---------------------------------------------------------------------------

/// A single item in the request `input` array or response output.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseItem {
    Message {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        role: String,
        content: Vec<ContentItem>,
    },
    FunctionCall {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        name: String,
        arguments: String,
        call_id: String,
    },
    FunctionCallOutput {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        call_id: String,
        output: String,
    },
    Reasoning {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default)]
        summary: Vec<ReasoningSummaryPart>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        encrypted_content: Option<String>,
    },
}

/// Content within a Message item.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentItem {
    InputText {
        text: String,
    },
    InputImage {
        image_url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
    OutputText {
        text: String,
    },
}

/// A segment of a reasoning summary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReasoningSummaryPart {
    #[serde(rename = "type", default = "default_reasoning_summary_part_type")]
    pub kind: String,
    pub text: String,
}

impl ReasoningSummaryPart {
    pub fn summary_text(text: String) -> Self {
        Self {
            kind: default_reasoning_summary_part_type(),
            text,
        }
    }
}

fn default_reasoning_summary_part_type() -> String {
    "summary_text".to_string()
}

// ---------------------------------------------------------------------------
// SSE events (parsed from stream)
// ---------------------------------------------------------------------------

/// High-level events parsed from the Responses API SSE stream.
///
/// These are NOT serde-derived; they are constructed manually from
/// `RawResponsesStreamEvent` during stream processing.
#[derive(Debug, Clone)]
pub enum ResponseEvent {
    Created,
    OutputItemAdded {
        item: ResponseItem,
    },
    OutputItemDone {
        item: ResponseItem,
    },
    OutputTextDelta {
        delta: String,
    },
    FunctionCallArgsDelta {
        item_id: Option<String>,
        call_id: Option<String>,
        delta: String,
    },
    ReasoningSummaryDelta {
        delta: String,
        summary_index: Option<i64>,
    },
    ReasoningContentDelta {
        delta: String,
        content_index: Option<i64>,
    },
    Completed {
        response_id: String,
        usage: Option<TokenUsage>,
    },
    Failed {
        error_code: String,
        error_message: String,
    },
    Incomplete {
        reason: Option<String>,
    },
}

/// Token usage breakdown returned upon response completion.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u64>,
    pub total_tokens: u64,
}

// ---------------------------------------------------------------------------
// Raw SSE payload (for deserialization before event dispatch)
// ---------------------------------------------------------------------------

/// Raw JSON payload received from the SSE stream, before being converted
/// into a typed `ResponseEvent`.
#[derive(Debug, Deserialize)]
pub struct RawResponsesStreamEvent {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub response: Option<serde_json::Value>,
    #[serde(default)]
    pub item: Option<serde_json::Value>,
    #[serde(default)]
    pub item_id: Option<String>,
    #[serde(default)]
    pub call_id: Option<String>,
    #[serde(default)]
    pub delta: Option<String>,
    #[serde(default)]
    pub summary_index: Option<i64>,
    #[serde(default)]
    pub content_index: Option<i64>,
}
