use crate::messages::AgentMessage;
use rozsa_model::types::{AssistantMessage, ToolResultMessage};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    AgentStart,
    AgentEnd {
        messages: Vec<AgentMessage>,
    },

    TurnStart,
    TurnEnd {
        message: AssistantMessage,
        tool_results: Vec<ToolResultMessage>,
    },

    MessageStart {
        message: AgentMessage,
    },
    MessageUpdate {
        message: AgentMessage,
        stream_event: rozsa_model::types::StreamEvent,
    },
    MessageEnd {
        message: AgentMessage,
    },

    ToolExecutionStart {
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
    },
    ToolExecutionEnd {
        tool_call_id: String,
        tool_name: String,
        result: ToolResultMessage,
    },
}
