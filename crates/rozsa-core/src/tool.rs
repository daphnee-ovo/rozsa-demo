// FrameworkTree
// tool.rs
// ├── struct ToolMetadata
// ├── impl ToolMetadata
// ├── from_tool()
// ├── tool_metadata()
// ├── enum ToolExecutionMode
// ├── struct ToolResult
// ├── enum ToolError
// ├── trait Tool
// ├── prepare_arguments()
// └── execution_mode()

use rozsa_model::types::ContentBlock;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

/// Stable, serializable metadata derived from a concrete registered tool.
///
/// Frontends consume this shape instead of maintaining a second hard-coded
/// catalog that can drift from the tools available to the agent loop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolMetadata {
    pub name: String,
    pub label: String,
    pub description: String,
}

impl ToolMetadata {
    pub fn from_tool(tool: &dyn Tool) -> Self {
        Self {
            name: tool.name().to_owned(),
            label: tool.label().to_owned(),
            description: tool.description().to_owned(),
        }
    }
}

pub fn tool_metadata<'a>(tools: impl IntoIterator<Item = &'a dyn Tool>) -> Vec<ToolMetadata> {
    tools.into_iter().map(ToolMetadata::from_tool).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolExecutionMode {
    Sequential,
    Parallel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub content: Vec<ContentBlock>,
    pub details: serde_json::Value,
    pub terminate: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("tool execution failed: {0}")]
    Execution(String),
    #[error("tool cancelled")]
    Cancelled,
}

#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn label(&self) -> &str;
    fn parameters_schema(&self) -> &serde_json::Value;

    async fn execute(
        &self,
        tool_call_id: &str,
        params: serde_json::Value,
        signal: Option<CancellationToken>,
        on_update: Option<&(dyn Fn(ToolResult) + Send + Sync)>,
    ) -> Result<ToolResult, ToolError>;

    fn prepare_arguments(&self, args: serde_json::Value) -> serde_json::Value {
        args
    }

    fn execution_mode(&self) -> Option<ToolExecutionMode> {
        None
    }
}
