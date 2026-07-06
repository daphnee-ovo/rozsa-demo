use regex::Regex;
use rozsa_core::tool::{Tool, ToolError, ToolResult};
use rozsa_model::types::ContentBlock;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use tokio::fs;
use tokio_util::sync::CancellationToken;

const DEFAULT_MAX_MATCHES: usize = 100;
const MAX_LINE_LENGTH: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GrepParams {
    pattern: String,
    #[serde(default = "default_path")]
    path: String,
    #[serde(default)]
    include: Option<String>,
}

fn default_path() -> String {
    ".".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GrepDetails {
    truncated: bool,
    match_limit_reached: Option<usize>,
    lines_truncated: bool,
    total_matches: usize,
}

pub struct GrepTool;

impl GrepTool {
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
        matches!(name, "node_modules" | "target" | ".git" | "dist" | "build")
    }

    fn truncate_line(line: &str) -> (String, bool) {
        if line.len() <= MAX_LINE_LENGTH {
            (line.to_string(), false)
        } else {
            (format!("{}...", &line[..MAX_LINE_LENGTH]), true)
        }
    }

    async fn search_in_file(
        file_path: &Path,
        regex: &Regex,
        relative_to: &Path,
        matches: &mut Vec<String>,
        lines_truncated: &mut bool,
        limit: usize,
    ) -> Result<bool, std::io::Error> {
        if matches.len() >= limit {
            return Ok(true);
        }

        let content = match fs::read_to_string(file_path).await {
            Ok(c) => c,
            Err(_) => return Ok(false), // Skip binary or unreadable files
        };

        let relative_path = file_path
            .strip_prefix(relative_to)
            .unwrap_or(file_path)
            .to_string_lossy();

        for (line_num, line) in content.lines().enumerate() {
            if matches.len() >= limit {
                return Ok(true);
            }

            if regex.is_match(line) {
                let (truncated_line, was_truncated) = Self::truncate_line(line);
                if was_truncated {
                    *lines_truncated = true;
                }
                matches.push(format!(
                    "{}:{}: {}",
                    relative_path,
                    line_num + 1,
                    truncated_line
                ));
            }
        }

        Ok(false)
    }

    fn search_directory<'a>(
        dir_path: &'a Path,
        regex: &'a Regex,
        include_pattern: Option<&'a Regex>,
        relative_to: &'a Path,
        matches: &'a mut Vec<String>,
        lines_truncated: &'a mut bool,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<bool, std::io::Error>> + Send + 'a>> {
        Box::pin(async move {
            if matches.len() >= limit {
                return Ok(true);
            }

            let mut read_dir = fs::read_dir(dir_path).await?;

            while let Some(entry) = read_dir.next_entry().await? {
                if matches.len() >= limit {
                    return Ok(true);
                }

                let path = entry.path();

                if Self::should_skip_path(&path) {
                    continue;
                }

                let metadata = entry.metadata().await?;

                if metadata.is_dir() {
                    if Self::search_directory(
                        &path,
                        regex,
                        include_pattern,
                        relative_to,
                        matches,
                        lines_truncated,
                        limit,
                    )
                    .await?
                    {
                        return Ok(true);
                    }
                } else if metadata.is_file() {
                    // Check include pattern if specified
                    if let Some(include_regex) = include_pattern {
                        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                        if !include_regex.is_match(file_name) {
                            continue;
                        }
                    }

                    if Self::search_in_file(
                        &path,
                        regex,
                        relative_to,
                        matches,
                        lines_truncated,
                        limit,
                    )
                    .await?
                    {
                        return Ok(true);
                    }
                }
            }

            Ok(false)
        })
    }

    async fn grep_search(
        pattern: &str,
        search_path: &str,
        include_pattern: Option<&str>,
    ) -> Result<(String, GrepDetails), String> {
        // Compile regex pattern
        let regex = Regex::new(pattern).map_err(|e| format!("Invalid regex pattern: {}", e))?;

        // Compile include pattern if specified
        let include_regex = if let Some(inc) = include_pattern {
            Some(Regex::new(inc).map_err(|e| format!("Invalid include pattern: {}", e))?)
        } else {
            None
        };

        let path = Path::new(search_path);
        if !path.exists() {
            return Err(format!("Path not found: {}", search_path));
        }

        let metadata = fs::metadata(path)
            .await
            .map_err(|e| format!("Cannot access path: {}", e))?;

        let mut matches = Vec::new();
        let mut lines_truncated = false;
        let limit = DEFAULT_MAX_MATCHES;

        let limit_reached = if metadata.is_dir() {
            Self::search_directory(
                path,
                &regex,
                include_regex.as_ref(),
                path,
                &mut matches,
                &mut lines_truncated,
                limit,
            )
            .await
            .map_err(|e| format!("Error searching directory: {}", e))?
        } else if metadata.is_file() {
            // Search single file
            let parent = path.parent().unwrap_or(Path::new("."));
            Self::search_in_file(
                path,
                &regex,
                parent,
                &mut matches,
                &mut lines_truncated,
                limit,
            )
            .await
            .map_err(|e| format!("Error searching file: {}", e))?
        } else {
            return Err(format!(
                "Path is neither file nor directory: {}",
                search_path
            ));
        };

        if matches.is_empty() {
            return Ok((
                "No matches found".to_string(),
                GrepDetails {
                    truncated: false,
                    match_limit_reached: None,
                    lines_truncated: false,
                    total_matches: 0,
                },
            ));
        }

        let total_matches = matches.len();
        let mut result = matches.join("\n");

        // Add notices
        let mut notices = Vec::new();
        if limit_reached {
            notices.push(format!(
                "{} matches limit reached. Use grep with a more specific pattern or path",
                limit
            ));
        }
        if lines_truncated {
            notices.push(format!(
                "Some lines truncated to {} chars. Use read tool to see full lines",
                MAX_LINE_LENGTH
            ));
        }

        if !notices.is_empty() {
            result.push_str("\n\n[");
            result.push_str(&notices.join(". "));
            result.push(']');
        }

        let details = GrepDetails {
            truncated: limit_reached,
            match_limit_reached: if limit_reached { Some(limit) } else { None },
            lines_truncated,
            total_matches,
        };

        Ok((result, details))
    }
}

impl Default for GrepTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn description(&self) -> &str {
        "Search file contents for a regex pattern. Returns matching lines with file:line:content format. Skips hidden files, .git, node_modules, target. Output is truncated to 100 matches. Long lines are truncated to 500 chars."
    }

    fn label(&self) -> &str {
        "grep"
    }

    fn parameters_schema(&self) -> &serde_json::Value {
        static SCHEMA: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Search pattern (regex)"
                    },
                    "path": {
                        "type": "string",
                        "description": "Directory or file to search (default: current directory)"
                    },
                    "include": {
                        "type": "string",
                        "description": "Filter files by regex pattern (e.g., '.*\\.ts$' for TypeScript files)"
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

        let params: GrepParams = serde_json::from_value(params)
            .map_err(|e| ToolError::Execution(format!("Invalid parameters: {}", e)))?;

        let result =
            Self::grep_search(&params.pattern, &params.path, params.include.as_deref()).await;

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

pub fn create_grep_tool() -> Box<dyn Tool> {
    Box::new(GrepTool::new())
}
