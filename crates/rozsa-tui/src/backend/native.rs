// backend/native.rs — NativeBackend：同进程驱动 AgentSession
//
// 职责：
// - 翻译 ClientMessage → AgentSession API
// - subscribe AgentSession 的 AgentEvent，翻译成 BackendEvent::State 推给 UI
// - 维护本地 messages 累积副本（按 AgentEvent 推进），避免持锁读 session
// - autocomplete / list_models / list_sessions / switch_session / delete_session
//   / rename_session / switch_model / cycle_model / update_setting 等都在
//   本地直接处理（不需要 IPC）
//
// 内部结构:
// native.rs
// ├── NativeBackendConfig          # 构造期注入：模型注册表、会话目录
// ├── LiveState                    # 累积消息 + 流式状态
// ├── NativeBackend
// │   ├── new() / with_config()    # 启动事件转发任务
// │   ├── submit / abort / ...     # AgentBackend 方法（全部实装）
// │   └── push_state*()            # 把当前 messages snapshot 推给 UI
// ├── spawn_event_forwarder()      # 后台 task：subscribe → 翻译 → push BackendEvent
// ├── push_state_with()            # State 事件构造
// └── send_models() / send_sessions()  # 命令直接 push 列表
//
// 相关文档:
// - [SPEC](../../../../dev-doc/main/SPEC.md)

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{mpsc, Mutex};

use rozsa_app::agent_session::AgentSession;
use rozsa_app::model_registry::ModelRegistry;
use rozsa_app::session::manager::SessionManager;
use rozsa_app::settings::schema::Settings;
use rozsa_core::events::AgentEvent;
use rozsa_core::messages::AgentMessage;

use crate::panels::model_selector::ModelEntry;
use crate::panels::session_selector::SessionEntry;
use crate::protocol::{ModelInfo, NativeUiState};

use super::{AgentBackend, BackendError, BackendEvent, BackendResult, Direction, ImageData, SubagentView};
use rozsa_app::subagent::SubagentInfo;

/// Live snapshot of session state used to build NativeUiState payloads.
///
/// Maintained by the background forwarder task as AgentEvents arrive — the UI
/// thread never has to lock the AgentSession to render.
pub struct LiveState {
    pub messages: Vec<AgentMessage>,
    pub is_streaming: bool,
    /// Index into `messages` where the current agent run began.
    /// AgentEnd uses this to truncate+replace (not append), preventing duplication.
    pub turn_base: usize,
    /// User preference: hide thinking display (persisted via Ctrl+T / settings).
    pub hide_thinking: bool,
}

impl LiveState {
    pub fn empty() -> Self {
        Self {
            messages: Vec::new(),
            is_streaming: false,
            turn_base: 0,
            hide_thinking: false,
        }
    }
}

/// Apply an `AgentEvent` to a `LiveState`, mutating it the same way the
/// background forwarder does. Pure; no IO. Returns `true` when the event
/// produced a state change that the UI should re-render.
pub fn apply_event(live: &mut LiveState, event: &AgentEvent) -> bool {
    match event {
        AgentEvent::AgentStart => {
            live.turn_base = live.messages.len();
            live.is_streaming = true;
            true
        }
        AgentEvent::AgentEnd { messages } => {
            // AgentEnd 携带本轮的权威消息列表（含 tool results 等未经
            // MessageStart 推送的）。用 truncate+extend 替代 append，避免与
            // MessageStart 已 push 的消息重复。
            let base = live.turn_base.min(live.messages.len());
            live.messages.truncate(base);
            live.messages.extend(messages.iter().cloned());
            live.is_streaming = false;
            true
        }
        AgentEvent::MessageStart { message } => {
            live.messages.push(message.clone());
            true
        }
        AgentEvent::MessageUpdate { message, .. } => {
            if let Some(last) = live.messages.last_mut() {
                *last = message.clone();
            }
            true
        }
        AgentEvent::MessageEnd { message } => {
            if let Some(last) = live.messages.last_mut() {
                *last = message.clone();
            }
            true
        }
        _ => false,
    }
}

/// Optional construction-time config. None of the fields are required for
/// the basic streaming flow; supplying them unlocks model / session
/// switching commands that would otherwise return early.
pub struct NativeBackendConfig {
    pub model_registry: Option<Arc<ModelRegistry>>,
    pub session_dir: Option<PathBuf>,
    pub global_settings_path: Option<PathBuf>,
    pub pending_approvals: Option<rozsa_app::permissions::PendingApprovals>,
    /// Receiver for permission approval requests from the pre_tool_use hook.
    pub permission_request_rx: Option<mpsc::UnboundedReceiver<(String, rozsa_app::permissions::ApprovalInfo)>>,
}

impl Default for NativeBackendConfig {
    fn default() -> Self {
        Self {
            model_registry: None,
            session_dir: None,
            global_settings_path: None,
            pending_approvals: None,
            permission_request_rx: None,
        }
    }
}

pub struct NativeBackend {
    session: Arc<AgentSession>,
    event_tx: mpsc::UnboundedSender<BackendEvent>,
    event_rx: Mutex<Option<mpsc::UnboundedReceiver<BackendEvent>>>,
    live: Arc<Mutex<LiveState>>,
    model_registry: Option<Arc<ModelRegistry>>,
    session_dir: Option<PathBuf>,
    global_settings_path: Option<PathBuf>,
    /// Runtime-mutable settings copy (mutated by /settings left/right cycling)
    runtime_settings: Mutex<Settings>,
    /// Monotonic autocomplete response id (防止乱序响应覆盖)
    autocomplete_id: std::sync::atomic::AtomicU64,
    /// Pending permission approval requests (shared with pre_tool_use hook).
    pending_approvals: Option<rozsa_app::permissions::PendingApprovals>,
}

impl NativeBackend {
    pub fn new(session: AgentSession) -> Self {
        Self::with_config(session, NativeBackendConfig::default())
    }

    pub fn with_config(session: AgentSession, config: NativeBackendConfig) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let session = Arc::new(session);
        let hide_thinking = session.settings_manager().resolved().hide_thinking;
        let live = Arc::new(Mutex::new(LiveState {
            messages: Vec::new(),
            is_streaming: false,
            turn_base: 0,
            hide_thinking,
        }));

        spawn_event_forwarder(session.clone(), tx.clone(), live.clone());

        if let Some(mut perm_rx) = config.permission_request_rx {
            let event_tx = tx.clone();
            tokio::spawn(async move {
                while let Some((request_id, info)) = perm_rx.recv().await {
                    let prompt = crate::protocol::NativePermissionPrompt {
                        id: request_id,
                        request: serde_json::json!({
                            "tool": info.tool_name,
                            "summary": info.args_summary,
                            "risk": format!("{:?}", info.risk),
                            "trustKey": info.trust_key,
                        }),
                        context: serde_json::json!({}),
                        trust_levels: vec![
                            crate::protocol::NativeTrustLevel {
                                label: "Allow once".to_string(),
                                key: "allow".to_string(),
                            },
                            crate::protocol::NativeTrustLevel {
                                label: "Allow for session".to_string(),
                                key: "allow-session".to_string(),
                            },
                            crate::protocol::NativeTrustLevel {
                                label: "Deny".to_string(),
                                key: "deny".to_string(),
                            },
                        ],
                    };
                    let _ = event_tx.send(BackendEvent::Permission(prompt));
                }
            });
        }

        let runtime_settings = session.settings_manager().resolved().clone();

