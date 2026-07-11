use super::file_lock;
use rozsa_core::tool::{Tool, ToolError, ToolResult};
use rozsa_model::types::ContentBlock;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::Path;
use tokio::fs;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EditParams {
    #[serde(alias = "path")]
    file_path: String,
    old_string: String,
    new_string: String,
    #[serde(default)]
    replace_all: bool,
}

pub struct EditTool;

impl EditTool {
    pub fn new() -> Self {
        Self
    }

    /// Normalize whitespace for fuzzy matching: collapse runs of whitespace to single space, trim lines
    fn normalize_whitespace(s: &str) -> String {
        s.lines()
            .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Try to find old_string in content using fuzzy matching strategies
    /// Returns match_strategy if found
    fn fuzzy_match(content: &str, old_string: &str) -> Option<&'static str> {
        // Strategy 1: Exact match
        if content.contains(old_string) {
            return Some("exact");
        }

        // Strategy 2: Whitespace-normalized match
        let normalized_content = Self::normalize_whitespace(content);
        let normalized_old = Self::normalize_whitespace(old_string);

        if normalized_content.contains(&normalized_old) {
            return Some("whitespace-normalized");
        }

        None
    }

    async fn edit_file(
        file_path: &str,
        old_string: &str,
        new_string: &str,
        replace_all: bool,
    ) -> Result<String, String> {
        let path = Path::new(file_path);
        let path_buf = path.to_path_buf();

        file_lock::with_file_lock(&path_buf, async move {
            // Check if file exists and is readable
            if !path.exists() {
                return Err(format!("File not found: {}", file_path));
            }

            if !path.is_file() {
                return Err(format!("Not a file: {}", file_path));
            }

            // Read file content
            let mut content = fs::read_to_string(file_path)
                .await
                .map_err(|e| match e.kind() {
                    std::io::ErrorKind::PermissionDenied => {
                        format!("Permission denied: {}", file_path)
                    }
                    std::io::ErrorKind::InvalidData => {
                        format!("Binary file or invalid UTF-8: {}", file_path)
                    }
                    _ => format!("Failed to read file: {}", e),
                })?;

            // Detect and strip BOM if present
            let has_bom = content.starts_with('\u{feff}');
            if has_bom {
                content = content.trim_start_matches('\u{feff}').to_string();
            }

            // Detect line ending style (CRLF vs LF)
            let uses_crlf = content.contains("\r\n");

            // Normalize to LF for matching
            let normalized_content = if uses_crlf {
                content.replace("\r\n", "\n")
            } else {
                content.clone()
            };

            let normalized_old_string = if uses_crlf {
                old_string.replace("\r\n", "\n")
            } else {
                old_string.to_string()
            };

            // Try fuzzy matching and determine which content to work with
            let match_strategy = Self::fuzzy_match(&normalized_content, &normalized_old_string);

            if match_strategy.is_none() {
                return Err(format!(
                    "Could not find old_string in {}. The text must match exactly including all whitespace and newlines.",
                    file_path
                ));
            }

            let match_strategy = match_strategy.unwrap();

            // For fuzzy matching, work with whitespace-normalized content
            let (working_content, working_old_string) = if match_strategy == "whitespace-normalized" {
                (
                    Self::normalize_whitespace(&normalized_content),
                    Self::normalize_whitespace(&normalized_old_string),
                )
            } else {
                (normalized_content.clone(), normalized_old_string.clone())
            };

            // Count occurrences in working content
            let occurrences = working_content.matches(&working_old_string).count();

            // Handle replacement based on replace_all flag
            let new_normalized_content = if replace_all {
                // Replace all occurrences
                working_content.replace(&working_old_string, new_string)
            } else {
                // Check for uniqueness
                if occurrences > 1 {
                    return Err(format!(
                        "Found {} occurrences of old_string in {}. The text must be unique or use replace_all=true.",
                        occurrences, file_path
                    ));
                }
                // Replace single occurrence
                working_content.replacen(&working_old_string, new_string, 1)
            };

            // Check if content actually changed
            if working_content == new_normalized_content {
                return Err(format!(
                    "No changes made to {}. The replacement produced identical content.",
                    file_path
                ));
            }

            // Convert back to original line ending style if needed
            let mut final_content = if uses_crlf {
                new_normalized_content.replace('\n', "\r\n")
            } else {
                new_normalized_content
            };

            // Restore BOM if present
            if has_bom {
                final_content = format!("\u{feff}{}", final_content);
            }

            // Write to temporary file first (atomic write)
            let tmp_path = path.with_extension("tmp");

            fs::write(&tmp_path, &final_content)
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
            fs::rename(&tmp_path, path)
                .await
                .map_err(|e| {
                    // Clean up tmp file on rename failure
                    let _ = std::fs::remove_file(&tmp_path);
                    match e.kind() {
                        std::io::ErrorKind::PermissionDenied => {
                            format!("Permission denied: cannot rename temporary file to {}", file_path)
                        }
                        _ => format!("Failed to rename temporary file: {}", e),
                    }
                })?;

            let replacement_count = if replace_all { occurrences } else { 1 };

            let strategy_msg = if match_strategy != "exact" {
                format!(" (using {} match)", match_strategy)
            } else {
                String::new()
            };

            Ok(format!(
                "Successfully replaced {} occurrence(s) in {}{}",
                replacement_count, file_path, strategy_msg
            ))
        })
        .await
    }
}

