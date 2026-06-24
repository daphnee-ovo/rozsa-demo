//! JSONL stdio bridge executable for the rozsa-core agent loop.
//!
//! Internal Framework:
//! bridge.rs
//! +-- main()                       # tokio entry, stdin/stdout/channel setup
//! +-- handle_start_run()           # spawns agent loop, forwards events to stdout
//! +-- HostTool                     # Tool impl that delegates to TS host via protocol
//! +-- route_tool_result()          # routes incoming tool_result to waiting HostTool
//! +-- write_bridge_output()        # serializes one BridgeOutput line to stdout
//!
//! Related Docs:
//! - [Protocol](../protocol.rs)
//! - [Agent Loop](../agent_loop.rs)

use std::collections::HashMap;
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use rozsa_core::config::AgentLoopConfig;
use rozsa_core::events::AgentEvent;
use rozsa_core::messages::AgentMessage;
use rozsa_core::protocol::{
    BridgeConfig, BridgeInput, BridgeOutput, RunMode, ToolHostResult, parse_input_line,
};
use rozsa_core::tool::{Tool, ToolError, ToolExecutionMode, ToolResult};

use rozsa_model::event_stream::EventStream;
use rozsa_model::types::Message;

// --- HostTool: delegates tool execution to TS host via protocol ---

/// 当 agent loop 需要执行 tool 时，HostTool 通过 protocol 将请求发给 TS host，
/// 等待 TS 返回 tool_result 后返回给 loop。
struct HostTool {
    tool_name: String,
    tool_description: String,
    schema: serde_json::Value,
    output_tx: mpsc::UnboundedSender<BridgeOutput>,
    run_id: Arc<str>,
    pending_requests: Arc<Mutex<HashMap<String, oneshot::Sender<ToolHostResult>>>>,
    /// 当前 turn 的 assistant message（tool dispatch 发生时已知）
    assistant_message: Arc<Mutex<rozsa_model::types::AssistantMessage>>,
    /// 当前 context 快照
    context: Arc<Mutex<rozsa_core::config::AgentContext>>,
}

#[async_trait::async_trait]
impl Tool for HostTool {
    fn name(&self) -> &str {
        &self.tool_name
    }

    fn description(&self) -> &str {
        &self.tool_description
    }

    fn label(&self) -> &str {
        &self.tool_name
    }

    fn parameters_schema(&self) -> &serde_json::Value {
        &self.schema
    }

    async fn execute(
        &self,
        tool_call_id: &str,
        params: serde_json::Value,
        signal: Option<CancellationToken>,
        _on_update: Option<&(dyn Fn(ToolResult) + Send + Sync)>,
    ) -> Result<ToolResult, ToolError> {
        let request_id = uuid::Uuid::new_v4().to_string();

        // 注册 oneshot channel，等待 TS 回复
        let (tx, rx) = oneshot::channel();
        self.pending_requests
            .lock()
            .await
            .insert(request_id.clone(), tx);

        let assistant_msg = self.assistant_message.lock().await.clone();
        let ctx = self.context.lock().await.clone();

        let output = BridgeOutput::tool_request(
            &self.run_id,
            &request_id,
            tool_call_id,
            &self.tool_name,
            params,
            assistant_msg,
            ctx,
        );
        if self.output_tx.send(output).is_err() {
            self.pending_requests.lock().await.remove(&request_id);
            return Err(ToolError::Execution(
                "Bridge output channel closed".to_string(),
            ));
        }

        // 等待 tool_result 或 cancellation
        let result = if let Some(token) = signal {
            tokio::select! {
                _ = token.cancelled() => {
                    self.pending_requests.lock().await.remove(&request_id);
                    return Err(ToolError::Cancelled);
                }
                res = rx => res,
            }
        } else {
            rx.await
        };

        match result {
            Ok(host_result) => Ok(ToolResult {
                content: host_result.content,
                details: serde_json::Value::Null,
                terminate: host_result.terminate,
            }),
            Err(_) => {
                // oneshot sender dropped — run was cancelled or bridge shutting down
                Err(ToolError::Cancelled)
            }
        }
    }

    fn execution_mode(&self) -> Option<ToolExecutionMode> {
        None
    }
}

// --- State for an active run ---

struct ActiveRun {
    cancel_token: CancellationToken,
    pending_requests: Arc<Mutex<HashMap<String, oneshot::Sender<ToolHostResult>>>>,
}

// --- Main ---

