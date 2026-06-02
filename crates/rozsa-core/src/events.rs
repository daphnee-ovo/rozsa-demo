use crate::messages::AgentMessage;
use crate::tool::ToolResult;
use rozsa_model::types::{AssistantMessage, ToolResultMessage};

pub enum AgentEvent {
    AgentStart,
    AgentEnd { messages: Vec<AgentMessage> },

    TurnStart,
    TurnEnd { message: AssistantMessage, tool_results: Vec<ToolResultMessage> },

    MessageStart { message: AgentMessage },
    MessageUpdate { message: AgentMessage, stream_event: rozsa_model::types::StreamEvent },
    MessageEnd { message: AgentMessage },

    ToolExecutionStart { tool_call_id: String, tool_name: String, args: serde_json::Value },
    ToolExecutionUpdate { tool_call_id: String, tool_name: String, partial_result: ToolResult },
    ToolExecutionEnd { tool_call_id: String, tool_name: String, result: ToolResult, is_error: bool },
}