impl Default for EditTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Tool for EditTool {
    fn name(&self) -> &str {
        "edit"
    }

    fn description(&self) -> &str {
        "Edit an existing file by replacing old_string with new_string. By default, old_string must be unique in the file. Set replace_all=true to replace all occurrences."
    }

    fn label(&self) -> &str {
        "Edit"
    }

    fn parameters_schema(&self) -> &serde_json::Value {
        static SCHEMA: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Path to the file to edit (relative or absolute)"
                    },
                    "old_string": {
                        "type": "string",
                        "description": "Exact text to find and replace. Must match exactly including all whitespace and newlines."
                    },
                    "new_string": {
                        "type": "string",
                        "description": "Text to replace old_string with"
                    },
                    "replace_all": {
                        "type": "boolean",
                        "description": "If true, replace all occurrences. If false (default), old_string must be unique.",
                        "default": false
                    }
                },
                "required": ["file_path", "old_string", "new_string"]
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

        let params: EditParams = serde_json::from_value(params)
            .map_err(|e| ToolError::Execution(format!("Invalid parameters: {}", e)))?;

        let before = super::file_delta::read_text_if_present(Path::new(&params.file_path));
        let result = Self::edit_file(
            &params.file_path,
            &params.old_string,
            &params.new_string,
            params.replace_all,
        )
        .await;

        match result {
            Ok(message) => {
                let after = super::file_delta::read_text_if_present(Path::new(&params.file_path));
                let delta = super::file_delta::build_file_delta(
                    params.file_path.clone(),
                    before,
                    after,
                );
                let replacement_count = if params.replace_all {
                    // Count occurrences in original file (we don't have it here, so we'll use a placeholder)
                    // In a real implementation, we'd need to read the file again or pass this info from edit_file
                    "multiple".to_string()
                } else {
                    "1".to_string()
                };

                Ok(ToolResult {
                    content: vec![ContentBlock::Text {
                        text: message,
                        signature: None,
                    }],
                    details: json!({
                        "file_path": params.file_path,
                        "changed_files": [params.file_path],
                        "success": true,
                        "replacements": replacement_count,
                        "file_deltas": delta.into_iter().collect::<Vec<_>>(),
                        "capture_complete": true,
                    }),
                    terminate: false,
                })
            }
            Err(error_msg) => Err(ToolError::Execution(error_msg)),
        }
    }
}

pub fn create_edit_tool() -> Box<dyn Tool> {
    Box::new(EditTool::new())
}
