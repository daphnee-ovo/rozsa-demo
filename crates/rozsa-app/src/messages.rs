// FrameworkTree
// messages.rs
// ├── struct CompactionMessage
// ├── struct ModelChangeMessage
// ├── struct ModelInfo
// ├── struct ThinkingEffortChangeMessage
// ├── struct SystemPromptMessage
// ├── struct StatusMessage
// ├── enum AppMessage
// ├── impl AppMessage
// ├── compaction()
// ├── model_change()
// ├── thinking_effort_change()
// ├── system_prompt()
// ├── status()
// ├── message_type()
// ├── impl AgentMessage
// ├── from()
// ├── struct BashExecutionMessage
// ├── impl BashExecutionMessage
// ├── new()
// ├── to_agent_message()
// ├── struct BranchSummaryMessage
// ├── impl BranchSummaryMessage
// ├── new()
// └── to_agent_message()

//! Product-level custom message types for rozsa-app.
//!
//! These message types extend the base AgentMessage from rozsa-core with
//! application-specific messages for status updates, compaction, model changes, etc.

use rozsa_core::messages::AgentMessage;
use serde::{Deserialize, Serialize};

/// Compaction summary message - indicates context was compacted
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionMessage {
    pub summary: String,
    pub removed_count: usize,
    pub tokens_before: u64,
}

/// Model change notification message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelChangeMessage {
    pub from_model: Option<ModelInfo>,
    pub to_model: ModelInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub provider: String,
    pub id: String,
}

/// Thinking effort change notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingEffortChangeMessage {
    pub level: String,
}

/// System prompt injection message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemPromptMessage {
    pub content: String,
}

/// Generic status message for UI display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusMessage {
    pub text: String,
    pub display_only: bool,
}

/// App-level message variants
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppMessage {
    Compaction(CompactionMessage),
    ModelChange(ModelChangeMessage),
    ThinkingEffortChange(ThinkingEffortChangeMessage),
    SystemPrompt(SystemPromptMessage),
    Status(StatusMessage),
}

impl AppMessage {
    /// Create a compaction message
    pub fn compaction(summary: String, removed_count: usize, tokens_before: u64) -> Self {
        Self::Compaction(CompactionMessage {
            summary,
            removed_count,
            tokens_before,
        })
    }

    /// Create a model change message
    pub fn model_change(from_model: Option<(String, String)>, to_model: (String, String)) -> Self {
        Self::ModelChange(ModelChangeMessage {
            from_model: from_model.map(|(provider, id)| ModelInfo { provider, id }),
            to_model: ModelInfo {
                provider: to_model.0,
                id: to_model.1,
            },
        })
    }

    /// Create a thinking effort change message
    pub fn thinking_effort_change(effort: impl Into<String>) -> Self {
        Self::ThinkingEffortChange(ThinkingEffortChangeMessage {
            level: effort.into(),
        })
    }

    /// Create a system prompt message
    pub fn system_prompt(content: impl Into<String>) -> Self {
        Self::SystemPrompt(SystemPromptMessage {
            content: content.into(),
        })
    }

    /// Create a status message
    pub fn status(text: impl Into<String>, display_only: bool) -> Self {
        Self::Status(StatusMessage {
            text: text.into(),
            display_only,
        })
    }

    /// Get the message type name for serialization
    pub fn message_type(&self) -> &'static str {
        match self {
            Self::Compaction(_) => "compaction",
            Self::ModelChange(_) => "model_change",
            Self::ThinkingEffortChange(_) => "thinking_effort_change",
            Self::SystemPrompt(_) => "system_prompt",
            Self::Status(_) => "status",
        }
    }
}

impl From<AppMessage> for AgentMessage {
    fn from(app_msg: AppMessage) -> Self {
        let message_type = app_msg.message_type().to_string();
        let payload = serde_json::to_value(&app_msg).unwrap_or_else(|_| serde_json::json!({}));
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        AgentMessage::custom(message_type, payload, timestamp)
    }
}

/// Bash execution message - stores results from shell commands
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BashExecutionMessage {
    pub command: String,
    pub output: String,
    pub exit_code: Option<i32>,
    pub cancelled: bool,
    pub truncated: bool,
    pub full_output_path: Option<String>,
    pub timestamp: i64,
    pub exclude_from_context: bool,
}

impl BashExecutionMessage {
    pub fn new(command: String, output: String, exit_code: Option<i32>) -> Self {
        Self {
            command,
            output,
            exit_code,
            cancelled: false,
            truncated: false,
            full_output_path: None,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64,
            exclude_from_context: false,
        }
    }

    pub fn to_agent_message(self) -> AgentMessage {
        let payload = serde_json::to_value(&self).unwrap_or_else(|_| serde_json::json!({}));
        AgentMessage::custom("bash_execution".to_string(), payload, self.timestamp)
    }
}

/// Branch summary message - represents a compacted branch point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchSummaryMessage {
    pub summary: String,
    pub from_id: String,
    pub timestamp: i64,
}

impl BranchSummaryMessage {
    pub fn new(summary: String, from_id: String) -> Self {
        Self {
            summary,
            from_id,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64,
        }
    }

    pub fn to_agent_message(self) -> AgentMessage {
        let payload = serde_json::to_value(&self).unwrap_or_else(|_| serde_json::json!({}));
        AgentMessage::custom("branch_summary".to_string(), payload, self.timestamp)
    }
}