#[tokio::main]
async fn main() {
    // tracing → stderr only
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing_subscriber::filter::LevelFilter::INFO.into()),
        )
        .init();

    info!("rozsa-core bridge starting");

    let stdin = BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();

    // stdout 输出 channel — 所有写 stdout 的消息通过这个 channel 串行化
    let (output_tx, mut output_rx) = mpsc::unbounded_channel::<BridgeOutput>();

    // stdout writer task
    let writer_handle = tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        while let Some(output) = output_rx.recv().await {
            write_bridge_output(&mut stdout, output).await;
        }
    });

    // 一次只有一个 active run
    let mut active_run: Option<ActiveRun> = None;
    // Channel for run completion notification
    let (run_done_tx, mut run_done_rx) = mpsc::unbounded_channel::<()>();
    // run_done_tx is used via clone in the spawn block below; keep it alive for channel lifetime

    // Main input loop
    while let Ok(Some(line)) = lines.next_line().await {
        // 先检查是否有 run 完成通知
        while run_done_rx.try_recv().is_ok() {
            active_run = None;
        }
        if line.trim().is_empty() {
            continue;
        }

        let input = match parse_input_line(&line) {
            Ok(input) => input,
            Err(err) => {
                warn!(error = %err, "Failed to parse input line");
                // 如果有 active run, 发送 run_error; 否则忽略
                if active_run.is_some() {
                    let _ = output_tx.send(BridgeOutput::run_error(
                        "unknown",
                        format!("Invalid JSON input: {err}"),
                    ));
                }
                continue;
            }
        };

        match input {
            BridgeInput::StartRun {
                run_id,
                mode,
                config,
                ..
            } => {
                if active_run.is_some() {
                    let _ = output_tx.send(BridgeOutput::run_error(
                        &run_id,
                        "A run is already active. Send cancel first.",
                    ));
                    continue;
                }

                let cancel_token = CancellationToken::new();
                let pending_requests: Arc<Mutex<HashMap<String, oneshot::Sender<ToolHostResult>>>> =
                    Arc::new(Mutex::new(HashMap::new()));

                active_run = Some(ActiveRun {
                    cancel_token: cancel_token.clone(),
                    pending_requests: pending_requests.clone(),
                });

                let output_tx_clone = output_tx.clone();
                let run_id_clone = run_id.clone();
                let done_tx = run_done_tx.clone();

                tokio::spawn(async move {
                    handle_start_run(
                        run_id_clone,
                        mode,
                        config,
                        cancel_token,
                        pending_requests,
                        output_tx_clone,
                    )
                    .await;
                    let _ = done_tx.send(());
                });
            }

            BridgeInput::Cancel { run_id, .. } => {
                if let Some(ref run) = active_run {
                    info!(run_id = %run_id, "Cancelling run");
                    run.cancel_token.cancel();
                    // 释放所有 pending tool requests
                    let mut pending = run.pending_requests.lock().await;
                    pending.clear(); // dropping senders will cause HostTool::execute to return Cancelled
                } else {
                    warn!(run_id = %run_id, "Cancel received but no active run");
                }
                active_run = None;
            }

            BridgeInput::ToolResult {
                run_id,
                request_id,
                result,
                ..
            } => {
                if let Some(ref run) = active_run {
                    let mut pending = run.pending_requests.lock().await;
                    if let Some(sender) = pending.remove(&request_id) {
                        if sender.send(result).is_err() {
                            warn!(
                                run_id = %run_id,
                                request_id = %request_id,
                                "Tool result receiver already dropped"
                            );
                        }
                    } else {
                        warn!(
                            run_id = %run_id,
                            request_id = %request_id,
                            "No pending request for tool_result"
                        );
                    }
                } else {
                    warn!(
                        run_id = %run_id,
                        request_id = %request_id,
                        "tool_result received but no active run"
                    );
                }
            }
        }
    }

    // stdin closed — cancel active run
    if let Some(run) = active_run.take() {
        info!("stdin closed, cancelling active run");
        run.cancel_token.cancel();
        run.pending_requests.lock().await.clear();
    }

    // Close output channel, wait for writer to drain
    drop(output_tx);
    let _ = writer_handle.await;

    info!("rozsa-core bridge exiting");
}

