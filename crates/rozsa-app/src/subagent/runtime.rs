// FrameworkTree
// runtime.rs
// ├── enum SubagentStatus
// ├── struct SubagentInfo
// └── struct SubagentRuntime

// File: subagent/runtime.rs
//
// Subagent runtime state — public info type + internal mutable runtime container.
//
// Internal Framework:
// runtime.rs
// ├── SubagentStatus       # idle / running / aborted / error
// ├── SubagentInfo         # public metadata (serializable)
// └── SubagentRuntime      # internal runtime (scope, messages, cancel token, ...)
//
// Related Code:
// - [manager.rs](./manager.rs)
// - [scope.rs](./scope.rs)

use std::path::PathBuf;
use std::sync::Arc;

use rozsa_core::messages::AgentMessage;
use rozsa_core::tool::Tool;
use rozsa_model::types::{Model, ThinkingEffort};
use serde::Serialize;
use tokio::sync::{Mutex, watch};
use tokio_util::sync::CancellationToken;

use super::scope::SubagentScope;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentStatus {
    Idle,
    Running,
    Aborted,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubagentInfo {
    pub id: String,
    pub name: String,
    pub status: SubagentStatus,
    pub model_id: String,
    pub model_provider: String,
    pub thinking_effort: ThinkingEffort,
    pub created_at: i64,
    pub last_activity_at: i64,
    pub last_error: Option<String>,
    pub message_count: usize,
    pub session_file: Option<PathBuf>,
}

/// Internal mutable runtime owned by SubagentManager.
/// Wrapped in `Arc<Mutex<...>>` so the background event drain task can update it.
pub(super) struct SubagentRuntime {
    pub info: SubagentInfo,
    pub scope: SubagentScope,
    pub messages: Vec<AgentMessage>,
    pub cancel_token: CancellationToken,
    pub system_prompt: String,
    pub model: Model,
    pub thinking_effort: ThinkingEffort,
    pub tools: Vec<Arc<dyn Tool>>,
    pub session_manager: Option<crate::session::manager::SessionManager>,
    /// Watch channel for status changes — used by `wait()`.
    pub status_tx: watch::Sender<SubagentStatus>,
}

pub(super) type SharedRuntime = Arc<Mutex<SubagentRuntime>>;