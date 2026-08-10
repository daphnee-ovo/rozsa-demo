// FrameworkTree
// read.rs
// ├── struct ReadParams
// ├── struct TruncationDetails
// ├── struct ReadTool
// ├── impl ReadTool
// ├── new()
// ├── format_line_with_number()
// ├── format_size()
// ├── read_and_format()
// ├── truncate_head()
// ├── impl ReadTool
// ├── default()
// ├── impl ReadTool
// ├── name()
// ├── description()
// ├── label()
// ├── parameters_schema()
// ├── execute()
// ├── create_read_tool()
// └── resolve_skill_path_vars()

use rozsa_core::tool::{Tool, ToolError, ToolResult};
use rozsa_model::types::ContentBlock;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::Path;
use tokio::fs;
use tokio_util::sync::CancellationToken;

const DEFAULT_MAX_LINES: usize = 2000;
const DEFAULT_MAX_BYTES: usize = 50 * 1024; // 50KB

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReadParams {
    #[serde(alias = "path")]
    file_path: String,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TruncationDetails {
    truncated: bool,
    truncated_by: Option<String>,
    total_lines: usize,
    output_lines: usize,
    first_line_exceeds_limit: bool,
    max_lines: usize,
    max_bytes: usize,
}

pub struct ReadTool;

impl ReadTool {
    pub fn new() -> Self {
        Self
    }

    fn format_line_with_number(line_num: usize, content: &str) -> String {
        format!("{:>6}\t{}", line_num, content)
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

    async fn read_and_format(
        file_path: &str,
        offset: Option<usize>,
        limit: Option<usize>,
    ) -> Result<(String, TruncationDetails), String> {
        let resolved = resolve_skill_path_vars(file_path);
        let file_path = resolved.as_deref().unwrap_or(file_path);
        let path = Path::new(file_path);

        // Check if file exists and is readable
        if !path.exists() {
            return Err(format!("File not found: {}", file_path));
        }

        if !path.is_file() {
            return Err(format!("Not a file: {}", file_path));
        }

        // Read file content
        let content = fs::read_to_string(file_path)
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

        let all_lines: Vec<&str> = content.lines().collect();
        let total_lines = all_lines.len();

        // Apply offset
        let start_line = offset.unwrap_or(1).saturating_sub(1);
        let start_line_display = start_line + 1;

        if start_line >= total_lines && total_lines > 0 {
            return Err(format!(
                "Offset {} is beyond end of file ({} lines total)",
                offset.unwrap_or(1),
                total_lines
            ));
        }

        // Determine end line based on user limit
        let selected_lines = if let Some(limit) = limit {
            let end_line = (start_line + limit).min(total_lines);
            &all_lines[start_line..end_line]
        } else {
            &all_lines[start_line..]
        };

        // Apply truncation logic
        let (output_lines, truncation) =
            Self::truncate_head(selected_lines, DEFAULT_MAX_LINES, DEFAULT_MAX_BYTES);

        // Check if first line exceeds limit
        if truncation.first_line_exceeds_limit {
            let first_line_bytes = selected_lines.first().map(|l| l.len()).unwrap_or(0);
            return Err(format!(
                "[Line {} is {}, exceeds {} limit. Use bash: sed -n '{}p' {} | head -c {}]",
                start_line_display,
                Self::format_size(first_line_bytes),
                Self::format_size(DEFAULT_MAX_BYTES),
                start_line_display,
                file_path,
                DEFAULT_MAX_BYTES
            ));
        }

        // Format output with line numbers
        let formatted_lines: Vec<String> = output_lines
            .iter()
            .enumerate()
            .map(|(i, line)| Self::format_line_with_number(start_line_display + i, line))
            .collect();

        let mut output = formatted_lines.join("\n");

        // Add continuation message if needed
        if truncation.truncated {
            let end_line_display = start_line_display + truncation.output_lines - 1;
            let next_offset = end_line_display + 1;
            if let Some(by) = &truncation.truncated_by {
                if by == "lines" {
                    output.push_str(&format!(
                        "\n\n[Showing lines {}-{} of {}. Use offset={} to continue.]",
                        start_line_display, end_line_display, total_lines, next_offset
                    ));
                } else {
                    output.push_str(&format!(
                        "\n\n[Showing lines {}-{} of {} ({} limit). Use offset={} to continue.]",
                        start_line_display,
                        end_line_display,
                        total_lines,
                        Self::format_size(DEFAULT_MAX_BYTES),
                        next_offset
                    ));
                }
            }
        } else if let Some(_user_limit) = limit {
            let lines_shown = output_lines.len();
            if start_line + lines_shown < total_lines {
                let remaining = total_lines - (start_line + lines_shown);
                let next_offset = start_line + lines_shown + 1;
                output.push_str(&format!(
                    "\n\n[{} more lines in file. Use offset={} to continue.]",
                    remaining, next_offset
                ));
            }
        }

        Ok((output, truncation))
    }

    fn truncate_head<'a>(
        lines: &'a [&'a str],
        max_lines: usize,
        max_bytes: usize,
    ) -> (Vec<&'a str>, TruncationDetails) {
        let total_lines = lines.len();

        // Check if first line exceeds byte limit
        if let Some(first) = lines.first() {
            if first.len() > max_bytes {
                return (
                    vec![],
                    TruncationDetails {
                        truncated: true,
                        truncated_by: Some("bytes".to_string()),
                        total_lines,
                        output_lines: 0,
                        first_line_exceeds_limit: true,
                        max_lines,
                        max_bytes,
                    },
                );
            }
        }

        let mut output_lines = Vec::new();
        let mut current_bytes = 0;
        let mut truncated_by = None;

        for (i, line) in lines.iter().enumerate() {
            if i >= max_lines {
                truncated_by = Some("lines".to_string());
                break;
            }

            let line_bytes = line.len() + if i > 0 { 1 } else { 0 }; // +1 for newline

            if current_bytes + line_bytes > max_bytes {
                truncated_by = Some("bytes".to_string());
                break;
            }

            output_lines.push(*line);
            current_bytes += line_bytes;
        }

        let output_count = output_lines.len();
        let truncated = output_count < total_lines;

        (
            output_lines,
            TruncationDetails {
                truncated,
                truncated_by,
                total_lines,
                output_lines: output_count,
                first_line_exceeds_limit: false,
                max_lines,
                max_bytes,
            },
        )
    }
}

