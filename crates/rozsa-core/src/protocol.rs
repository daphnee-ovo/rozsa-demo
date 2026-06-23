//! JSONL bridge protocol between the TypeScript host and `rozsa-core`.
//!
//! Messages flow bidirectionally over stdin/stdout:
//! - stdin: TS → Rust (start_run, cancel, tool_result)
//! - stdout: Rust → TS (agent_event, tool_request, run_done, run_error)
//!
//! Protocol invariants:
//! - Only one run at a time (no concurrent runs)
//! - stdout is ONLY protocol JSON lines
//! - stderr is ONLY logs
//! - Every message has a version field
//! - requestId correlates tool_request with tool_result
//! - runId correlates all messages in a single run

use serde::{Deserialize, Serialize};

use crate::config::AgentContext;
use crate::events::AgentEvent;
use crate::messages::AgentMessage;
use crate::tool::ToolExecutionMode;

pub const PROTOCOL_VERSION: u32 = 1;

// --- Input (TS → Rust) ---

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BridgeInput {
    StartRun {
        version: u32,
        run_id: String,
        #[serde(flatten)]
        mode: RunMode,
        config: BridgeConfig,
    },
    Cancel {
        version: u32,
        run_id: String,
    },
    ToolResult {
        version: u32,
        run_id: String,
        request_id: String,
        result: ToolHostResult,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum RunMode {
    Prompt {
        prompts: Vec<AgentMessage>,
        context: AgentContext,
    },
    Continue {
        context: AgentContext,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeConfig {
    pub model: rozsa_model::types::Model,
    pub reasoning: Option<rozsa_model::types::ThinkingLevel>,
    pub stream_options: rozsa_model::types::SimpleStreamOptions,
    pub tool_execution: ToolExecutionMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolHostResult {
    pub content: Vec<rozsa_model::types::ContentBlock>,
    pub is_error: bool,
    pub terminate: bool,
}

// --- Output (Rust → TS) ---

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BridgeOutput {
    AgentEvent {
        version: u32,
        run_id: String,
        event: AgentEvent,
    },
    ToolRequest {
        version: u32,
        run_id: String,
        request_id: String,
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
        assistant_message: rozsa_model::types::AssistantMessage,
        context: AgentContext,
    },
    RunDone {
        version: u32,
        run_id: String,
    },
    RunError {
        version: u32,
        run_id: String,
        error: String,
    },
}

// Helper constructors
impl BridgeOutput {
    pub fn agent_event(run_id: &str, event: AgentEvent) -> Self {
        Self::AgentEvent {
            version: PROTOCOL_VERSION,
            run_id: run_id.to_string(),
            event,
        }
    }

    pub fn tool_request(
        run_id: &str,
        request_id: &str,
        tool_call_id: &str,
        tool_name: &str,
        args: serde_json::Value,
        assistant_message: rozsa_model::types::AssistantMessage,
        context: AgentContext,
    ) -> Self {
        Self::ToolRequest {
            version: PROTOCOL_VERSION,
            run_id: run_id.to_string(),
            request_id: request_id.to_string(),
            tool_call_id: tool_call_id.to_string(),
            tool_name: tool_name.to_string(),
            args,
            assistant_message,
            context,
        }
    }

    pub fn run_done(run_id: &str) -> Self {
        Self::RunDone {
            version: PROTOCOL_VERSION,
            run_id: run_id.to_string(),
        }
    }

    pub fn run_error(run_id: &str, error: impl ToString) -> Self {
        Self::RunError {
            version: PROTOCOL_VERSION,
            run_id: run_id.to_string(),
            error: error.to_string(),
        }
    }
}

pub fn parse_input_line(line: &str) -> Result<BridgeInput, serde_json::Error> {
    serde_json::from_str(line)
}
