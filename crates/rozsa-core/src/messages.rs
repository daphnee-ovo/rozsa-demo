use rozsa_model::types::Message;

pub enum AgentMessage {
    Standard(Message),
    Custom(Box<dyn CustomMessage>),
}

pub trait CustomMessage: Send + Sync + std::fmt::Debug {
    fn message_type(&self) -> &str;
    fn as_any(&self) -> &dyn std::any::Any;
}