impl Default for ReadTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Tool for ReadTool {
    fn name(&self) -> &str {
        "read"
    }

    fn description(&self) -> &str {
        "Read the contents of a file. Output is formatted with line numbers (like cat -n). For large files, output is truncated to 2000 lines or 50KB (whichever is hit first). Use offset/limit parameters to read specific ranges. When you need the full file, continue with offset until complete."
    }

    fn label(&self) -> &str {
        "Read"
    }

    fn parameters_schema(&self) -> &serde_json::Value {
        static SCHEMA: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Path to the file to read (relative or absolute)"
                    },
                    "offset": {
                        "type": "number",
                        "description": "Line number to start reading from (1-indexed)"
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum number of lines to read"
                    }
                },
                "required": ["file_path"]
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

        let params: ReadParams = serde_json::from_value(params)
            .map_err(|e| ToolError::Execution(format!("Invalid parameters: {}", e)))?;

        let result = Self::read_and_format(&params.file_path, params.offset, params.limit).await;

        match result {
            Ok((output, truncation)) => {
                let details = serde_json::to_value(truncation).unwrap_or(json!({}));

                Ok(ToolResult {
                    content: vec![ContentBlock::Text {
                        text: output,
                        signature: None,
                    }],
                    details,
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

pub fn create_read_tool() -> Box<dyn Tool> {
    Box::new(ReadTool::new())
}

/// Resolve `$PROJECT_SKILLS`, `$AGENTS_SKILLS`, and `$USER_SKILLS` path variables.
/// Returns Some(resolved_path) if a variable was found, None otherwise.
fn resolve_skill_path_vars(path: &str) -> Option<String> {
    let (var, rest) = if let Some(rest) = path.strip_prefix("$PROJECT_SKILLS") {
        ("$PROJECT_SKILLS", rest)
    } else if let Some(rest) = path.strip_prefix("$AGENTS_SKILLS") {
        ("$AGENTS_SKILLS", rest)
    } else {
        ("$USER_SKILLS", path.strip_prefix("$USER_SKILLS")?)
    };

    let rest = rest.strip_prefix('/').unwrap_or(rest);
    let cwd = std::env::current_dir().ok()?;
    let roots = crate::config_paths::ConfigRoots::discover(&cwd).ok()?;
    let [user_skills, project_skills] = roots.skill_dirs();

    let base = match var {
        "$PROJECT_SKILLS" => project_skills,
        "$AGENTS_SKILLS" => roots.agents_skills_dir()?.to_path_buf(),
        "$USER_SKILLS" => user_skills,
        _ => unreachable!(),
    };

    Some(base.join(rest).to_string_lossy().into_owned())
}
