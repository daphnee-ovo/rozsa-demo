use crate::messages::AgentMessage;
use crate::tool::{Tool, ToolExecutionMode};
use rozsa_model::event_stream::EventStream;
use rozsa_model::types::{
    Context as ModelContext, Message, Model, SimpleStreamOptions, StreamEvent, ThinkingLevel,
    ToolSchema,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub type ModelStreamFn = Box<
    dyn Fn(&Model, &ModelContext, &SimpleStreamOptions) -> EventStream<StreamEvent> + Send + Sync,
>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentContext {
    pub system_prompt: Option<String>,
    pub messages: Vec<AgentMessage>,
    pub tools: Vec<ToolSchema>,
}

pub struct AgentLoopConfig {
    pub model: Model,
    pub reasoning: Option<ThinkingLevel>,
    pub stream_options: SimpleStreamOptions,
    pub model_stream: ModelStreamFn,
    pub convert_to_llm: Box<dyn Fn(&[AgentMessage]) -> Vec<Message> + Send + Sync>,
    pub transform_context: Option<Box<dyn Fn(&[AgentMessage]) -> Vec<AgentMessage> + Send + Sync>>,
    pub get_api_key: Option<Box<dyn Fn(&str) -> Option<String> + Send + Sync>>,
    pub should_stop_after_turn: Option<Box<dyn Fn(&ShouldStopContext) -> bool + Send + Sync>>,
    pub prepare_next_turn:
        Option<Box<dyn Fn(&ShouldStopContext) -> Option<TurnUpdate> + Send + Sync>>,
    pub get_steering_messages: Option<Box<dyn Fn() -> Vec<AgentMessage> + Send + Sync>>,
    pub get_follow_up_messages: Option<Box<dyn Fn() -> Vec<AgentMessage> + Send + Sync>>,
    pub tool_execution: ToolExecutionMode,
    pub before_tool_call:
        Option<Box<dyn Fn(&BeforeToolCallContext) -> Option<BeforeToolCallResult> + Send + Sync>>,
    pub after_tool_call:
        Option<Box<dyn Fn(&AfterToolCallContext) -> Option<AfterToolCallResult> + Send + Sync>>,
    pub tools: Vec<Arc<dyn Tool>>,
}

#[derive(Debug, Clone)]
pub struct ShouldStopContext {
    pub message: rozsa_model::types::AssistantMessage,
    pub tool_results: Vec<rozsa_model::types::ToolResultMessage>,
    pub context: AgentContext,
    pub new_messages: Vec<AgentMessage>,
}

#[derive(Debug, Clone)]
pub struct TurnUpdate {
    pub context: Option<AgentContext>,
    pub model: Option<Model>,
    pub thinking_level: Option<ThinkingLevel>,
}

#[derive(Debug, Clone)]
pub struct BeforeToolCallContext {
    pub assistant_message: rozsa_model::types::AssistantMessage,
    pub tool_call_id: String,
    pub tool_name: String,
    pub args: serde_json::Value,
    pub context: AgentContext,
}

#[derive(Debug, Clone)]
pub struct BeforeToolCallResult {
    pub block: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AfterToolCallContext {
    pub assistant_message: rozsa_model::types::AssistantMessage,
    pub tool_call_id: String,
    pub tool_name: String,
    pub args: serde_json::Value,
    pub result: crate::tool::ToolResult,
    pub is_error: bool,
    pub context: AgentContext,
}

#[derive(Debug, Clone)]
pub struct AfterToolCallResult {
    pub content: Option<Vec<rozsa_model::types::ContentBlock>>,
    pub is_error: Option<bool>,
    pub terminate: Option<bool>,
}
