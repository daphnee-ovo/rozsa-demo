//! JSONL stdio bridge executable for the Rust model layer.

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use rozsa_model::protocol::{
    BridgeMethod, BridgeOutput, bridge_error, event_to_bridge_output, parse_input_line,
    provider_request,
};
use rozsa_model::providers::register_builtin_providers;
use rozsa_model::registry::get_provider;
use tokio::io::Stdout;

/// Run the JSONL bridge loop until stdin closes.
#[tokio::main]
async fn main() {
    register_builtin_providers();
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

/// Handle one JSONL bridge input line and write response lines as events arrive.
async fn handle_line(line: &str, stdout: &mut Stdout) {
    let input = match parse_input_line(line) {
        Ok(input) => input,
        Err(error) => {
            write_bridge_output(stdout, bridge_error("unknown", error, "parse_error")).await;
            return;
        }
    };
    let request = match provider_request(input) {
        Ok(Some(request)) => request,
        Ok(None) => return,
        Err(error) => {
            write_bridge_output(stdout, bridge_error("unknown", error, "request_error")).await;
            return;
        }
    };
    let Some(provider) = get_provider(&request.model.api) else {
        write_bridge_output(
            stdout,
            bridge_error(
                &request.id,
                format!("No provider registered for api: {:?}", request.model.api),
                "unsupported_api",
            ),
        )
        .await;
        return;
    };

    let mut stream = match request.method {
        BridgeMethod::Stream | BridgeMethod::StreamSimple => {
            provider.stream_simple(&request.model, &request.context, &request.options)
        }
    };

    while let Some(event) = stream.next().await {
        write_bridge_output(stdout, event_to_bridge_output(&request.id, event)).await;
    }
}

/// Serialize one bridge output and flush it to stdout immediately.
async fn write_bridge_output(stdout: &mut Stdout, output: BridgeOutput) {
    match serde_json::to_string(&output) {
        Ok(serialized) => {
            let _ = stdout.write_all(serialized.as_bytes()).await;
            let _ = stdout.write_all(b"\n").await;
            let _ = stdout.flush().await;
        }
        Err(error) => {
            let fallback = bridge_error("unknown", error, "serialize_error");
            if let Ok(serialized) = serde_json::to_string(&fallback) {
                let _ = stdout.write_all(serialized.as_bytes()).await;
                let _ = stdout.write_all(b"\n").await;
                let _ = stdout.flush().await;
            }
        }
    }
}
