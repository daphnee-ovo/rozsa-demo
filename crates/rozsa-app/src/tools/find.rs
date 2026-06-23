use regex::Regex;
use rozsa_core::tool::{Tool, ToolError, ToolResult};
use rozsa_model::types::ContentBlock;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::Path;
use std::pin::Pin;
use std::future::Future;
use tokio::fs;
use tokio_util::sync::CancellationToken;

const DEFAULT_MAX_RESULTS: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FindParams {
    pattern: String,
    #[serde(default = "default_path")]
    path: String,
}

fn default_path() -> String {
    ".".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FindDetails {
    truncated: bool,
    result_limit_reached: Option<usize>,
    total_results: usize,
}

pub struct FindTool;

impl FindTool {
    pub fn new() -> Self {
        Self
    }

    fn should_skip_path(path: &Path) -> bool {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Skip hidden files and directories
        if name.starts_with('.') && name != "." && name != ".." {
            return true;
        }

        // Skip common large directories
        matches!(
            name,
            "node_modules" | "target" | ".git" | "dist" | "build"
        )
    }

    fn find_files<'a>(
        dir_path: &'a Path,
        regex: &'a Regex,
        relative_to: &'a Path,
        results: &'a mut Vec<String>,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<bool, std::io::Error>> + Send + 'a>> {
        Box::pin(async move {
            if results.len() >= limit {
                return Ok(true);
            }

            let mut read_dir = fs::read_dir(dir_path).await?;

            while let Some(entry) = read_dir.next_entry().await? {
                if results.len() >= limit {
                    return Ok(true);
                }

                let path = entry.path();

                if Self::should_skip_path(&path) {
                    continue;
                }

                let metadata = entry.metadata().await?;

                if metadata.is_dir() {
                    // Recursively search subdirectories
                    if Self::find_files(&path, regex, relative_to, results, limit).await? {
                        return Ok(true);
                    }
                } else if metadata.is_file() {
                    // Check if filename matches pattern
                    if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                        if regex.is_match(file_name) {
                            let relative_path = path
                                .strip_prefix(relative_to)
                                .unwrap_or(&path)
                                .to_string_lossy()
                                .to_string();
                            results.push(relative_path);
                        }
                    }
                }
            }

            Ok(false)
        })
    }

    async fn find_search(
        pattern: &str,
        search_path: &str,
    ) -> Result<(String, FindDetails), String> {
        // Compile regex pattern
        let regex = Regex::new(pattern).map_err(|e| format!("Invalid regex pattern: {}", e))?;

        let path = Path::new(search_path);
        if !path.exists() {
            return Err(format!("Path not found: {}", search_path));
        }

        let metadata = fs::metadata(path)
            .await
            .map_err(|e| format!("Cannot access path: {}", e))?;

        if !metadata.is_dir() {
            return Err(format!("Not a directory: {}", search_path));
        }

        let mut results = Vec::new();
        let limit = DEFAULT_MAX_RESULTS;

        let limit_reached =
            Self::find_files(path, &regex, path, &mut results, limit)
                .await
                .map_err(|e| format!("Error searching directory: {}", e))?;

        if results.is_empty() {
            return Ok((
                "No files found matching pattern".to_string(),
                FindDetails {
                    truncated: false,
                    result_limit_reached: None,
                    total_results: 0,
                },
            ));
        }

        let total_results = results.len();
        let mut result = results.join("\n");

        // Add notice if limit reached
        if limit_reached {
            result.push_str(&format!(
                "\n\n[{} results limit reached. Use a more specific pattern or path]",
                limit
            ));
        }

        let details = FindDetails {
            truncated: limit_reached,
            result_limit_reached: if limit_reached { Some(limit) } else { None },
            total_results,
        };

        Ok((result, details))
    }
}

impl Default for FindTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Tool for FindTool {
    fn name(&self) -> &str {
        "find"
    }

    fn description(&self) -> &str {
        "Find files by name pattern (regex). Returns matching file paths relative to the search directory. Skips hidden files, .git, node_modules, target. Output is truncated to 200 files."
    }

    fn label(&self) -> &str {
        "find"
    }

    fn parameters_schema(&self) -> &serde_json::Value {
        static SCHEMA: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "File name pattern (regex, e.g., '.*\\.rs$' for Rust files)"
                    },
                    "path": {
                        "type": "string",
                        "description": "Directory to search (default: current directory)"
                    }
                },
                "required": ["pattern"]
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

        let params: FindParams = serde_json::from_value(params)
            .map_err(|e| ToolError::Execution(format!("Invalid parameters: {}", e)))?;

        let result = Self::find_search(&params.pattern, &params.path).await;

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

pub fn create_find_tool() -> Box<dyn Tool> {
    Box::new(FindTool::new())
}
