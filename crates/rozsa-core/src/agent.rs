use crate::events::AgentEvent;
use crate::messages::AgentMessage;
use crate::queue::PendingMessageQueue;
use crate::tool::ToolExecutionMode;
use rozsa_model::types::{Model, ThinkingLevel, ThinkingBudgets, Transport};

pub struct Agent {
    pub(crate) state: AgentState,
    pub(crate) listeners: Vec<Box<dyn Fn(&AgentEvent) + Send + Sync>>,
    pub(crate) steering_queue: PendingMessageQueue,
    pub(crate) follow_up_queue: PendingMessageQueue,
    pub session_id: Option<String>,
    pub thinking_budgets: Option<ThinkingBudgets>,
    pub transport: Transport,
    pub max_retry_delay_ms: Option<u64>,
    pub tool_execution: ToolExecutionMode,
}

pub struct AgentState {
    pub system_prompt: String,
    pub model: Model,
    pub thinking_level: ThinkingLevel,
    pub tools: Vec<Box<dyn crate::tool::Tool>>,
    pub messages: Vec<AgentMessage>,
    pub is_streaming: bool,
    pub streaming_message: Option<rozsa_model::types::AssistantMessage>,
    pub pending_tool_calls: std::collections::HashSet<String>,
    pub error_message: Option<String>,
}
