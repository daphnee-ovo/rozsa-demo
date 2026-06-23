use rozsa_core::tool::{Tool, ToolError, ToolResult};
use rozsa_model::types::ContentBlock;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::Path;
use tokio::fs;
use tokio_util::sync::CancellationToken;

const DEFAULT_MAX_ENTRIES: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LsParams {
    #[serde(default = "default_path")]
    path: String,
    #[serde(default)]
    limit: Option<usize>,
}

fn default_path() -> String {
    ".".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LsDetails {
    truncated: bool,
    entry_limit_reached: Option<usize>,
    total_entries: usize,
    output_entries: usize,
}

pub struct LsTool;

impl LsTool {
    pub fn new() -> Self {
        Self
    }

    async fn list_directory(
        dir_path: &str,
        limit: Option<usize>,
    ) -> Result<(String, LsDetails), String> {
        let path = Path::new(dir_path);

        // Check if path exists
        if !path.exists() {
            return Err(format!("Path not found: {}", dir_path));
        }

        // Check if path is a directory
        let metadata = fs::metadata(path)
            .await
            .map_err(|e| format!("Cannot access path: {}", e))?;

        if !metadata.is_dir() {
            return Err(format!("Not a directory: {}", dir_path));
        }

        // Read directory entries
        let mut read_dir = fs::read_dir(path)
            .await
            .map_err(|e| format!("Cannot read directory: {}", e))?;

        let mut entries = Vec::new();
        while let Some(entry) = read_dir
            .next_entry()
            .await
            .map_err(|e| format!("Error reading directory entry: {}", e))?
        {
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy().to_string();

            let metadata = entry.metadata().await.ok();
            let is_dir = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);

            entries.push((name, is_dir));
        }

        // Sort alphabetically, case-insensitive
        entries.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

        let total_entries = entries.len();
        let effective_limit = limit.unwrap_or(DEFAULT_MAX_ENTRIES);
        let mut entry_limit_reached = false;

        // Apply entry limit
        let limited_entries = if entries.len() > effective_limit {
            entry_limit_reached = true;
            &entries[..effective_limit]
        } else {
            &entries[..]
        };

        // Format output with directory indicators
        let output: Vec<String> = limited_entries
            .iter()
            .map(|(name, is_dir)| {
                if *is_dir {
                    format!("{}/", name)
                } else {
                    name.clone()
                }
            })
            .collect();

        if output.is_empty() {
            return Ok((
                "(empty directory)".to_string(),
                LsDetails {
                    truncated: false,
                    entry_limit_reached: None,
                    total_entries: 0,
                    output_entries: 0,
                },
            ));
        }

        let mut result = output.join("\n");
        let output_entries = output.len();

        // Add notices for truncation
        if entry_limit_reached {
            result.push_str(&format!(
                "\n\n[{} entries limit reached. Use limit={} for more]",
                effective_limit,
                effective_limit * 2
            ));
        }

        let details = LsDetails {
            truncated: entry_limit_reached,
            entry_limit_reached: if entry_limit_reached {
                Some(effective_limit)
            } else {
                None
            },
            total_entries,
            output_entries,
        };

        Ok((result, details))
    }
}

impl Default for LsTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Tool for LsTool {
    fn name(&self) -> &str {
        "ls"
    }

    fn description(&self) -> &str {
        "List directory contents. Returns entries sorted alphabetically, with '/' suffix for directories. Includes dotfiles. Output is truncated to 500 entries (or specified limit)."
    }

    fn label(&self) -> &str {
        "ls"
    }

    fn parameters_schema(&self) -> &serde_json::Value {
        static SCHEMA: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory to list (default: current directory)"
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum number of entries to return (default: 500)"
                    }
                },
                "required": []
            })
        })
    }

    async fn execute(
        &self,
        _tool_call_id: &str,
        params: serde_json::Value,
        signal: Option<CancellationToken>,
        _on_update: Option<&(dyn Fn(ToolResult) + Send + Sync)>,
    ) -> Result<ToolResult, ToolError> {
        if let Some(ref token) = signal {
            if token.is_cancelled() {
                return Err(ToolError::Cancelled);
            }
        }

        let params: LsParams = serde_json::from_value(params)
            .map_err(|e| ToolError::Execution(format!("Invalid parameters: {}", e)))?;

        let result = Self::list_directory(&params.path, params.limit).await;

        match result {
            Ok((output, details)) => {
                let details_json = serde_json::to_value(details).unwrap_or(json!({}));

                Ok(ToolResult {
                    content: vec![ContentBlock::Text {
                        text: output,
                        signature: None,
                    }],
                    details: details_json,
                    terminate: false,
                })
            }
            Err(error_msg) => Ok(ToolResult {
                content: vec![ContentBlock::Text {
                    text: error_msg,
                    signature: None,
                }],
                details: json!({}),
                terminate: false,
            }),
        }
    }
}

pub fn create_ls_tool() -> Box<dyn Tool> {
    Box::new(LsTool::new())
}