/// 处理 start_run：构建 AgentLoopConfig，启动 agent loop，将事件转发到 stdout，
/// loop 结束后发送 run_done。
async fn handle_start_run(
    run_id: String,
    mode: RunMode,
    config: BridgeConfig,
    cancel_token: CancellationToken,
    pending_requests: Arc<Mutex<HashMap<String, oneshot::Sender<ToolHostResult>>>>,
    output_tx: mpsc::UnboundedSender<BridgeOutput>,
) {
    let run_id_arc: Arc<str> = Arc::from(run_id.as_str());

    // 从 mode 中取出 context 和 prompts
    let (prompts, context) = match mode {
        RunMode::Prompt { prompts, context } => (prompts, context),
        RunMode::Continue { context } => (vec![], context),
    };

    // Shared state for HostTool to include in tool_request
    let shared_assistant_message = Arc::new(Mutex::new(rozsa_model::types::AssistantMessage {
        content: vec![],
        api: rozsa_model::types::Api::OpenAIResponses,
        provider: rozsa_model::types::Provider::OpenAI,
        model: config.model.id.clone(),
        response_model: None,
        response_id: None,
        usage: rozsa_model::types::Usage {
            input: 0, output: 0, cache_read: 0, cache_write: 0, total_tokens: 0,
            cost: rozsa_model::types::UsageCost { input: 0.0, output: 0.0, cache_read: 0.0, cache_write: 0.0, total: 0.0 },
        },
        stop_reason: rozsa_model::types::StopReason::Stop,
        error_message: None,
        timestamp: 0,
    }));
    let shared_context = Arc::new(Mutex::new(context.clone()));

    let tools: Vec<Arc<dyn Tool>> = context
        .tools
        .iter()
        .map(|schema| {
            let tool: Arc<dyn Tool> = Arc::new(HostTool {
                tool_name: schema.name.clone(),
                tool_description: schema.description.clone(),
                schema: schema.parameters.clone(),
                output_tx: output_tx.clone(),
                run_id: run_id_arc.clone(),
                pending_requests: pending_requests.clone(),
                assistant_message: shared_assistant_message.clone(),
                context: shared_context.clone(),
            });
            tool
        })
        .collect();

    // 构建 model_stream fn — 使用 rozsa-model 的 simple stream
    let model_stream_fn: rozsa_core::config::ModelStreamFn = Box::new(
        move |model: &rozsa_model::types::Model,
              ctx: &rozsa_model::types::Context,
              opts: &rozsa_model::types::SimpleStreamOptions| {
            rozsa_model::stream::stream_simple(model, ctx, opts)
        },
    );

    // 构建 convert_to_llm fn — 直接提取标准消息
    let convert_to_llm: Box<dyn Fn(&[AgentMessage]) -> Vec<Message> + Send + Sync> =
        Box::new(|messages: &[AgentMessage]| {
            messages
                .iter()
                .filter_map(|m| m.as_standard().cloned())
                .collect()
        });

    let loop_config = AgentLoopConfig {
        model: config.model,
        reasoning: config.reasoning,
        stream_options: config.stream_options,
        model_stream: model_stream_fn,
        convert_to_llm,
        transform_context: None,
        get_api_key: None,
        should_stop_after_turn: None,
        prepare_next_turn: None,
        get_steering_messages: None,
        get_follow_up_messages: None,
        tool_execution: config.tool_execution,
        pre_tool_use: None,
        post_tool_use: None,
        tools,
    };

    // 启动 agent loop
    let is_continue = prompts.is_empty();
    let mut event_stream: EventStream<AgentEvent> = if is_continue {
        rozsa_core::agent_loop::agent_loop_continue(context, loop_config, Some(cancel_token))
    } else {
        rozsa_core::agent_loop::agent_loop(prompts, context, loop_config, Some(cancel_token))
    };

    while let Some(event) = event_stream.next().await {
        // Track latest assistant message for tool_request context
        if let AgentEvent::MessageEnd { ref message } = event {
            if let Some(rozsa_model::types::Message::Assistant(msg)) = message.as_standard() {
                *shared_assistant_message.lock().await = msg.clone();
            }
        }
        // Track context updates from tool results being added
        if let AgentEvent::TurnEnd { .. } = event {
            // Context is managed by the loop internally; tool_request captures at call time
        }

        let output = BridgeOutput::agent_event(&run_id, event);
        if output_tx.send(output).is_err() {
            error!(run_id = %run_id, "Output channel closed while streaming events");
            return;
        }
    }

    // loop 结束，发送 run_done
    let _ = output_tx.send(BridgeOutput::run_done(&run_id));
}

/// 将一个 BridgeOutput 序列化为 JSON 行并立即写入 stdout。
async fn write_bridge_output(stdout: &mut tokio::io::Stdout, output: BridgeOutput) {
    match serde_json::to_string(&output) {
        Ok(serialized) => {
            let _ = stdout.write_all(serialized.as_bytes()).await;
            let _ = stdout.write_all(b"\n").await;
            let _ = stdout.flush().await;
        }
        Err(err) => {
            // Fallback: 尝试发送一个 run_error
            let fallback = BridgeOutput::run_error("unknown", format!("Serialize error: {err}"));
            if let Ok(serialized) = serde_json::to_string(&fallback) {
                let _ = stdout.write_all(serialized.as_bytes()).await;
                let _ = stdout.write_all(b"\n").await;
                let _ = stdout.flush().await;
            }
        }
    }
}
