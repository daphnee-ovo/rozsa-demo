// FrameworkTree
// subagent.rs
// ├── struct SubagentTool
// ├── impl SubagentTool
// ├── new()
// ├── parse_scope()
// ├── format_summary()
// ├── extract_last_assistant_text()
// ├── info_details()
// ├── impl SubagentTool
// ├── name()
// ├── description()
// ├── label()
// ├── parameters_schema()
// ├── execution_mode()
// ├── execute()
// └── create_subagent_tool()

// File: tools/subagent.rs
//
// Subagent tool — exposes SubagentManager to the model via the Tool trait.
//
// Internal Framework:
// subagent.rs
// ├── SubagentTool
// │   ├── name() → "subagent"
// │   ├── execute() → dispatch by action
// │   │   ├── spawn
// │   │   ├── send
// │   │   ├── wait
// │   │   ├── interrupt
// │   │   └── list
// │   └── parse_scope()
// └── create_subagent_tool()
//
// Related Code:
// - [SubagentManager](../subagent/manager.rs)
// - [SubagentScope](../subagent/scope.rs)
// - [Tool trait](../../rozsa-core/src/tool.rs)

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use rozsa_core::tool::{Tool, ToolError, ToolExecutionMode, ToolResult};
use rozsa_model::types::ContentBlock;
use serde_json::{Value, json};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::subagent::manager::{SpawnConfig, SubagentManager};
use crate::subagent::scope::{AllowedTools, SubagentScope};

pub struct SubagentTool {
    manager: Arc<Mutex<SubagentManager>>,
}

impl SubagentTool {
    pub fn new(manager: Arc<Mutex<SubagentManager>>) -> Self {
        Self { manager }
    }

