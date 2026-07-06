use super::file_lock;
use rozsa_core::tool::{Tool, ToolError, ToolResult};
use rozsa_model::types::ContentBlock;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::Path;
use tokio::fs;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WriteParams {
    #[serde(alias = "path")]
    file_path: String,
    content: String,
}

pub struct WriteTool;

impl WriteTool {
    pub fn new() -> Self {
        Self
    }

    async fn write_file(file_path: &str, content: &str) -> Result<String, String> {
        let path = Path::new(file_path);
        let path_buf = path.to_path_buf();

        file_lock::with_file_lock(&path_buf, async move {
            // Create parent directories if needed
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent)
                        .await
                        .map_err(|e| match e.kind() {
                            std::io::ErrorKind::PermissionDenied => {
                                format!(
                                    "Permission denied: cannot create parent directory for {}",
                                    file_path
                                )
                            }
                            _ => format!("Failed to create parent directory: {}", e),
                        })?;
                }
            }

            // Write to temporary file first (atomic write)
            let tmp_path = path.with_extension("tmp");

            fs::write(&tmp_path, content)
                .await
                .map_err(|e| match e.kind() {
                    std::io::ErrorKind::PermissionDenied => {
                        format!("Permission denied: {}", file_path)
                    }
                    std::io::ErrorKind::OutOfMemory => {
                        format!("Out of memory while writing to {}", file_path)
                    }
                    _ => format!("Failed to write file: {}", e),
                })?;

            // Atomic rename
            fs::rename(&tmp_path, path).await.map_err(|e| {
                // Clean up tmp file on rename failure
                let _ = std::fs::remove_file(&tmp_path);
                match e.kind() {
                    std::io::ErrorKind::PermissionDenied => {
                        format!(
                            "Permission denied: cannot rename temporary file to {}",
                            file_path
                        )
                    }
                    _ => format!("Failed to rename temporary file: {}", e),
                }
            })?;

            // Count lines
            let line_count = content.lines().count();

            Ok(format!(
                "Successfully wrote {} to {} ({} lines)",
                Self::format_size(content.len()),
                file_path,
                line_count
            ))
        })
        .await
    }

    fn format_size(bytes: usize) -> String {
        if bytes < 1024 {
            format!("{}B", bytes)
        } else if bytes < 1024 * 1024 {
            format!("{:.1}KB", bytes as f64 / 1024.0)
        } else {
            format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
        }
    }
}

impl Default for WriteTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Tool for WriteTool {
    fn name(&self) -> &str {
        "write"
    }

    fn description(&self) -> &str {
        "Write content to a file. Creates the file if it doesn't exist, overwrites if it does. Automatically creates parent directories."
    }

    fn label(&self) -> &str {
        "Write"
    }

    fn parameters_schema(&self) -> &serde_json::Value {
        static SCHEMA: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Path to the file to write (relative or absolute)"
                    },
                    "content": {
                        "type": "string",
                        "description": "Content to write to the file"
                    }
                },
                "required": ["file_path", "content"]
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

        let params: WriteParams = serde_json::from_value(params)
            .map_err(|e| ToolError::Execution(format!("Invalid parameters: {}", e)))?;

        let result = Self::write_file(&params.file_path, &params.content).await;

        match result {
            Ok(message) => Ok(ToolResult {
                content: vec![ContentBlock::Text {
                    text: message,
                    signature: None,
                }],
                details: json!({
                    "file_path": params.file_path,
                    "bytes_written": params.content.len(),
                    "line_count": params.content.lines().count(),
                }),
                terminate: false,
            }),
            Err(error_msg) => Err(ToolError::Execution(error_msg)),
        }
    }
}

pub fn create_write_tool() -> Box<dyn Tool> {
    Box::new(WriteTool::new())
}
