use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Provider protocol family used to select a streaming implementation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Api {
    AnthropicMessages,
    OpenAICompletions,
    OpenAIResponses,
    BedrockConverseStream,
    GoogleGenerativeAI,
    GoogleVertex,
    MistralConversations,
    Custom(String),
}

/// Concrete provider identity used for credentials and compatibility rules.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Provider {
    Anthropic,
    OpenAI,
    AmazonBedrock,
    Google,
    GoogleVertex,
    DeepSeek,
    OpenRouter,
    XAI,
    Groq,
    Cerebras,
    Mistral,
    Zai,
    Together,
    MoonshotAI,
    MoonshotAICn,
    HuggingFace,
    CloudflareWorkersAI,
    CloudflareAIGateway,
    Xiaomi,
    XiaomiTokenPlanCn,
    XiaomiTokenPlanAms,
    XiaomiTokenPlanSgp,
    Custom(String),
}

/// Input modality supported by a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputModality {
    Text,
    Image,
}

/// Per-million-token provider cost metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCost {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
}

/// Unified reasoning control exposed to callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ThinkingLevel {
    Off,
    Minimal,
    Low,
    Medium,
    High,
    XHigh,
}

/// Optional token budgets for provider-specific reasoning levels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingBudgets {
    pub minimal: Option<u64>,
    pub low: Option<u64>,
    pub medium: Option<u64>,
    pub high: Option<u64>,
}

/// Model metadata required to route and price a request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub api: Api,
    pub provider: Provider,
    pub base_url: String,
    pub reasoning: bool,
    pub input_modalities: Vec<InputModality>,
    pub cost: ModelCost,
    pub context_window: usize,
    pub max_tokens: usize,
    pub thinking_level_map: Option<HashMap<ThinkingLevel, Option<String>>>,
    pub headers: Option<HashMap<String, String>>,
    #[serde(default)]
    pub compat: Option<serde_json::Value>,
}

/// Transport preference for providers that support multiple streaming protocols.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Transport {
    Sse,
    WebSocket,
    WebSocketCached,
    Auto,
}

/// Prompt cache retention preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheRetention {
    None,
    Short,
    Long,
}

/// Provider request options common to all streaming methods.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamOptions {
    pub temperature: Option<f64>,
    pub max_tokens: Option<usize>,
    pub api_key: Option<String>,
    pub transport: Transport,
    pub cache_retention: CacheRetention,
    pub session_id: Option<String>,
    pub headers: Option<HashMap<String, String>>,
    pub timeout_ms: Option<u64>,
    pub max_retries: Option<u32>,
    pub max_retry_delay_ms: Option<u64>,
    pub metadata: Option<serde_json::Value>,
}

/// Simplified stream options with unified reasoning controls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpleStreamOptions {
    #[serde(flatten)]
    pub base: StreamOptions,
    pub reasoning: Option<ThinkingLevel>,
    pub thinking_budgets: Option<ThinkingBudgets>,
    pub tool_choice: Option<serde_json::Value>,
}

/// Normalized reason a provider stopped streaming.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StopReason {
    Stop,
    Length,
    ToolUse,
    Error,
    Aborted,
}

/// Provider tool call with parsed JSON arguments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Message content block shared across providers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContentBlock {
    Text {
        text: String,
        signature: Option<String>,
    },
    Thinking {
        thinking: String,
        signature: Option<String>,
        redacted: bool,
    },
    Image {
        data: String,
        mime_type: String,
    },
    ToolCall(ToolCall),
}

/// Monetary usage cost derived from model cost metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageCost {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
    pub total: f64,
}

/// Token usage reported by a provider or derived by the model layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub total_tokens: u64,
    pub cost: UsageCost,
}

/// User message content in text-only or multimodal form.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UserContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

/// Message sent by a user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessage {
    pub content: UserContent,
    pub display_text: Option<String>,
    pub timestamp: i64,
}

/// Message returned by a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessage {
    pub content: Vec<ContentBlock>,
    pub api: Api,
    pub provider: Provider,
    pub model: String,
    pub response_model: Option<String>,
    pub response_id: Option<String>,
    pub usage: Usage,
    pub stop_reason: StopReason,
    pub error_message: Option<String>,
    pub timestamp: i64,
}

/// Tool execution result sent back to a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultMessage {
    pub tool_call_id: String,
    pub tool_name: String,
    pub content: Vec<ContentBlock>,
    pub is_error: bool,
    pub timestamp: i64,
}

/// Conversation message accepted by providers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    User(UserMessage),
    Assistant(AssistantMessage),
    ToolResult(ToolResultMessage),
}

/// JSON-schema-backed tool definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Provider request context including prompt, history, and tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Context {
    pub system_prompt: Option<String>,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolSchema>,
}

/// Stream event emitted by provider implementations.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    Start {
        partial: AssistantMessage,
    },
    TextStart {
        content_index: usize,
        partial: AssistantMessage,
    },
    TextDelta {
        content_index: usize,
        delta: String,
        partial: AssistantMessage,
    },
    TextEnd {
        content_index: usize,
        content: String,
        partial: AssistantMessage,
    },
    ThinkingStart {
        content_index: usize,
        partial: AssistantMessage,
    },
    ThinkingDelta {
        content_index: usize,
        delta: String,
        partial: AssistantMessage,
    },
    ThinkingEnd {
        content_index: usize,
        content: String,
        partial: AssistantMessage,
    },
    ToolCallStart {
        content_index: usize,
        partial: AssistantMessage,
    },
    ToolCallDelta {
        content_index: usize,
        delta: String,
        partial: AssistantMessage,
    },
    ToolCallEnd {
        content_index: usize,
        tool_call: ToolCall,
        partial: AssistantMessage,
    },
    Done {
        reason: StopReason,
        message: AssistantMessage,
    },
    Error {
        reason: StopReason,
        error: AssistantMessage,
    },
}
