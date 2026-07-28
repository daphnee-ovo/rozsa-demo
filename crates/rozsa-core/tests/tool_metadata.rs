use rozsa_core::tool::{Tool, ToolError, ToolMetadata, ToolResult, tool_metadata};

struct ExampleTool;

#[async_trait::async_trait]
impl Tool for ExampleTool {
    fn name(&self) -> &str {
        "example"
    }

    fn description(&self) -> &str {
        "An actual tool description"
    }

    fn label(&self) -> &str {
        "Example"
    }

    fn parameters_schema(&self) -> &serde_json::Value {
        static SCHEMA: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| serde_json::json!({"type": "object"}))
    }

    async fn execute(
        &self,
        _tool_call_id: &str,
        _params: serde_json::Value,
        _signal: Option<tokio_util::sync::CancellationToken>,
        _on_update: Option<&(dyn Fn(ToolResult) + Send + Sync)>,
    ) -> Result<ToolResult, ToolError> {
        unreachable!()
    }
}

#[test]
fn metadata_is_derived_from_the_registered_tool_contract() {
    let tool = ExampleTool;
    assert_eq!(
        tool_metadata([&tool as &dyn Tool]),
        vec![ToolMetadata {
            name: "example".to_owned(),
            label: "Example".to_owned(),
            description: "An actual tool description".to_owned(),
        }]
    );
}
