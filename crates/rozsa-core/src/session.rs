use crate::messages::AgentMessage;

pub struct SessionInfo {
    pub id: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub message_count: usize,
}

#[async_trait::async_trait]
pub trait SessionStore: Send + Sync {
    async fn save(&self, session_id: &str, messages: &[AgentMessage]) -> anyhow::Result<()>;
    async fn load(&self, session_id: &str) -> anyhow::Result<Option<Vec<AgentMessage>>>;
    async fn list(&self) -> anyhow::Result<Vec<SessionInfo>>;
    async fn delete(&self, session_id: &str) -> anyhow::Result<()>;
}
