// FrameworkTree
// config.rs
// ├── struct AgentContext
// ├── struct AgentLoopConfig
// ├── struct ShouldStopContext
// ├── struct TurnUpdate
// ├── struct PreToolUseContext
// ├── struct PreToolUseResult
// ├── struct PostToolUseContext
// └── struct PostToolUseResult

use crate::messages::AgentMessage;
use crate::tool::{Tool, ToolExecutionMode};
use rozsa_model::event_stream::EventStream;
use rozsa_model::types::{
    Context as ModelContext, Message, Model, SimpleStreamOptions, StreamEvent, ThinkingEffort,
    ToolSchema,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

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
    pub reasoning: Option<ThinkingEffort>,
    pub stream_options: SimpleStreamOptions,
    pub model_stream: ModelStreamFn,
    pub convert_to_llm: Box<dyn Fn(&[AgentMessage]) -> Vec<Message> + Send + Sync>,
    pub transform_context: Option<Box<dyn Fn(&[AgentMessage]) -> Vec<AgentMessage> + Send + Sync>>,
    pub get_api_key: Option<Box<dyn Fn(&str) -> Option<String> + Send + Sync>>,
    pub should_stop_after_turn: Option<Box<dyn Fn(&ShouldStopContext<'_>) -> bool + Send + Sync>>,
    pub prepare_next_turn:
        Option<Box<dyn Fn(&ShouldStopContext<'_>) -> Option<TurnUpdate> + Send + Sync>>,
    pub get_steering_messages: Option<Box<dyn Fn() -> Vec<AgentMessage> + Send + Sync>>,
    pub get_follow_up_messages: Option<Box<dyn Fn() -> Vec<AgentMessage> + Send + Sync>>,
    pub max_turns: Option<u32>,
    pub tool_execution: ToolExecutionMode,
    pub pre_tool_use: Option<
        Box<
            dyn Fn(
                    PreToolUseContext,
                ) -> std::pin::Pin<
                    Box<dyn std::future::Future<Output = Option<PreToolUseResult>> + Send>,
                > + Send
                + Sync,
        >,
    >,
    pub post_tool_use:
        Option<Box<dyn Fn(&PostToolUseContext) -> Option<PostToolUseResult> + Send + Sync>>,
    pub tools: Vec<Arc<dyn Tool>>,
}

#[derive(Debug)]
pub struct ShouldStopContext<'a> {
    pub message: &'a rozsa_model::types::AssistantMessage,
    pub tool_results: &'a [rozsa_model::types::ToolResultMessage],
    pub context: &'a AgentContext,
    pub new_messages: &'a [AgentMessage],
}

#[derive(Debug, Clone)]
pub struct TurnUpdate {
    pub context: Option<AgentContext>,
    pub model: Option<Model>,
    pub thinking_effort: Option<ThinkingEffort>,
}

#[derive(Debug, Clone)]
pub struct PreToolUseContext {
    pub assistant_message: rozsa_model::types::AssistantMessage,
    pub tool_call_id: String,
    pub tool_name: String,
    pub args: serde_json::Value,
    pub context: AgentContext,
    pub signal: Option<CancellationToken>,
}

#[derive(Debug, Clone)]
pub struct PreToolUseResult {
    pub block: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PostToolUseContext {
    pub assistant_message: rozsa_model::types::AssistantMessage,
    pub tool_call_id: String,
    pub tool_name: String,
    pub args: serde_json::Value,
    pub result: crate::tool::ToolResult,
    pub is_error: bool,
    pub context: AgentContext,
}

#[derive(Debug, Clone)]
pub struct PostToolUseResult {
    pub content: Option<Vec<rozsa_model::types::ContentBlock>>,
    pub is_error: Option<bool>,
    pub terminate: Option<bool>,
}