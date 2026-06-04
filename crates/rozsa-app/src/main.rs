//! JSONL stdio bridge for Rust application services.
//!
//! Currently exposes model registry listing for the TypeScript frontend bridge.
//! Related docs: `docs/model/supported-providers.md`.

use std::path::PathBuf;

use rozsa_app::model_registry::ModelRegistry;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Stdout};

/// Run the app bridge loop until stdin closes.
#[tokio::main]
async fn main() {
    let stdin = BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();
    let mut stdout = tokio::io::stdout();

    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() {
            continue;
        }
        handle_line(&line, &mut stdout).await;
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum AppBridgeInput {
    #[serde(rename = "list_models")]
    ListModels {
        id: String,
        #[serde(rename = "modelsJsonPath")]
        models_json_path: Option<String>,
        #[serde(rename = "discoverNvidia", default = "default_true")]
        discover_nvidia: bool,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum AppBridgeOutput {
    #[serde(rename = "models")]
    Models {
        id: String,
        models: Value,
        errors: Vec<String>,
    },
    #[serde(rename = "error")]
    Error {
        id: String,
        message: String,
        code: String,
    },
}

async fn handle_line(line: &str, stdout: &mut Stdout) {
    let input = match serde_json::from_str::<AppBridgeInput>(line) {
        Ok(input) => input,
        Err(error) => {
            write_output(
                stdout,
                AppBridgeOutput::Error {
                    id: "unknown".to_string(),
                    message: format!("invalid app bridge input: {error}"),
                    code: "parse_error".to_string(),
                },
            )
            .await;
            return;
        }
    };

    match input {
        AppBridgeInput::ListModels {
            id,
            models_json_path,
            discover_nvidia,
        } => {
            let mut errors = Vec::new();
            let path = models_json_path.as_ref().map(PathBuf::from);
            let mut registry =
                match ModelRegistry::from_generated_with_models_json_path(path.as_deref()) {
                    Ok(registry) => registry,
                    Err(error) => {
                        write_output(
                            stdout,
                            AppBridgeOutput::Error {
                                id,
                                message: error.to_string(),
                                code: "model_registry_error".to_string(),
                            },
                        )
                        .await;
                        return;
                    }
                };

            if discover_nvidia {
                if let Err(error) = registry.merge_nvidia_models_if_configured().await {
                    errors.push(error.to_string());
                }
            }

            write_output(
                stdout,
                AppBridgeOutput::Models {
                    id,
                    models: serde_json::json!(registry.all()),
                    errors,
                },
            )
            .await;
        }
    }
}

async fn write_output(stdout: &mut Stdout, output: AppBridgeOutput) {
    if let Ok(serialized) = serde_json::to_string(&output) {
        let _ = stdout.write_all(serialized.as_bytes()).await;
        let _ = stdout.write_all(b"\n").await;
        let _ = stdout.flush().await;
    }
}

fn default_true() -> bool {
    true
}