    fn parse_scope(scope_val: Option<&Value>) -> SubagentScope {
        let Some(val) = scope_val else {
            return SubagentScope::inherit();
        };

        if let Some(s) = val.as_str() {
            return match s {
                "readonly" => SubagentScope::readonly(),
                _ => SubagentScope::inherit(),
            };
        }

        if let Some(obj) = val.as_object() {
            let scope_type = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
            match scope_type {
                "scoped" => {
                    let paths: Vec<PathBuf> = obj
                        .get("paths")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(PathBuf::from))
                                .collect()
                        })
                        .unwrap_or_default();
                    SubagentScope::scoped(paths)
                }
                "custom" => {
                    let tools = obj
                        .get("tools")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            let set: HashSet<String> = arr
                                .iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect();
                            AllowedTools::Only(set)
                        })
                        .unwrap_or(AllowedTools::All);

                    let paths = obj.get("paths").and_then(|v| v.as_array()).map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(PathBuf::from))
                            .collect()
                    });

                    let bash_prefixes =
                        obj.get("bash_prefixes")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str().map(String::from))
                                    .collect()
                            });

                    let skills = obj.get("skills").and_then(|v| v.as_array()).map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    });

                    SubagentScope::custom(tools, paths, bash_prefixes, skills)
                }
                _ => SubagentScope::inherit(),
            }
        } else {
            SubagentScope::inherit()
        }
    }

    fn format_summary(
        info: &crate::subagent::runtime::SubagentInfo,
        messages: &[rozsa_core::messages::AgentMessage],
    ) -> String {
        let mut lines = vec![
            format!("{} ({})", info.id, info.name),
            format!("status: {:?}", info.status),
            format!("model: {}/{}", info.model_provider, info.model_id),
            format!("thinking_effort: {:?}", info.thinking_effort),
            format!("messages: {}", messages.len()),
        ];

        if let Some(ref err) = info.last_error {
            lines.push(format!("last_error: {}", err));
        }

        if let Some(text) = Self::extract_last_assistant_text(messages) {
            lines.push(format!("last_assistant: {}", text));
        }

        lines.join("\n")
    }

    fn extract_last_assistant_text(
        messages: &[rozsa_core::messages::AgentMessage],
    ) -> Option<String> {
        use rozsa_model::types::{ContentBlock, Message};

        for msg in messages.iter().rev() {
            if let Some(Message::Assistant(assistant)) = msg.as_standard() {
                let text: String = assistant
                    .content
                    .iter()
                    .filter_map(|block| match block {
                        ContentBlock::Text { text, .. } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("");
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
        None
    }

    fn info_details(action: &str, info: &crate::subagent::runtime::SubagentInfo) -> Value {
        json!({
            "action": action,
            "id": info.id,
            "name": info.name,
            "status": format!("{:?}", info.status).to_lowercase(),
            "model_id": info.model_id,
            "model_provider": info.model_provider,
            "thinking_effort": format!("{:?}", info.thinking_effort).to_lowercase(),
        })
    }
}

#[async_trait::async_trait]
impl Tool for SubagentTool {
    fn name(&self) -> &str {
        "subagent"
    }

    fn description(&self) -> &str {
        "Create, message, wait for, list, or interrupt independent subagents. \
         Use spawn with a focused system_prompt and optional prompt to delegate work. \
         Subagents run with their own transcript and can be inspected or interrupted by the user."
    }

    fn label(&self) -> &str {
        "Subagent"
    }

    fn parameters_schema(&self) -> &serde_json::Value {
        static SCHEMA: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["spawn", "send", "wait", "interrupt", "list"],
                        "description": "Subagent action to perform"
                    },
                    "id": {
                        "type": "string",
                        "description": "Subagent id for send, wait, or interrupt"
                    },
                    "name": {
                        "type": "string",
                        "description": "Short human-readable name for a new subagent"
                    },
                    "system_prompt": {
                        "type": "string",
                        "description": "System prompt for a new subagent (required for spawn)"
                    },
                    "prompt": {
                        "type": "string",
                        "description": "User prompt to send to the subagent"
                    },
                    "wait": {
                        "type": "boolean",
                        "description": "Wait for the subagent to become idle before returning"
                    },
                    "scope": {
                        "description": "Scope/permission level. Default: 'inherit'. Options: 'inherit', 'readonly', or an object with type 'scoped' or 'custom'."
                    }
                },
                "required": ["action"]
            })
        })
    }

    fn execution_mode(&self) -> Option<ToolExecutionMode> {
        Some(ToolExecutionMode::Sequential)
    }

    async fn execute(
        &self,
        _tool_call_id: &str,
        params: serde_json::Value,
        signal: Option<CancellationToken>,
        _on_update: Option<&(dyn Fn(ToolResult) + Send + Sync)>,
    ) -> Result<ToolResult, ToolError> {
        if signal.as_ref().is_some_and(|t| t.is_cancelled()) {
            return Err(ToolError::Cancelled);
        }

        let action = params
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        match action.as_str() {
            "spawn" => {
                let system_prompt = params
                    .get("system_prompt")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();

                if system_prompt.is_empty() {
                    return Err(ToolError::Execution(
                        "subagent spawn requires system_prompt".to_string(),
                    ));
                }

                let name = params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let scope = Self::parse_scope(params.get("scope"));

                let config = SpawnConfig {
                    name,
                    system_prompt,
                    model: None,
                    thinking_effort: None,
                    scope,
                };

                let mut manager = self.manager.lock().await;
                let info = manager.spawn(config).await.map_err(ToolError::Execution)?;

                let prompt = params
                    .get("prompt")
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim().to_string());
                if let Some(prompt_text) = prompt.filter(|s| !s.is_empty()) {
                    let wait = params
                        .get("wait")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let id = info.id.clone();
                    manager
                        .send(&id, &prompt_text, wait)
                        .await
                        .map_err(ToolError::Execution)?;
                }

                let list = manager.list().await;
                let final_info = list.iter().find(|i| i.id == info.id).unwrap_or(&info);
                let messages = manager.get_messages(&info.id).await.unwrap_or_default();

                Ok(ToolResult {
                    content: vec![ContentBlock::Text {
                        text: Self::format_summary(final_info, &messages),
                        signature: None,
                    }],
                    details: Self::info_details("spawn", final_info),
                    terminate: false,
                })
            }

            "send" => {
                let id = params
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if id.is_empty() {
                    return Err(ToolError::Execution(
                        "subagent send requires id".to_string(),
                    ));
                }

                let prompt = params
                    .get("prompt")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if prompt.is_empty() {
                    return Err(ToolError::Execution(
                        "subagent send requires prompt".to_string(),
                    ));
                }

                let wait = params
                    .get("wait")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let manager = self.manager.lock().await;
                manager
                    .send(&id, &prompt, wait)
                    .await
                    .map_err(ToolError::Execution)?;

                let list = manager.list().await;
                let info = list
                    .iter()
                    .find(|i| i.id == id)
                    .ok_or_else(|| ToolError::Execution(format!("subagent '{}' not found", id)))?;
                let messages = manager.get_messages(&id).await.unwrap_or_default();

                Ok(ToolResult {
                    content: vec![ContentBlock::Text {
                        text: Self::format_summary(info, &messages),
                        signature: None,
                    }],
                    details: Self::info_details("send", info),
                    terminate: false,
                })
            }

            "wait" => {
                let id = params
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if id.is_empty() {
                    return Err(ToolError::Execution(
                        "subagent wait requires id".to_string(),
                    ));
                }

                let manager = self.manager.lock().await;
                manager.wait(&id).await.map_err(ToolError::Execution)?;

                let list = manager.list().await;
                let info = list
                    .iter()
                    .find(|i| i.id == id)
                    .ok_or_else(|| ToolError::Execution(format!("subagent '{}' not found", id)))?;
                let messages = manager.get_messages(&id).await.unwrap_or_default();

                Ok(ToolResult {
                    content: vec![ContentBlock::Text {
                        text: Self::format_summary(info, &messages),
                        signature: None,
                    }],
                    details: Self::info_details("wait", info),
                    terminate: false,
                })
            }

            "interrupt" => {
                let id = params
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if id.is_empty() {
                    return Err(ToolError::Execution(
                        "subagent interrupt requires id".to_string(),
                    ));
                }

                let manager = self.manager.lock().await;
                manager.abort(&id).await.map_err(ToolError::Execution)?;

                let list = manager.list().await;
                let info = list
                    .iter()
                    .find(|i| i.id == id)
                    .ok_or_else(|| ToolError::Execution(format!("subagent '{}' not found", id)))?;
                let messages = manager.get_messages(&id).await.unwrap_or_default();

                Ok(ToolResult {
                    content: vec![ContentBlock::Text {
                        text: Self::format_summary(info, &messages),
                        signature: None,
                    }],
                    details: Self::info_details("interrupt", info),
                    terminate: false,
                })
            }

            "list" => {
                let manager = self.manager.lock().await;
                let subagents = manager.list().await;

                let content = if subagents.is_empty() {
                    "No subagents.".to_string()
                } else {
                    let mut lines = Vec::new();
                    for info in &subagents {
                        lines.push(format!(
                            "{} ({}) — status: {:?}, model: {}/{}",
                            info.id, info.name, info.status, info.model_provider, info.model_id,
                        ));
                    }
                    lines.join("\n")
                };

                let details = json!({
                    "action": "list",
                    "subagents": subagents.iter().map(|info| json!({
                        "id": info.id,
                        "name": info.name,
                        "status": format!("{:?}", info.status).to_lowercase(),
                        "model_id": info.model_id,
                        "model_provider": info.model_provider,
                        "thinking_effort": format!("{:?}", info.thinking_effort).to_lowercase(),
                    })).collect::<Vec<_>>(),
                });

                Ok(ToolResult {
                    content: vec![ContentBlock::Text {
                        text: content,
                        signature: None,
                    }],
                    details,
                    terminate: false,
                })
            }

            _ => Err(ToolError::Execution(format!(
                "Unknown subagent action: '{}'. Valid actions: spawn, send, wait, interrupt, list",
                action
            ))),
        }
    }
}

pub fn create_subagent_tool(manager: Arc<Mutex<SubagentManager>>) -> Box<dyn Tool> {
    Box::new(SubagentTool::new(manager))
}
