pub mod scope;
pub mod runtime;
pub mod manager;

pub use runtime::{SubagentInfo, SubagentStatus};
pub use manager::{SubagentManager, SpawnConfig, SubagentSnapshot, SharedResources};
pub use scope::SubagentScope;