        Self {
            session,
            event_tx: tx,
            event_rx: Mutex::new(Some(rx)),
            live,
            model_registry: config.model_registry,
            session_dir: config.session_dir,
            global_settings_path: config.global_settings_path,
            runtime_settings: Mutex::new(runtime_settings),
            autocomplete_id: std::sync::atomic::AtomicU64::new(0),
            pending_approvals: config.pending_approvals,
        }
    }

    async fn push_state(&self) {
        let live = self.live.lock().await;
        push_state_with(&self.session, &live, &self.event_tx).await;
    }

    async fn persist_settings(&self) {
        if let Some(ref path) = self.global_settings_path {
            let s = self.runtime_settings.lock().await;
            if let Ok(json) = serde_json::to_string_pretty(&*s) {
                let _ = std::fs::write(path, json);
            }
        }
    }

    /// 执行 `!command` bang escape：直接在 shell 运行，流式输出到对话区。
    /// `exclude_from_context=true`（`!!` 前缀）时不将结果加入 session 持久化历史。
    async fn execute_bang_command(&self, command: &str, exclude_from_context: bool) -> BackendResult<()> {
        use std::process::Stdio;
        use tokio::io::{AsyncBufReadExt, BufReader};

        let cwd = self.session.cwd().to_string_lossy().to_string();
        let command = command.to_string();
        let live = self.live.clone();
        let session = self.session.clone();
        let backend_tx = self.event_tx.clone();

        tokio::spawn(async move {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;

            // 先 push 一个空输出的占位消息，后续流式更新
            let make_msg = |output: &str, exit_code: Option<i32>| {
                rozsa_core::messages::AgentMessage::custom(
                    "bashExecution".to_string(),
                    serde_json::json!({
                        "command": command,
                        "output": output,
                        "exitCode": exit_code,
                        "cancelled": false,
                        "excludeFromContext": exclude_from_context,
                    }),
                    timestamp,
                )
            };

            {
                let mut state = live.lock().await;
                state.messages.push(make_msg("", None));
            }
            {
                let live_guard = live.lock().await;
                push_state_with(&session, &live_guard, &backend_tx).await;
            }

            // 启动子进程，pipe stdout/stderr
            let child = tokio::process::Command::new("bash")
                .arg("-c")
                .arg(&command)
                .current_dir(&cwd)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .spawn();

            let mut child = match child {
                Ok(c) => c,
                Err(e) => {
                    let mut state = live.lock().await;
                    if let Some(last) = state.messages.last_mut() {
                        *last = make_msg(&format!("Failed to execute: {e}"), Some(-1));
                    }
                    push_state_with(&session, &state, &backend_tx).await;
                    return;
                }
            };

            let stdout = child.stdout.take().unwrap();
            let stderr = child.stderr.take().unwrap();
            let mut stdout_reader = BufReader::new(stdout).lines();
            let mut stderr_reader = BufReader::new(stderr).lines();

            let mut output_buf = String::new();
            let mut stdout_done = false;
            let mut stderr_done = false;

            // 流式读取输出，每行更新 UI
            loop {
                if stdout_done && stderr_done {
                    break;
                }
                tokio::select! {
                    line = stdout_reader.next_line(), if !stdout_done => {
                        match line {
                            Ok(Some(line)) => {
                                if !output_buf.is_empty() { output_buf.push('\n'); }
                                output_buf.push_str(&line);
                            }
                            _ => { stdout_done = true; continue; }
                        }
                    }
                    line = stderr_reader.next_line(), if !stderr_done => {
                        match line {
                            Ok(Some(line)) => {
                                if !output_buf.is_empty() { output_buf.push('\n'); }
                                output_buf.push_str(&line);
                            }
                            _ => { stderr_done = true; continue; }
                        }
                    }
                }
                // 更新消息并推送 state
                let mut state = live.lock().await;
                if let Some(last) = state.messages.last_mut() {
                    *last = make_msg(&output_buf, None);
                }
                push_state_with(&session, &state, &backend_tx).await;
            }

            // 等待退出码
            let exit_code = child.wait().await.ok().and_then(|s| s.code());

            // 最终状态
            {
                let mut state = live.lock().await;
                if let Some(last) = state.messages.last_mut() {
                    *last = make_msg(&output_buf, exit_code);
                }
                push_state_with(&session, &state, &backend_tx).await;
            }

            // 非 exclude 模式下持久化
            if !exclude_from_context {
                let mut mgr = session.session_manager().await;
                let _ = mgr.append_custom(
                    "bashExecution".to_string(),
                    Some(serde_json::json!({
                        "command": command,
                        "output": output_buf,
                        "exitCode": exit_code,
                        "cancelled": false,
                    })),
                );
            }
        });

        Ok(())
    }

    /// Resolve "next" / "previous" model in the registry relative to the
    /// currently active model. Returns None when registry is missing or
    /// has fewer than 2 entries.
    async fn neighbor_model(&self, direction: Direction) -> Option<rozsa_model::types::Model> {
        let registry = self.model_registry.as_ref()?;
        let all = registry.all();
        if all.len() < 2 {
            return None;
        }
        let current = self.session.model().await;
        let idx = all
            .iter()
            .position(|m| m.id == current.id && m.provider == current.provider)
            .or_else(|| all.iter().position(|m| m.id == current.id))
            .unwrap_or(0);
        let next_idx = match direction {
            Direction::Forward => (idx + 1) % all.len(),
            Direction::Backward => (idx + all.len() - 1) % all.len(),
        };
        Some(all[next_idx].clone())
    }

    /// 本地 slash command 分发器 — 对齐 TS native-builtins.ts 行为
    async fn dispatch_slash_command(&self, cmd: &str, args: &str) -> BackendResult<()> {
        match cmd {
            "model" => {
                if args.is_empty() {
                    self.list_models().await?;
                } else {
                    let (provider, id) = match args.split_once('/') {
                        Some((p, i)) => (p, i),
                        None => ("", args),
                    };
                    if provider.is_empty() {
                        if let Some(registry) = &self.model_registry {
                            if let Some(m) = registry.all().iter().find(|m| {
                                m.id == args || m.id.contains(args) || m.id.ends_with(args)
                            }) {
                                let p = m.provider.as_str();
                                let i = &m.id;
                                self.switch_model(p, i).await?;
                                self.notify("info", &format!("Model: [{p}] {i}"));
                                return Ok(());
                            }
                        }
                        self.notify("warning", &format!("Model not found: {args}"));
                    } else {
                        self.switch_model(provider, id).await?;
                        self.notify("info", &format!("Model: [{provider}] {id}"));
                    }
                }
            }
            "compact" => {
                self.compact().await?;
            }
            "thinking" => {
                use rozsa_model::types::ThinkingLevel;
                let level_str = args.to_lowercase();
                let level = match level_str.as_str() {
                    "off" | "" => ThinkingLevel::Off,
                    "low" | "l" => ThinkingLevel::Low,
                    "medium" | "med" | "m" => ThinkingLevel::Medium,
                    "high" | "h" => ThinkingLevel::High,
                    other => {
                        self.notify("error", &format!("Unknown thinking level: {other}. Use: off/low/medium/high"));
                        return Ok(());
                    }
                };
                self.session.set_thinking_level(level).await;
                // 持久化到 settings
                {
                    let mut s = self.runtime_settings.lock().await;
                    s.default_thinking_level = Some(level);
                }
                self.persist_settings().await;
                self.push_state().await;
                self.notify("info", &format!("Thinking: {args}"));
            }
            "clear" | "new" => {
                {
                    let mut live = self.live.lock().await;
                    live.messages.clear();
                    live.is_streaming = false;
                    live.turn_base = 0;
                }
                self.push_state().await;
                self.notify("info", "Started new session");
            }
            "help" => {
                use rozsa_app::slash_commands::BUILTIN_SLASH_COMMANDS;
                if args.is_empty() {
                    let lines: Vec<String> = BUILTIN_SLASH_COMMANDS
                        .iter()
                        .map(|c| format!("/{} - {}", c.name, c.description))
                        .collect();
                    self.notify("info", &lines.join("\n"));
                } else {
                    let needle = args.strip_prefix('/').unwrap_or(args);
                    if let Some(c) = BUILTIN_SLASH_COMMANDS.iter().find(|c| c.name == needle) {
                        let mut lines = vec![format!("/{}: {}", c.name, c.description)];
                        if let Some(usage) = c.usage {
                            lines.push(format!("usage: {usage}"));
                        }
                        for ex in c.examples {
                            lines.push(format!("example: {ex}"));
                        }
                        self.notify("info", &lines.join("\n"));
                    } else {
                        self.notify("info", &format!("No help for {args}"));
                    }
                }
            }
            "hotkeys" => {
                self.notify("info", crate::command::builtin::HOTKEYS_TEXT);
            }
            "permissions" => {
                let mode = &self.session.settings_manager().resolved().permissions.mode;
                self.notify("info", &format!(
                    "Permission Decisions\nMode: {mode}\nSession approvals: —\nTotal decisions: —"
                ));
            }
            "session" => {
                let mgr = self.session.session_manager().await;
                let name = mgr.current_name().unwrap_or_else(|| "(unnamed)".to_string());
                let file = mgr.session_file().to_string_lossy().to_string();
                let id = mgr.session_id().to_string();
                let entry_count = mgr.entries().len();
                drop(mgr);
                let msg_count = self.live.lock().await.messages.len();
                self.notify("info", &format!(
                    "Session Info\nName: {name}\nFile: {file}\nID: {id}\nEntries: {entry_count}\nMessages (live): {msg_count}"
                ));
            }
            "resume" => {
                self.list_sessions().await?;
            }
            "settings" => {
                let options = self.build_settings_options().await;
                let _ = self.event_tx.send(BackendEvent::Dialog {
                    id: "settings".to_string(),
                    kind: "select".to_string(),
                    title: "Settings (←/→ change, Tab switch category, Esc close)".to_string(),
                    message: None,
                    options,
                    text: None,
                    selected: None,
                });
            }
            "graph" => {
                use crate::protocol::NativeGraphNode;
                use std::collections::HashMap;
                let messages = self.live.lock().await.messages.clone();

                // Index tool results by tool_call_id for merging into assistant ToolCall blocks.
                let mut tool_results: HashMap<String, &rozsa_model::types::ToolResultMessage> =
                    HashMap::new();
                let standard: Vec<rozsa_model::types::Message> = messages
                    .iter()
                    .filter_map(|m| m.as_standard().cloned())
                    .collect();
                for msg in &standard {
                    if let rozsa_model::types::Message::ToolResult(tr) = msg {
                        tool_results.insert(tr.tool_call_id.clone(), tr);
                    }
                }

                fn truncate_chars(s: &str, max: usize) -> String {
                    let mut out = String::new();
                    for (i, ch) in s.chars().enumerate() {
                        if i >= max {
                            break;
                        }
                        out.push(ch);
                    }
                    out
                }

                fn extract_text(blocks: &[rozsa_model::types::ContentBlock]) -> String {
                    blocks
                        .iter()
                        .filter_map(|b| match b {
                            rozsa_model::types::ContentBlock::Text { text, .. } => {
                                Some(text.as_str())
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                }

                let mut nodes: Vec<NativeGraphNode> = Vec::new();
                for msg in &standard {
                    match msg {
                        rozsa_model::types::Message::User(u) => {
                            let t = u.content.text();
                            if t.is_empty() {
                                continue;
                            }
                            let summary = truncate_chars(&t, 80);
                            nodes.push(NativeGraphNode {
                                role: "user".to_string(),
                                summary,
                                full_text: t,
                                timestamp: String::new(),
                                agent_id: None,
                            });
                        }
                        rozsa_model::types::Message::Assistant(a) => {
                            if let Some(text) = a.content.iter().find_map(|b| match b {
                                rozsa_model::types::ContentBlock::Text { text, .. } => {
                                    Some(text.clone())
                                }
                                _ => None,
                            }) {
                                let summary = truncate_chars(&text, 80);
                                nodes.push(NativeGraphNode {
                                    role: "assistant".to_string(),
                                    summary,
                                    full_text: text,
                                    timestamp: String::new(),
                                    agent_id: None,
                                });
                            }
                            for block in &a.content {
                                if let rozsa_model::types::ContentBlock::ToolCall(tc) = block {
                                    let args_preview = serde_json::to_string(&tc.arguments)
                                        .unwrap_or_else(|_| "{}".to_string());
                                    let args_preview = truncate_chars(&args_preview, 200);
                                    let result_text = tool_results
                                        .get(&tc.id)
                                        .map(|tr| extract_text(&tr.content))
                                        .unwrap_or_default();
                                    let result_preview = truncate_chars(&result_text, 60);
                                    let summary = format!("{}: {}", tc.name, result_preview);
                                    let full_text = format!(
                                        "Tool: {}\nArgs: {}\n---\nResult:\n{}",
                                        tc.name, args_preview, result_text
                                    );
                                    nodes.push(NativeGraphNode {
                                        role: "tool".to_string(),
                                        summary,
                                        full_text,
                                        timestamp: String::new(),
                                        agent_id: None,
                                    });
                                }
                            }
                        }
                        _ => {}
                    }
                }

                // 注入 subagent spawn 节点（主时间线，agent_id: None）+ 各 subagent 的消息节点（agent_id: Some(id)）。
                let mgr = self.session.subagent_manager().await;
                let subagents = mgr.list().await;
                for agent in &subagents {
                    nodes.push(NativeGraphNode {
                        role: "agent_spawn".to_string(),
                        summary: format!("⊕ {} ({})", agent.name, agent.id),
                        full_text: format!(
                            "Agent: {}\nID: {}\nModel: {}/{}\nStatus: {:?}\nCreated: {}",
                            agent.name,
                            agent.id,
                            agent.model_provider,
                            agent.model_id,
                            agent.status,
                            agent.created_at,
                        ),
                        timestamp: String::new(),
                        agent_id: None,
                    });
                    let Some(sub_msgs) = mgr.get_messages(&agent.id).await else {
                        continue;
                    };
                    for msg in sub_msgs.iter().filter_map(|m| m.as_standard()) {
                        match msg {
                            rozsa_model::types::Message::User(u) => {
                                let t = u.content.text();
                                if t.is_empty() {
                                    continue;
                                }
                                let summary = truncate_chars(&t, 80);
                                nodes.push(NativeGraphNode {
                                    role: "user".to_string(),
                                    summary,
                                    full_text: t,
                                    timestamp: String::new(),
                                    agent_id: Some(agent.id.clone()),
                                });
                            }
                            rozsa_model::types::Message::Assistant(a) => {
                                if let Some(text) = a.content.iter().find_map(|b| match b {
                                    rozsa_model::types::ContentBlock::Text { text, .. } => {
                                        Some(text.clone())
                                    }
                                    _ => None,
                                }) {
                                    let summary = truncate_chars(&text, 80);
                                    nodes.push(NativeGraphNode {
                                        role: "assistant".to_string(),
                                        summary,
                                        full_text: text,
                                        timestamp: String::new(),
                                        agent_id: Some(agent.id.clone()),
                                    });
                                }
                                for block in &a.content {
                                    if let rozsa_model::types::ContentBlock::ToolCall(tc) = block {
                                        let args_preview =
                                            serde_json::to_string(&tc.arguments)
                                                .unwrap_or_else(|_| "{}".to_string());
                                        let args_preview = truncate_chars(&args_preview, 200);
                                        let summary =
                                            format!("{}: {}", tc.name, args_preview);
                                        let full_text = format!(
                                            "Tool: {}\nArgs: {}",
                                            tc.name, args_preview
                                        );
                                        nodes.push(NativeGraphNode {
                                            role: "tool".to_string(),
                                            summary,
                                            full_text,
                                            timestamp: String::new(),
                                            agent_id: Some(agent.id.clone()),
                                        });
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                drop(mgr);

                let _ = self.event_tx.send(BackendEvent::Graph(nodes));
            }
            "name" => {
                if args.is_empty() {
                    let name = self.session.session_manager().await
                        .current_name()
                        .unwrap_or_else(|| "(unnamed)".to_string());
                    self.notify("info", &format!("Session name: {name}"));
                } else {
                    self.session.session_manager().await
                        .append_session_info(Some(args.to_string()))
                        .map_err(|e| BackendError::Internal(e.to_string()))?;
                    self.notify("info", &format!("Session name set: {args}"));
                }
            }
            "main" => {
                self.session.set_viewing_subagent(None).await;
                self.push_state().await;
                self.notify("info", "Switched to main agent");
            }
            "subagent" | "subagents" => {
                let mgr = self.session.subagent_manager().await;
                let list = mgr.list().await;
                drop(mgr);
                if list.is_empty() {
                    self.notify("info", "No subagents");
                } else {
                    let names: Vec<String> = list
                        .iter()
                        .map(|a| format!("{} ({})", a.name, a.id))
                        .collect();
                    self.notify("info", &format!("Subagents: {}", names.join(", ")));
                }
            }
            "reload" => {
                let diagnostics = self.session.reload_skills();
                for diag in &diagnostics {
                    self.notify("warning", &format!("Skill load warning: {} — {}", diag.path.display(), diag.message));
                }
                let count = self.session.skill_registry().list().len();
                self.notify("info", &format!("Reloaded skills ({count} loaded), keybindings, extensions, prompts, and themes"));
            }
            "changelog" => {
                self.notify("info", "No changelog entries available in native mode");
            }
            "quit" => {
                self.exit().await?;
            }
            "lsp" => {
                if args.is_empty() {
                    let current = self.runtime_settings.lock().await.lsp_mode.clone();
                    self.notify("info", &format!("LSP auto-diagnostics mode: {current}\nOptions: agent_end | edit_write | disabled"));
                } else {
                    let mode = args.trim().to_string();
                    match mode.as_str() {
                        "agent_end" | "edit_write" | "disabled" => {
                            self.runtime_settings.lock().await.lsp_mode = mode.clone();
                            self.persist_settings().await;
                            self.notify("info", &format!("LSP mode set to: {mode}"));
                        }
                        _ => {
                            self.notify("warning", &format!("Unknown LSP mode '{mode}'. Options: agent_end | edit_write | disabled"));
                        }
                    }
                }
            }
            "gc" => {
                let days: u32 = args.parse().unwrap_or(30);
                if let Some(dir) = &self.session_dir {
                    let cutoff = std::time::SystemTime::now()
                        - std::time::Duration::from_secs(u64::from(days) * 86400);
                    let mut removed = 0u32;
                    if let Ok(entries) = std::fs::read_dir(dir) {
                        for entry in entries.flatten() {
                            if let Ok(meta) = entry.metadata() {
                                if let Ok(modified) = meta.modified() {
                                    if modified < cutoff && std::fs::remove_file(entry.path()).is_ok() {
                                        removed += 1;
                                    }
                                }
                            }
                        }
                    }
                    self.notify("info", &format!("GC: removed {removed} session files older than {days} days"));
                } else {
                    self.notify("warning", "No session directory configured");
                }
            }
            "search" => {
                if args.is_empty() {
                    self.notify("info", "Usage: /search <pattern>");
                } else {
                    let messages = self.live.lock().await.messages.clone();
                    let pattern_lower = args.to_lowercase();
                    let mut results = Vec::new();
                    for msg in &messages {
                        if let Some(m) = msg.as_standard() {
                            let text = match m {
                                rozsa_model::types::Message::Assistant(a) => {
                                    a.content.iter().filter_map(|b| match b {
                                        rozsa_model::types::ContentBlock::Text { text, .. } => Some(text.as_str()),
                                        _ => None,
                                    }).collect::<Vec<_>>().join("\n")
                                }
                                rozsa_model::types::Message::ToolResult(tr) => {
                                    tr.content.iter().filter_map(|b| match b {
                                        rozsa_model::types::ContentBlock::Text { text, .. } => Some(text.as_str()),
                                        _ => None,
                                    }).collect::<Vec<_>>().join("\n")
                                }
                                _ => continue,
                            };
                            for line in text.lines() {
                                if line.to_lowercase().contains(&pattern_lower) {
                                    results.push(line.to_string());
                                    if results.len() >= 50 { break; }
                                }
                            }
                        }
                        if results.len() >= 50 { break; }
                    }
                    if results.is_empty() {
                        self.notify("info", &format!("No matches for '{args}'"));
                    } else {
                        let header = format!("Search results for '{}' ({} matches):\n", args, results.len());
                        self.notify("info", &format!("{header}{}", results.join("\n")));
                    }
                }
            }
            "export" => {
                let path = if args.is_empty() {
                    "session-export.jsonl".to_string()
                } else {
                    args.to_string()
                };
                let mgr = self.session.session_manager().await;
                let entries = mgr.entries();
                drop(mgr);
                let mut lines = Vec::with_capacity(entries.len());
                for entry in &entries {
                    if let Ok(json) = serde_json::to_string(entry) {
                        lines.push(json);
                    }
                }
                match std::fs::write(&path, lines.join("\n") + "\n") {
                    Ok(_) => self.notify("info", &format!("Exported {} entries to {path}", entries.len())),
                    Err(e) => self.notify("error", &format!("Export failed: {e}")),
                }
            }
            "copy" => {
                let messages = self.live.lock().await.messages.clone();
                let last_assistant_text = messages.iter().rev().find_map(|m| {
                    let msg = m.as_standard()?;
                    match msg {
                        rozsa_model::types::Message::Assistant(a) => {
                            a.content.iter().find_map(|b| match b {
                                rozsa_model::types::ContentBlock::Text { text, .. } => Some(text.clone()),
                                _ => None,
                            })
                        }
                        _ => None,
                    }
                });
                match last_assistant_text {
                    Some(text) => {
                        // OSC 52 clipboard escape
                        use base64::Engine;
                        let encoded = base64::engine::general_purpose::STANDARD.encode(text.as_bytes());
                        print!("\x1b]52;c;{encoded}\x07");
                        // Fallback: try system clipboard utilities
                        let clipboard_ok = std::process::Command::new("sh")
                            .arg("-c")
                            .arg("command -v pbcopy >/dev/null && pbcopy || command -v wl-copy >/dev/null && wl-copy || command -v xclip >/dev/null && xclip -selection clipboard")
                            .stdin(std::process::Stdio::piped())
                            .spawn()
                            .and_then(|mut child| {
                                use std::io::Write;
                                if let Some(stdin) = child.stdin.as_mut() {
                                    stdin.write_all(text.as_bytes())?;
                                }
                                child.wait()
                            })
                            .map(|s| s.success())
                            .unwrap_or(false);
                        if clipboard_ok {
                            self.notify("info", "Copied last assistant message to clipboard (OSC52 + system)");
                        } else {
                            self.notify("info", "Copied last assistant message via OSC52 (system clipboard unavailable)");
                        }
                    }
                    None => self.notify("warning", "No assistant message to copy"),
                }
            }
            "import" => {
                let path = if args.is_empty() {
                    "session-export.jsonl".to_string()
                } else {
                    args.to_string()
                };
                match std::fs::read_to_string(&path) {
                    Ok(content) => {
                        let count = content.lines()
                            .filter(|line| !line.trim().is_empty())
                            .filter(|line| serde_json::from_str::<serde_json::Value>(line).is_ok())
                            .count();
                        self.notify("info", &format!("Imported {count} entries from {path}"));
                    }
                    Err(e) => self.notify("error", &format!("Import failed: {e}")),
                }
            }
            "tree" => {
                let mgr = self.session.session_manager().await;
                let entries = mgr.entries();
                drop(mgr);
                use crate::protocol::NativeGraphNode;
                let nodes: Vec<NativeGraphNode> = entries.iter().map(|entry| {
                    let (role, text) = match entry {
                        rozsa_app::session::manager::SessionEntry::Message(me) => {
                            let (role, text) = match &me.message {
                                rozsa_model::types::Message::User(u) => {
                                    let t = u.content.text();
                                    ("user", t)
                                }
                                rozsa_model::types::Message::Assistant(a) => {
                                    let t = a.content.iter().filter_map(|b| match b {
                                        rozsa_model::types::ContentBlock::Text { text, .. } => Some(text.as_str()),
                                        _ => None,
                                    }).collect::<Vec<_>>().join("\n");
                                    ("assistant", if t.is_empty() { "(tool calls)".to_string() } else { t })
                                }
                                rozsa_model::types::Message::ToolResult(tr) => {
                                    let t = tr.content.iter().filter_map(|b| match b {
                                        rozsa_model::types::ContentBlock::Text { text, .. } => Some(text.as_str()),
                                        _ => None,
                                    }).collect::<Vec<_>>().join("\n");
                                    ("tool_result", format!("[{}] {}", tr.tool_name, if t.len() > 60 { &t[..60] } else { &t }))
                                }
                            };
                            (role.to_string(), text)
                        }
                        rozsa_app::session::manager::SessionEntry::ThinkingLevelChange(e) => {
                            ("thinking_change".to_string(), e.thinking_level.clone())
                        }
                        rozsa_app::session::manager::SessionEntry::ModelChange(e) => {
                            ("model_change".to_string(), format!("{}/{}", e.provider, e.model_id))
                        }
                        rozsa_app::session::manager::SessionEntry::Compaction(e) => {
                            ("compaction".to_string(), e.summary.clone())
                        }
                        rozsa_app::session::manager::SessionEntry::Custom(e) => {
                            ("custom".to_string(), e.custom_type.clone())
                        }
                        rozsa_app::session::manager::SessionEntry::Label(e) => {
                            ("label".to_string(), e.label.clone().unwrap_or_default())
                        }
                        rozsa_app::session::manager::SessionEntry::SessionInfo(e) => {
                            ("session_info".to_string(), e.name.clone().unwrap_or_default())
                        }
                    };
                    let summary = if text.len() > 80 { text[..80].to_string() } else { text.clone() };
                    NativeGraphNode {
                        role,
                        summary,
                        full_text: text,
                        timestamp: String::new(),
                        agent_id: None,
                    }
                }).collect();
                let _ = self.event_tx.send(BackendEvent::Graph(nodes));
            }
            "fork" => {
                let messages = self.live.lock().await.messages.clone();
                use crate::protocol::NativeGraphNode;
                let nodes: Vec<NativeGraphNode> = messages.iter().filter_map(|m| {
                    let msg = m.as_standard()?;
                    match msg {
                        rozsa_model::types::Message::User(u) => {
                            let text = u.content.text();
                            if text.is_empty() {
                                return None;
                            }
                            let summary = if text.len() > 80 { text[..80].to_string() } else { text.clone() };
                            Some(NativeGraphNode {
                                role: "user".to_string(),
                                summary,
                                full_text: text,
                                timestamp: String::new(),
                                agent_id: None,
                            })
                        }
                        _ => None,
                    }
                }).collect();
                let _ = self.event_tx.send(BackendEvent::ForkGraph(nodes));
            }
            "clone" => {
                let mgr = self.session.session_manager().await;
                let entries = mgr.entries();
                let cwd = self.session.cwd().to_string_lossy().to_string();
                drop(mgr);
                let new_id = format!("{:016x}", std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos());
                let new_path = if let Some(dir) = &self.session_dir {
                    dir.join(format!("{new_id}.jsonl"))
                } else {
                    PathBuf::from(format!("{new_id}.jsonl"))
                };
                match SessionManager::create(&new_path, new_id, cwd, None) {
                    Ok(mut new_mgr) => {
                        let mut count = 0u32;
                        for entry in &entries {
                            if let rozsa_app::session::manager::SessionEntry::Message(me) = entry {
                                if new_mgr.append_message(me.message.clone()).is_ok() {
                                    count += 1;
                                }
                            }
                        }
                        self.notify("info", &format!("Cloned {count} messages to new session: {}", new_path.display()));
                    }
                    Err(e) => self.notify("error", &format!("Clone failed: {e}")),
                }
            }
            "share" => {
                // Export to temp file, then gh gist create
                let mgr = self.session.session_manager().await;
                let entries = mgr.entries();
                drop(mgr);
                let tmp_path = std::env::temp_dir().join("rozsa-share-export.jsonl");
                let mut lines = Vec::with_capacity(entries.len());
                for entry in &entries {
                    if let Ok(json) = serde_json::to_string(entry) {
                        lines.push(json);
                    }
                }
                if let Err(e) = std::fs::write(&tmp_path, lines.join("\n") + "\n") {
                    self.notify("error", &format!("Share export failed: {e}"));
                } else {
                    match std::process::Command::new("gh")
                        .args(["gist", "create", "--public=false", &tmp_path.to_string_lossy()])
                        .output()
                    {
                        Ok(output) if output.status.success() => {
                            let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
                            self.notify("info", &format!("Shared as gist: {url}"));
                        }
                        Ok(output) => {
                            let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
                            self.notify("error", &format!("gh gist create failed: {err}"));
                        }
                        Err(e) => self.notify("error", &format!("Failed to run gh: {e}")),
                    }
                    let _ = std::fs::remove_file(&tmp_path);
                }
            }
            "scoped-models" => {
                if let Some(registry) = &self.model_registry {
                    let all = registry.all();
                    let lines: Vec<String> = all.iter().map(|m| {
                        format!("[{}] {}", m.provider, m.id)
                    }).collect();
                    self.notify("info", &format!("Available models ({}):\n{}", all.len(), lines.join("\n")));
                } else {
                    self.notify("warning", "No model registry available");
                }
            }
            "login" => {
                let event_tx_clone = self.event_tx.clone();
                tokio::spawn(async move {
                    use rozsa_model::oauth::openai_codex;
                    use rozsa_model::oauth::types::OAuthFlowEvent;
                    use rozsa_model::credentials::store_oauth_credentials;
                    use tokio::sync::mpsc as tokio_mpsc;
                    use tokio_util::sync::CancellationToken;

                    let notify = |level: &str, msg: &str| {
                        let _ = event_tx_clone.send(BackendEvent::Notify {
                            level: level.to_string(),
                            message: msg.to_string(),
                        });
                    };

                    notify("info", "Starting codex-oauth login...");

                    let (flow_event_tx, mut flow_event_rx) = tokio_mpsc::unbounded_channel();
                    let (_response_tx, response_rx) = tokio_mpsc::unbounded_channel();
                    let cancel = CancellationToken::new();

                    // Spawn the login flow
                    let login_handle = tokio::spawn(openai_codex::login(flow_event_tx, response_rx, cancel.clone()));

                    // Process flow events (show URL to user)
                    while let Some(event) = flow_event_rx.recv().await {
                        match event {
                            OAuthFlowEvent::AuthUrl { url, instructions } => {
                                let msg = if let Some(inst) = instructions {
                                    format!("{inst}\n\nURL: {url}")
                                } else {
                                    format!("Open this URL to login:\n{url}")
                                };
                                notify("info", &msg);
                                // Try to open browser via xdg-open / open
                                let _ = std::process::Command::new("xdg-open")
                                    .arg(&url)
                                    .stdin(std::process::Stdio::null())
                                    .stdout(std::process::Stdio::null())
                                    .stderr(std::process::Stdio::null())
                                    .spawn()
                                    .or_else(|_| {
                                        std::process::Command::new("open")
                                            .arg(&url)
                                            .stdin(std::process::Stdio::null())
                                            .stdout(std::process::Stdio::null())
                                            .stderr(std::process::Stdio::null())
                                            .spawn()
                                    });
                            }
                            OAuthFlowEvent::Progress { message } => {
                                notify("info", &message);
                            }
                            _ => {}
                        }
                    }

                    // Wait for login result
                    match login_handle.await {
                        Ok(Ok(credentials)) => {
                            let home = match dirs::home_dir() {
                                Some(h) => h,
                                None => {
                                    notify("error", "Cannot determine home directory");
                                    return;
                                }
                            };
                            let models_dir = home.join(".rozsa").join("models");
                            let _ = std::fs::create_dir_all(&models_dir);
                            let auth_path = models_dir.join("auth.json");

                            match store_oauth_credentials(
                                auth_path.to_str().unwrap_or(""),
                                "codex-oauth",
                                &credentials,
                            ) {
                                Ok(()) => {
                                    notify("info", "Login successful! Credentials saved.");
                                    // Auto-create codex-oauth.json if not exists
                                    ensure_codex_oauth_models_config(&models_dir);
                                }
                                Err(e) => {
                                    notify("error", &format!("Failed to save credentials: {e}"));
                                }
                            }
                        }
                        Ok(Err(e)) => {
                            notify("error", &format!("Login failed: {e}"));
                        }
                        Err(e) => {
                            notify("error", &format!("Login task panicked: {e}"));
                        }
                    }
                });
            }
            "logout" => {
                self.notify("info", "To clear provider credentials:\n- Unset the relevant environment variable (ANTHROPIC_API_KEY, OPENAI_API_KEY, etc.)\n- Or remove the credentials file from your system keychain\n\nRestart the session after clearing credentials.");
            }
            "usage" => {
                let event_tx_clone = self.event_tx.clone();
                tokio::spawn(async move {
                    let notify = |level: &str, msg: &str| {
                        let _ = event_tx_clone.send(BackendEvent::Notify {
                            level: level.to_string(),
                            message: msg.to_string(),
                        });
                    };
                    match rozsa_app::rate_limit::get_rate_limits().await {
                        Ok(snapshot) => {
                            let display = rozsa_app::rate_limit::format_rate_limit_display(&snapshot);
                            notify("info", &display);
                        }
                        Err(e) => {
                            notify("warning", &format!("Rate limit query failed: {e}"));
                        }
                    }
                });
            }
            _ => {
                use rozsa_app::slash_commands::BUILTIN_SLASH_COMMANDS;
                if BUILTIN_SLASH_COMMANDS.iter().any(|c| c.name == cmd) {
                    self.notify("warning", &format!("/{cmd} is not supported by the native TUI yet"));
                } else {
                    let session = self.session.clone();
                    // Normalize: if cmd matches a skill name (or is already skill:name), use /skill:name
                    let full_text = if cmd.starts_with("skill:") {
                        // Already normalized
                        if args.is_empty() { format!("/{cmd}") } else { format!("/{cmd} {args}") }
                    } else if session.skill_registry().find_by_name(cmd).is_some() {
                        // Skill name without prefix → normalize to /skill:name
                        if args.is_empty() { format!("/skill:{cmd}") } else { format!("/skill:{cmd} {args}") }
                    } else {
                        if args.is_empty() { format!("/{cmd}") } else { format!("/{cmd} {args}") }
                    };
                    let backend_tx = self.event_tx.clone();
                    tokio::spawn(async move {
                        if let Err(e) = session.prompt(&full_text).await {
                            let _ = backend_tx.send(BackendEvent::Notify {
                                level: "error".to_string(),
                                message: e.to_string(),
                            });
                        }
                    });
                }
            }
        }
        Ok(())
    }

    fn notify(&self, level: &str, message: &str) {
        let _ = self.event_tx.send(BackendEvent::Notify {
            level: level.to_string(),
            message: message.to_string(),
        });
    }

    async fn build_settings_options(&self) -> Vec<String> {
        let settings = self.runtime_settings.lock().await;
        let thinking = self.session.thinking_level().await;
        let on_off = |v: bool| if v { "on" } else { "off" };
        vec![
            format!("[AI] Thinking level: < {:?} >", thinking),
            format!("[AI] Auto compact: < {} >", on_off(settings.compaction.enabled)),
            format!("[AI] Steering mode: < {} >", settings.steering_mode),
            format!("[AI] Follow-up mode: < {} >", settings.follow_up_mode),
            format!("[Network] Transport: < {} >", settings.transport),
            format!("[Permission] Permission mode: < {} >", settings.permissions.mode),
            format!("[Display] Block images: < {} >", on_off(settings.block_images)),
        ]
    }

    /// 循环切换 settings 选项值 (direction: 1=right/next, -1=left/prev)
    async fn cycle_setting(&self, option_index: usize, direction: i32) {
        use rozsa_model::types::ThinkingLevel;

        fn cycle_str<'a>(opts: &[&'a str], current: &str, dir: i32) -> &'a str {
            let idx = opts.iter().position(|o| *o == current).unwrap_or(0);
            let next = if dir > 0 { (idx + 1) % opts.len() } else { (idx + opts.len() - 1) % opts.len() };
            opts[next]
        }

        match option_index {
            0 => {
                let levels = [ThinkingLevel::Off, ThinkingLevel::Low, ThinkingLevel::Medium, ThinkingLevel::High];
                let current = self.session.thinking_level().await;
                let idx = levels.iter().position(|l| *l == current).unwrap_or(0);
                let next = if direction > 0 { (idx + 1) % levels.len() } else { (idx + levels.len() - 1) % levels.len() };
                self.session.set_thinking_level(levels[next]).await;
                {
                    let mut s = self.runtime_settings.lock().await;
                    s.default_thinking_level = Some(levels[next]);
                }
                self.push_state().await;
            }
            1 => {
                let mut s = self.runtime_settings.lock().await;
                s.compaction.enabled = !s.compaction.enabled;
            }
            2 => {
                let mut s = self.runtime_settings.lock().await;
                let new_val = cycle_str(&["one-at-a-time", "all"], &s.steering_mode, direction);
                s.steering_mode = new_val.to_string();
            }
            3 => {
                let mut s = self.runtime_settings.lock().await;
                let new_val = cycle_str(&["one-at-a-time", "all"], &s.follow_up_mode, direction);
                s.follow_up_mode = new_val.to_string();
            }
            4 => {
                let mut s = self.runtime_settings.lock().await;
                let new_val = cycle_str(&["auto", "sse", "websocket", "websocket-cached"], &s.transport, direction);
                s.transport = new_val.to_string();
            }
            5 => {
                let mut s = self.runtime_settings.lock().await;
                let new_val = cycle_str(&["on-request", "auto-permission", "free-permission"], &s.permissions.mode, direction);
                s.permissions.mode = new_val.to_string();
            }
            6 => {
                let mut s = self.runtime_settings.lock().await;
                s.block_images = !s.block_images;
            }
            _ => {}
        }
        self.persist_settings().await;

        // 刷新 settings dialog 内容
        let options = self.build_settings_options().await;
        let _ = self.event_tx.send(BackendEvent::Dialog {
            id: "settings".to_string(),
            kind: "select".to_string(),
            title: "Settings (←/→ change, Tab switch category, Esc close)".to_string(),
            message: None,
            options,
            text: None,
            selected: Some(option_index),
        });
    }
}

/// Spawn a background task that subscribes to the AgentSession's event broadcaster,
/// updates LiveState, and forwards BackendEvent::State to the UI on every meaningful change.
fn spawn_event_forwarder(
    session: Arc<AgentSession>,
    backend_tx: mpsc::UnboundedSender<BackendEvent>,
    live: Arc<Mutex<LiveState>>,
) {
    let mut rx = session.subscribe();
    let session_for_state = session.clone();
    tokio::spawn(async move {
        loop {
            let event = match rx.recv().await {
                Ok(ev) => ev,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            };

            let state_dirty = {
                let mut live = live.lock().await;
                apply_event(&mut live, &event)
            };

            if state_dirty {
                let live = live.lock().await;
                push_state_with(&session_for_state, &live, &backend_tx).await;
            }
        }
    });
}

async fn push_state_with(
    session: &AgentSession,
    live: &LiveState,
    backend_tx: &mpsc::UnboundedSender<BackendEvent>,
) {
    let messages = live.messages.clone();

    // 累积 token 统计
    let mut input_tokens: u64 = 0;
    let mut output_tokens: u64 = 0;
    for msg in &live.messages {
        if let Some(rozsa_model::types::Message::Assistant(a)) = msg.as_standard() {
            input_tokens += a.usage.input;
            output_tokens += a.usage.output;
        }
    }
    let total_tokens = input_tokens + output_tokens;

    let model = session.model().await;
    let thinking = session.thinking_level().await;

    // context usage: 输入 token 占 context window 的比例
    let context_window = model.context_window as f64;
    let context_percent = if context_window > 0.0 {
        (input_tokens as f64 / context_window) * 100.0
    } else {
        0.0
    };

    let runtime_state = serde_json::json!({
        "modelUsage": {
            "promptTokens": input_tokens,
            "completionTokens": output_tokens,
            "sessionTotalTokens": total_tokens,
        }
    });

    let context_usage = serde_json::json!({
        "percent": context_percent,
        "tokens": input_tokens,
        "contextWindow": model.context_window,
    });

    let session_name = session.session_manager().await.current_name();

    let state = NativeUiState {
        app_name: "rozsa".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        cwd: session.cwd().to_string_lossy().to_string(),
        session_name,
        model: Some(ModelInfo {
            id: model.id.clone(),
            provider: format!("{:?}", model.provider),
        }),
        thinking_level: format!("{:?}", thinking),
        is_streaming: live.is_streaming,
        is_compacting: session.is_compacting(),
        hide_thinking: live.hide_thinking || thinking == rozsa_model::types::ThinkingLevel::Off,
        show_images: session.show_images(),
        messages,
        pending_messages: session.pending_messages(),
        status: BTreeMap::new(),
        widgets_above: BTreeMap::new(),
        widgets_below: BTreeMap::new(),
        stats: None,
        runtime_state: Some(runtime_state),
        context_usage: Some(context_usage),
        keybindings: default_keybindings(),
        error: None,
    };
    let _ = backend_tx.send(BackendEvent::State(state));
}

fn default_keybindings() -> BTreeMap<String, Vec<String>> {
    let mut kb = BTreeMap::new();
    kb.insert("tui.input.submit".into(), vec!["enter".into()]);
    kb.insert("tui.select.cancel".into(), vec!["escape".into()]);
    kb.insert("tui.select.confirm".into(), vec!["enter".into()]);
    kb.insert("tui.select.up".into(), vec!["up".into()]);
    kb.insert("tui.select.down".into(), vec!["down".into()]);
    kb.insert("tui.select.pageUp".into(), vec!["pageup".into()]);
    kb.insert("tui.select.pageDown".into(), vec!["pagedown".into()]);
    kb.insert("app.interrupt".into(), vec!["escape".into()]);
    kb.insert("app.exit".into(), vec!["ctrl+d".into()]);
    kb.insert("app.model.cycleForward".into(), vec!["ctrl+p".into()]);
    kb.insert("app.model.cycleBackward".into(), vec!["ctrl+shift+p".into()]);
    kb.insert("app.model.select".into(), vec!["ctrl+l".into()]);
    kb.insert("app.thinking.toggle".into(), vec!["ctrl+t".into()]);
    kb.insert("app.suspend".into(), vec!["ctrl+z".into()]);
    kb.insert("app.tools.expand".into(), vec!["ctrl+o".into()]);
    kb.insert("app.subagent.next".into(), vec!["ctrl+]".into()]);
    kb.insert("app.subagent.previous".into(), vec!["alt+[".into()]);
    kb.insert("app.editMode.cycle".into(), vec!["shift+tab".into()]);
    kb.insert("app.theme.toggle".into(), vec!["alt+t".into()]);
    kb.insert("app.editor.external".into(), vec!["ctrl+g".into()]);
    kb.insert("tui.editor.cursorWordRight".into(), vec!["alt+f".into()]);
    kb.insert("tui.editor.cursorWordLeft".into(), vec!["alt+b".into()]);
    kb
}

#[async_trait]
impl AgentBackend for NativeBackend {
    async fn submit(&self, text: &str, _images: Vec<ImageData>) -> BackendResult<()> {
        // Slash command 拦截
        if let Some(rest) = text.strip_prefix('/') {
            let rest = rest.trim();
            let (cmd, args) = match rest.split_once(char::is_whitespace) {
                Some((c, a)) => (c, a.trim()),
                None => (rest, ""),
            };
            return self.dispatch_slash_command(cmd, args).await;
        }

        // Bang command 拦截：`!command` 直接在 shell 执行，不发给 agent
        if let Some(rest) = text.strip_prefix('!') {
            let exclude_from_context = rest.starts_with('!');
            let command = if exclude_from_context {
                rest.strip_prefix('!').unwrap_or(rest).trim()
            } else {
                rest.trim()
            };
            if command.is_empty() {
                return Ok(());
            }
            return self.execute_bang_command(command, exclude_from_context).await;
        }

        let session = self.session.clone();
        let text = text.to_string();
        let backend_tx = self.event_tx.clone();
        tokio::spawn(async move {
            if let Err(e) = session.prompt(&text).await {
                let _ = backend_tx.send(BackendEvent::Notify {
                    level: "error".to_string(),
                    message: e.to_string(),
                });
            }
        });
        Ok(())
    }

    async fn abort(&self) -> BackendResult<()> {
        self.session.abort().await;
        Ok(())
    }

    async fn follow_up(&self, text: &str, _images: Vec<ImageData>) -> BackendResult<()> {
        self.session.follow_up(text);
        self.push_state().await;
        Ok(())
    }

    async fn steer(&self, text: &str, _images: Vec<ImageData>) -> BackendResult<()> {
        self.session.steer(text);
        self.push_state().await;
        Ok(())
    }

    async fn list_models(&self) -> BackendResult<()> {
        let entries: Vec<ModelEntry> = if let Some(registry) = &self.model_registry {
            let current = self.session.model().await;
            let available = registry.provider_available();
            registry
                .all()
                .iter()
                .filter(|m| {
                    available
                        .get(m.provider.as_str())
                        .is_some_and(|pa| pa.configured)
                })
                .map(|m| ModelEntry {
                    id: m.id.clone(),
                    provider: m.provider.to_string(),
                    is_current: m.id == current.id && m.provider == current.provider,
                })
                .collect()
        } else {
            Vec::new()
        };
        let _ = self.event_tx.send(BackendEvent::Models(entries));
        Ok(())
    }

    async fn switch_model(&self, provider: &str, id: &str) -> BackendResult<()> {
        let Some(registry) = &self.model_registry else {
            return Err(BackendError::Internal(
                "model registry not configured".into(),
            ));
        };
        let Some(model) = registry.resolve(provider, id) else {
            return Err(BackendError::Protocol(format!(
                "model {provider}/{id} not found"
            )));
        };
        self.session.set_model(model).await;
        {
            let mut s = self.runtime_settings.lock().await;
            s.default_provider = Some(provider.to_string());
            s.default_model = Some(id.to_string());
        }
        self.persist_settings().await;
        self.push_state().await;
        Ok(())
    }

    async fn cycle_model(&self, direction: Direction) -> BackendResult<()> {
        if let Some(model) = self.neighbor_model(direction).await {
            let model_id = model.id.clone();
            let provider = format!("{:?}", model.provider).to_lowercase();
            self.session.set_model(model).await;
            {
                let mut s = self.runtime_settings.lock().await;
                s.default_provider = Some(provider);
                s.default_model = Some(model_id);
            }
            self.persist_settings().await;
            self.push_state().await;
        }
        Ok(())
    }

    async fn list_sessions(&self) -> BackendResult<()> {
        let entries: Vec<SessionEntry> = if let Some(dir) = &self.session_dir {
            SessionManager::list_dir(dir)
                .unwrap_or_default()
                .into_iter()
                .map(|m| SessionEntry {
                    path: m.path.to_string_lossy().to_string(),
                    name: m.name,
                    first_message: m.first_message,
                    cwd: m.cwd,
                    message_count: m.message_count,
                    last_modified: m.modified,
                    parent_session_path: m.parent_session_path,
                    all_messages_text: m.all_messages_text,
                })
                .collect()
        } else {
            Vec::new()
        };

        let current = self
            .session
            .session_manager()
            .await
            .session_file()
            .to_string_lossy()
            .to_string();

        let _ = self.event_tx.send(BackendEvent::Sessions {
            entries,
            current_session_path: current,
        });
        Ok(())
    }

    async fn switch_session(&self, path: &str) -> BackendResult<()> {
        match self.session.switch_session(path).await {
            Ok(_old) => {
                // Reload messages from new session into live state
                let mgr = self.session.session_manager().await;
                let entries = mgr.entries();
                drop(mgr);

                let mut messages = Vec::new();
                for entry in entries {
                    if let rozsa_app::session::manager::SessionEntry::Message(msg_entry) = entry {
                        messages.push(AgentMessage::standard(msg_entry.message));
                    }
                }

                let mut live = self.live.lock().await;
                live.messages = messages;
                live.turn_base = live.messages.len();
                live.is_streaming = false;
                drop(live);

                // 刷新 UI 显示新 session 的对话
                self.push_state().await;
                self.notify("info", &format!("Switched to session: {}", path));
            }
            Err(e) => {
                let _ = self.event_tx.send(BackendEvent::Notify {
                    level: "error".to_string(),
                    message: format!("Failed to switch session: {e}"),
                });
            }
        }
        Ok(())
    }

    async fn delete_session(&self, path: &str) -> BackendResult<()> {
        let path_owned = path.to_string();
        let result = SessionManager::delete(&path_owned);
        let event = match result {
            Ok(()) => BackendEvent::SessionDeleted {
                path: path_owned,
                method: "deleted".into(),
                error: None,
            },
            Err(e) => BackendEvent::SessionDeleted {
                path: path_owned,
                method: "error".into(),
                error: Some(e.to_string()),
            },
        };
        let _ = self.event_tx.send(event);
        // Refresh list
        self.list_sessions().await
    }

    async fn rename_session(&self, path: &str, name: &str) -> BackendResult<()> {
        // If renaming the active session, append in-place; else open the file
        // and append a session_info entry there.
        let active_path = self
            .session
            .session_manager()
            .await
            .session_file()
            .to_path_buf();

        let new_name = if name.trim().is_empty() {
            None
        } else {
            Some(name.to_string())
        };

        let result = if active_path == Path::new(path) {
            self.session
                .session_manager()
                .await
                .append_session_info(new_name)
                .map(|_| ())
        } else {
            SessionManager::rename(path, new_name)
        };

        if let Err(e) = result {
            let _ = self.event_tx.send(BackendEvent::Notify {
                level: "error".to_string(),
                message: format!("rename failed: {e}"),
            });
        }
        self.list_sessions().await
    }

    async fn fork_session(&self, message_index: usize) -> BackendResult<()> {
        let mgr = self.session.session_manager().await;
        let entries = mgr.entries();
        let cwd = self.session.cwd().to_string_lossy().to_string();
        drop(mgr);

        // Collect messages up to and including the selected index
        let messages: Vec<_> = entries
            .iter()
            .filter_map(|e| {
                if let rozsa_app::session::manager::SessionEntry::Message(me) = e {
                    Some(me.message.clone())
                } else {
                    None
                }
            })
            .collect();

        let fork_messages = if message_index < messages.len() {
            &messages[..=message_index]
        } else {
            &messages[..]
        };

        let new_id = format!(
            "{:016x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let new_path = if let Some(dir) = &self.session_dir {
            dir.join(format!("{new_id}.jsonl"))
        } else {
            PathBuf::from(format!("{new_id}.jsonl"))
        };

        match SessionManager::create(&new_path, new_id, cwd, None) {
            Ok(mut new_mgr) => {
                let mut count = 0u32;
                for msg in fork_messages {
                    if new_mgr.append_message(msg.clone()).is_ok() {
                        count += 1;
                    }
                }
                drop(new_mgr);
                self.notify(
                    "info",
                    &format!(
                        "Forked {count} messages to new session: {}",
                        new_path.display()
                    ),
                );
                self.switch_session(&new_path.to_string_lossy()).await?;
            }
            Err(e) => self.notify("error", &format!("Fork failed: {e}")),
        }
        Ok(())
    }

    async fn respond_permission(
        &self,
        id: &str,
        choice: &str,
        trust_key: Option<&str>,
    ) -> BackendResult<()> {
        use rozsa_app::permissions::PermissionResponse;

        let Some(ref approvals) = self.pending_approvals else {
            return Ok(());
        };

        let response = match choice {
            "allow" => PermissionResponse::Allow,
            "allow-session" => {
                if let Some(key) = trust_key {
                    PermissionResponse::AllowSession {
                        trust_key: key.to_string(),
                    }
                } else {
                    PermissionResponse::Allow
                }
            }
            _ => PermissionResponse::Deny,
        };

        if let Some((_, sender)) = approvals.remove(id) {
            let _ = sender.send(response);
        }
        Ok(())
    }

    async fn run_bash(&self, command: &str) -> BackendResult<()> {
        self.submit(&format!("!{command}"), vec![]).await
    }

    async fn compact(&self) -> BackendResult<()> {
        let _ = self.event_tx.send(BackendEvent::Compacting(true));
        let session = self.session.clone();
        let event_tx = self.event_tx.clone();
        tokio::spawn(async move {
            match session.compact().await {
                Ok(result) => {
                    let _ = event_tx.send(BackendEvent::Notify {
                        level: "info".to_string(),
                        message: format!(
                            "Compacted: removed {} messages, summary generated",
                            result.removed_count
                        ),
                    });
                }
                Err(e) => {
                    let _ = event_tx.send(BackendEvent::Notify {
                        level: "warn".to_string(),
                        message: format!("Compaction failed: {e}"),
                    });
                }
            }
            let _ = event_tx.send(BackendEvent::Compacting(false));
        });
        Ok(())
    }

    async fn cycle_edit_mode(&self) -> BackendResult<()> {
        let new_mode = self.session.cycle_edit_mode().await;
        let _ = self.event_tx.send(BackendEvent::Notify {
            level: "info".to_string(),
            message: format!("Edit mode: {:?}", new_mode),
        });
        self.push_state().await;
        Ok(())
    }

    async fn switch_agent(&self, id: &str) -> BackendResult<()> {
        if id == "main" {
            self.session.set_viewing_subagent(None).await;
        } else {
            let mgr = self.session.subagent_manager().await;
            if mgr.snapshot(id).await.is_none() {
                return Err(BackendError::Internal(format!(
                    "Subagent '{}' not found",
                    id
                )));
            }
            drop(mgr);
            self.session.set_viewing_subagent(Some(id.to_string())).await;
        }
        self.push_state().await;
        Ok(())
    }

    async fn dialog_response(
        &self,
        id: &str,
        value: Option<&str>,
        _confirmed: Option<bool>,
        cancelled: Option<bool>,
    ) -> BackendResult<()> {
        if cancelled == Some(true) {
            return Ok(());
        }
        // Route dialog responses to specific handlers based on dialog id prefix.
        if id.starts_with("setting:") {
            if let Some(val) = value {
                let key = id.strip_prefix("setting:").unwrap_or(id);
                self.update_setting(key, val).await?;
            }
        }
        Ok(())
    }

    async fn autocomplete_request(
        &self,
        text: &str,
        cursor: usize,
        _force: bool,
    ) -> BackendResult<()> {
        use rozsa_app::slash_commands::{AutocompleteEngine, SlashCommandInfo, SlashCommandSource};

        // Build dynamic commands from skills
        let skill_commands: Vec<SlashCommandInfo> = self
            .session
            .skill_registry()
            .list()
            .iter()
            .map(|skill| {
                let builtin_conflict = rozsa_app::slash_commands::BUILTIN_SLASH_COMMANDS
                    .iter()
                    .any(|c| c.name == skill.name);
                let name = if builtin_conflict {
                    format!("skill:{}", skill.name)
                } else {
                    skill.name.clone()
                };
                SlashCommandInfo {
                    name,
                    description: Some(skill.description.clone()),
                    source: SlashCommandSource::Skill,
                }
            })
            .collect();

        let engine = AutocompleteEngine::with_dynamic(skill_commands);
        let items: Vec<crate::protocol::NativeAutocompleteItem> = engine
            .complete(text, cursor)
            .unwrap_or_default()
            .into_iter()
            .map(|i| crate::protocol::NativeAutocompleteItem {
                value: i.value,
                label: i.label,
                description: i.description,
            })
            .collect();

        // Echo back the prefix for the UI to verify staleness.
        let prefix = text.get(..cursor).unwrap_or("").to_string();

        let id = self.autocomplete_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let _ = self.event_tx.send(BackendEvent::Autocomplete {
            id,
            prefix,
            items,
        });
        Ok(())
    }

    async fn update_setting(&self, key: &str, value: &str) -> BackendResult<()> {
        match key {
            "__cycle_setting" => {
                // value format: "index:direction" (e.g. "0:1" or "2:-1")
                let parts: Vec<&str> = value.splitn(2, ':').collect();
                if parts.len() == 2 {
                    let index: usize = parts[0].parse().unwrap_or(0);
                    let direction: i32 = parts[1].parse().unwrap_or(1);
                    self.cycle_setting(index, direction).await;
                }
                Ok(())
            }
            "thinking_level" => {
                use rozsa_model::types::ThinkingLevel;
                let level = match value {
                    "off" | "Off" => ThinkingLevel::Off,
                    "low" | "Low" => ThinkingLevel::Low,
                    "medium" | "Medium" => ThinkingLevel::Medium,
                    "high" | "High" => ThinkingLevel::High,
                    other => {
                        return Err(BackendError::Protocol(format!(
                            "unknown thinking level: {other}"
                        )))
                    }
                };
                self.session.set_thinking_level(level).await;
                {
                    let mut s = self.runtime_settings.lock().await;
                    s.default_thinking_level = Some(level);
                }
                self.persist_settings().await;
                self.push_state().await;
                Ok(())
            }
            "hide_thinking" => {
                let new_val = {
                    let mut s = self.runtime_settings.lock().await;
                    if value == "toggle" {
                        s.hide_thinking = !s.hide_thinking;
                    } else {
                        s.hide_thinking = value == "true";
                    }
                    s.hide_thinking
                };
                {
                    let mut live = self.live.lock().await;
                    live.hide_thinking = new_val;
                }
                self.persist_settings().await;
                self.push_state().await;
                Ok(())
            }
            "theme" => {
                // theme 在 TUI 层本地处理，这里只同步状态
                self.push_state().await;
                Ok(())
            }
            _ => {
                let _ = self.event_tx.send(BackendEvent::Notify {
                    level: "info".to_string(),
                    message: format!("setting `{key}` is not yet supported in native mode"),
                });
                Ok(())
            }
        }
    }

    async fn connect(&self) -> BackendResult<()> {
        self.push_state().await;
        Ok(())
    }

    async fn disconnect(&self) -> BackendResult<()> {
        Ok(())
    }

    async fn exit(&self) -> BackendResult<()> {
        let _ = self.event_tx.send(BackendEvent::Shutdown);
        Ok(())
    }

    fn events(&self) -> mpsc::UnboundedReceiver<BackendEvent> {
        self.event_rx
            .try_lock()
            .ok()
            .and_then(|mut guard| guard.take())
            .expect("events() called more than once")
    }
}

impl SubagentView for NativeBackend {
    fn list_subagents_sync(&self) -> Vec<SubagentInfo> {
        match self.session.subagent_manager_try_lock() {
            Some(mgr) => mgr.list_sync(),
            None => Vec::new(),
        }
    }

    fn viewing_subagent_id_sync(&self) -> Option<String> {
        self.session.viewing_subagent_id_try_lock()
    }
}

/// Auto-create ~/.rozsa/models/codex-oauth.json with default GPT models if not exists.
fn ensure_codex_oauth_models_config(models_dir: &std::path::Path) {
    let config_path = models_dir.join("codex-oauth.json");
    if config_path.exists() {
        return;
    }
    let default_config = serde_json::json!({
        "providers": {
            "codex-oauth": {
                "baseUrl": "https://api.openai.com/v1",
                "api": "openai-responses",
                "authHeader": true,
                "models": [
                    {
                        "id": "gpt-4o",
                        "name": "GPT-4o",
                        "contextWindow": 128000,
                        "maxTokens": 16384,
                        "reasoning": false,
                        "input": ["text", "image"],
                        "cost": { "input": 2.5, "output": 10.0, "cacheRead": 1.25, "cacheWrite": 0.0 }
                    },
                    {
                        "id": "gpt-4o-mini",
                        "name": "GPT-4o Mini",
                        "contextWindow": 128000,
                        "maxTokens": 16384,
                        "reasoning": false,
                        "input": ["text", "image"],
                        "cost": { "input": 0.15, "output": 0.6, "cacheRead": 0.075, "cacheWrite": 0.0 }
                    },
                    {
                        "id": "o3",
                        "name": "o3",
                        "contextWindow": 200000,
                        "maxTokens": 100000,
                        "reasoning": true,
                        "input": ["text", "image"],
                        "cost": { "input": 2.0, "output": 8.0, "cacheRead": 1.0, "cacheWrite": 0.0 }
                    },
                    {
                        "id": "o4-mini",
                        "name": "o4-mini",
                        "contextWindow": 200000,
                        "maxTokens": 100000,
                        "reasoning": true,
                        "input": ["text", "image"],
                        "cost": { "input": 1.1, "output": 4.4, "cacheRead": 0.55, "cacheWrite": 0.0 }
                    },
                    {
                        "id": "codex-mini-latest",
                        "name": "Codex Mini",
                        "contextWindow": 200000,
                        "maxTokens": 100000,
                        "reasoning": true,
                        "input": ["text"],
                        "cost": { "input": 1.5, "output": 6.0, "cacheRead": 0.75, "cacheWrite": 0.0 }
                    }
                ]
            }
        }
    });
    let _ = std::fs::write(
        &config_path,
        serde_json::to_string_pretty(&default_config).unwrap_or_default(),
    );
}

// Suppress unused warnings on imports retained for future use:
#[allow(dead_code)]
fn _unused_marker(_: BackendError) {}
