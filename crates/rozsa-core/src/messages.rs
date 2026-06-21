use rozsa_model::types::Message;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentMessage {
    Standard { message: Message },
    Custom { message: CustomAgentMessage },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomAgentMessage {
    pub message_type: String,
    pub payload: serde_json::Value,
    pub timestamp: i64,
}

impl AgentMessage {
    pub fn standard(message: Message) -> Self {
        Self::Standard { message }
    }

    pub fn custom(message_type: String, payload: serde_json::Value, timestamp: i64) -> Self {
        Self::Custom {
            message: CustomAgentMessage {
                message_type,
                payload,
                timestamp,
            },
        }
    }

    pub fn as_standard(&self) -> Option<&Message> {
        match self {
            Self::Standard { message } => Some(message),
            Self::Custom { .. } => None,
        }
    }
}
