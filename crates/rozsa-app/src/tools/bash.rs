use rozsa_core::tool::{Tool, ToolError, ToolExecutionMode, ToolResult};
use rozsa_model::types::ContentBlock;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::select;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

const DEFAULT_TIMEOUT_MS: u64 = 120_000; // 2 minutes
const MAX_OUTPUT_BYTES: usize = 100 * 1024; // 100KB

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BashParams {
    command: String,
    #[serde(default)]
    timeout: Option<u64>, // milliseconds
    #[serde(default)]
    description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BashDetails {
    exit_code: Option<i32>,
    truncated: bool,
    timeout_ms: u64,
}

pub struct BashTool {
    working_dir: String,
}

impl BashTool {
    pub fn new(working_dir: String) -> Self {
        Self { working_dir }
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

    async fn execute_command(
        command: &str,
        working_dir: &str,
        timeout_ms: u64,
        signal: Option<CancellationToken>,
    ) -> Result<(String, Option<i32>, bool), String> {
        // Spawn bash process
        let mut child = Command::new("bash")
            .arg("-c")
            .arg(command)
            .current_dir(working_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("Failed to spawn bash process: {}", e))?;

        let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
        let stderr = child.stderr.take().ok_or("Failed to capture stderr")?;

        let mut stdout_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();

        let mut output_buffer = Vec::new();
        let mut total_bytes = 0usize;
        let mut truncated = false;
        let mut stdout_done = false;
        let mut stderr_done = false;

        // Capture output with timeout and cancellation support
        let timeout_duration = Duration::from_millis(timeout_ms);

        let result = timeout(timeout_duration, async {
            loop {
                if stdout_done && stderr_done {
                    break;
                }

                select! {
                    biased;

                    _ = async {
                        if let Some(token) = &signal {
                            token.cancelled().await
                        } else {
                            std::future::pending::<()>().await
                        }
                    } => {
                        // Cancelled - kill the process
                        let _ = child.kill().await;
                        return Err("Command cancelled".to_string());
                    }
                    line_result = stdout_reader.next_line(), if !stdout_done => {
                        match line_result {
                            Ok(Some(line)) => {
                                let line_with_newline = format!("{}\n", line);
                                let line_bytes = line_with_newline.as_bytes();

                                if total_bytes + line_bytes.len() > MAX_OUTPUT_BYTES {
                                    truncated = true;
                                    let _ = child.kill().await;
                                    break;
                                }

                                output_buffer.extend_from_slice(line_bytes);
                                total_bytes += line_bytes.len();
                            }
                            Ok(None) => {
                                stdout_done = true;
                            }
                            Err(e) => return Err(format!("Error reading stdout: {}", e)),
                        }
                    }
                    line_result = stderr_reader.next_line(), if !stderr_done => {
                        match line_result {
                            Ok(Some(line)) => {
                                let line_with_newline = format!("{}\n", line);
                                let line_bytes = line_with_newline.as_bytes();

                                if total_bytes + line_bytes.len() > MAX_OUTPUT_BYTES {
                                    truncated = true;
                                    let _ = child.kill().await;
                                    break;
                                }

                                output_buffer.extend_from_slice(line_bytes);
                                total_bytes += line_bytes.len();
                            }
                            Ok(None) => {
                                stderr_done = true;
                            }
                            Err(e) => return Err(format!("Error reading stderr: {}", e)),
                        }
                    }
                }
            }

            // Wait for process to exit
            match child.wait().await {
                Ok(status) => Ok((
                    String::from_utf8_lossy(&output_buffer).to_string(),
                    status.code(),
                    truncated,
                )),
                Err(e) => Err(format!("Failed to wait for process: {}", e)),
            }
        })
        .await;

        match result {
            Ok(inner_result) => inner_result,
            Err(_) => {
                // Timeout - kill the process
                let _ = child.kill().await;
                let _ = child.wait().await; // Prevent zombie

                let output = String::from_utf8_lossy(&output_buffer).to_string();
                Err(format!(
                    "{}Command timed out after {}ms",
                    if output.is_empty() {
                        String::new()
                    } else {
                        format!("{}\n\n", output)
                    },
                    timeout_ms
                ))
            }
        }
    }
}

#[async_trait::async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "Execute a bash command in the current working directory. Returns combined stdout and stderr. Output is truncated at 100KB. Use timeout parameter (in milliseconds) to limit execution time (default: 120000ms / 2 minutes)."
    }

    fn label(&self) -> &str {
        "Bash"
    }

    fn parameters_schema(&self) -> &serde_json::Value {
        static SCHEMA: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Bash command to execute"
                    },
                    "timeout": {
                        "type": "number",
                        "description": "Timeout in milliseconds (default: 120000)"
                    },
                    "description": {
                        "type": "string",
                        "description": "Optional description of what this command does"
                    }
                },
                "required": ["command"]
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
        if let Some(ref token) = signal {
            if token.is_cancelled() {
                return Err(ToolError::Cancelled);
            }
        }

        let params: BashParams = serde_json::from_value(params)
            .map_err(|e| ToolError::Execution(format!("Invalid parameters: {}", e)))?;

        let timeout_ms = params.timeout.unwrap_or(DEFAULT_TIMEOUT_MS);

        let result =
            Self::execute_command(&params.command, &self.working_dir, timeout_ms, signal).await;

        match result {
            Ok((mut output, exit_code, truncated)) => {
                // Add truncation message if needed
                if truncated {
                    output.push_str(&format!(
                        "\n\n[Output truncated at {} limit]",
                        Self::format_size(MAX_OUTPUT_BYTES)
                    ));
                }

                // Add exit code message if non-zero
                let final_output = if let Some(code) = exit_code {
                    if code != 0 {
                        format!("{}\n\nCommand exited with code {}", output.trim_end(), code)
                    } else {
                        output
                    }
                } else {
                    output
                };

                let details = serde_json::to_value(BashDetails {
                    exit_code,
                    truncated,
                    timeout_ms,
                })
                .unwrap_or(json!({}));

                Ok(ToolResult {
                    content: vec![ContentBlock::Text {
                        text: final_output,
                        signature: None,
                    }],
                    details,
                    terminate: false,
                })
            }
            Err(error_msg) => {
                // Return error as tool result content (not ToolError)
                // This matches the TypeScript behavior of returning errors as text
                Ok(ToolResult {
                    content: vec![ContentBlock::Text {
                        text: error_msg,
                        signature: None,
                    }],
                    details: json!({
                        "exit_code": null,
                        "truncated": false,
                        "timeout_ms": timeout_ms,
                    }),
                    terminate: false,
                })
            }
        }
    }
}

pub fn create_bash_tool(working_dir: String) -> Box<dyn Tool> {
    Box::new(BashTool::new(working_dir))
}
