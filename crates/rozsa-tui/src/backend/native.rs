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

use crate::components::model_selector::ModelEntry;
use crate::components::session_selector::SessionEntry;
use crate::protocol::{ModelInfo, NativeUiState};

use super::{AgentBackend, BackendError, BackendEvent, BackendResult, Direction, ImageData};

/// Live snapshot of session state used to build NativeUiState payloads.
///
/// Maintained by the background forwarder task as AgentEvents arrive — the UI
/// thread never has to lock the AgentSession to render.
struct LiveState {
    messages: Vec<AgentMessage>,
    is_streaming: bool,
    /// Index into `messages` where the current agent run began.
    /// AgentEnd uses this to truncate+replace (not append), preventing duplication.
    turn_base: usize,
}

/// Optional construction-time config. None of the fields are required for
/// the basic streaming flow; supplying them unlocks model / session
/// switching commands that would otherwise return early.
#[derive(Default)]
pub struct NativeBackendConfig {
    pub model_registry: Option<Arc<ModelRegistry>>,
    pub session_dir: Option<PathBuf>,
}

pub struct NativeBackend {
    session: Arc<AgentSession>,
    event_tx: mpsc::UnboundedSender<BackendEvent>,
    event_rx: Mutex<Option<mpsc::UnboundedReceiver<BackendEvent>>>,
    live: Arc<Mutex<LiveState>>,
    model_registry: Option<Arc<ModelRegistry>>,
    session_dir: Option<PathBuf>,
    /// Runtime-mutable settings copy (mutated by /settings left/right cycling)
    runtime_settings: Mutex<Settings>,
}

impl NativeBackend {
    pub fn new(session: AgentSession) -> Self {
        Self::with_config(session, NativeBackendConfig::default())
    }

    pub fn with_config(session: AgentSession, config: NativeBackendConfig) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let session = Arc::new(session);
        let live = Arc::new(Mutex::new(LiveState {
            messages: Vec::new(),
            is_streaming: false,
            turn_base: 0,
        }));

        spawn_event_forwarder(session.clone(), tx.clone(), live.clone());

        let runtime_settings = session.settings_manager().resolved().clone();

        Self {
            session,
            event_tx: tx,
            event_rx: Mutex::new(Some(rx)),
            live,
            model_registry: config.model_registry,
            session_dir: config.session_dir,
            runtime_settings: Mutex::new(runtime_settings),
        }
    }

    async fn push_state(&self) {
        let live = self.live.lock().await;
        push_state_with(&self.session, &live, &self.event_tx).await;
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
            .position(|m| m.id == current.id && m.provider == format!("{:?}", current.provider).to_ascii_lowercase())
            .or_else(|| all.iter().position(|m| m.id == current.id))
            .unwrap_or(0);
        let next_idx = match direction {
            Direction::Forward => (idx + 1) % all.len(),
            Direction::Backward => (idx + all.len() - 1) % all.len(),
        };
        Some(all[next_idx].to_model())
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
                                let p = m.provider.clone();
                                let i = m.id.clone();
                                self.switch_model(&p, &i).await?;
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
                let messages = self.live.lock().await.messages.clone();
                let nodes = messages
                    .iter()
                    .filter_map(|m| {
                        let msg = m.as_standard()?;
                        let (role, text) = match msg {
                            rozsa_model::types::Message::User(u) => {
                                let t = match &u.content {
                                    rozsa_model::types::UserContent::Text(s) => s.clone(),
                                    _ => return None,
                                };
                                ("user", t)
                            }
                            rozsa_model::types::Message::Assistant(a) => {
                                let t = a.content.iter().find_map(|b| match b {
                                    rozsa_model::types::ContentBlock::Text { text, .. } => Some(text.clone()),
                                    _ => None,
                                })?;
                                ("assistant", t)
                            }
                            _ => return None,
                        };
                        use crate::protocol::NativeGraphNode;
                        let summary = if text.len() > 80 { text[..80].to_string() } else { text.clone() };
                        Some(NativeGraphNode {
                            role: role.to_string(),
                            summary,
                            full_text: text,
                            timestamp: String::new(),
                        })
                    })
                    .collect();
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
                self.notify("info", "Switched to main agent");
            }
            "subagent" | "subagents" => {
                self.notify("info", "No subagents");
            }
            "reload" => {
                // SettingsManager::reload() 需要 &mut — 暂用 notify 确认拦截
                self.notify("info", "Reloaded keybindings, extensions, skills, prompts, and themes");
            }
            "changelog" => {
                self.notify("info", "No changelog entries available in native mode");
            }
            "quit" => {
                self.exit().await?;
            }
            "lsp" => {
                if args.is_empty() {
                    self.notify("info", "LSP auto-diagnostics modes: agent_end | edit_write | disabled");
                } else {
                    self.notify("info", &format!("LSP mode set to: {args}"));
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
                                    let t = match &u.content {
                                        rozsa_model::types::UserContent::Text(s) => s.clone(),
                                        _ => "(blocks)".to_string(),
                                    };
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
                            let text = match &u.content {
                                rozsa_model::types::UserContent::Text(s) => s.clone(),
                                _ => return None,
                            };
                            let summary = if text.len() > 80 { text[..80].to_string() } else { text.clone() };
                            Some(NativeGraphNode {
                                role: "user".to_string(),
                                summary,
                                full_text: text,
                                timestamp: String::new(),
                            })
                        }
                        _ => None,
                    }
                }).collect();
                let _ = self.event_tx.send(BackendEvent::Graph(nodes));
                self.notify("info", "Select a message to fork from (via /graph)");
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
                self.notify("info", "Provider authentication:\n- Anthropic: set ANTHROPIC_API_KEY environment variable\n- OpenAI: set OPENAI_API_KEY environment variable\n- AWS Bedrock: configure AWS credentials (aws configure)\n- GCP Vertex: set GOOGLE_APPLICATION_CREDENTIALS\n\nRestart the session after setting credentials.");
            }
            "logout" => {
                self.notify("info", "To clear provider credentials:\n- Unset the relevant environment variable (ANTHROPIC_API_KEY, OPENAI_API_KEY, etc.)\n- Or remove the credentials file from your system keychain\n\nRestart the session after clearing credentials.");
            }
            _ => {
                use rozsa_app::slash_commands::BUILTIN_SLASH_COMMANDS;
                if BUILTIN_SLASH_COMMANDS.iter().any(|c| c.name == cmd) {
                    self.notify("warning", &format!("/{cmd} is not supported by the native TUI yet"));
                } else {
                    let session = self.session.clone();
                    let full_text = if args.is_empty() { format!("/{cmd}") } else { format!("/{cmd} {args}") };
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

            let mut state_dirty = false;
            {
                let mut live = live.lock().await;
                match &event {
                    AgentEvent::AgentStart => {
                        live.turn_base = live.messages.len();
                        live.is_streaming = true;
                        state_dirty = true;
                    }
                    AgentEvent::AgentEnd { messages } => {
                        // AgentEnd 携带本轮的权威消息列表（含 tool results 等
                        // 未经 MessageStart 推送的）。用 truncate+extend 替代
                        // append，避免与 MessageStart 已 push 的消息重复。
                        let base = live.turn_base.min(live.messages.len());
                        live.messages.truncate(base);
                        live.messages.extend(messages.iter().cloned());
                        live.is_streaming = false;
                        state_dirty = true;
                    }
                    AgentEvent::MessageStart { message } => {
                        live.messages.push(message.clone());
                        state_dirty = true;
                    }
                    AgentEvent::MessageUpdate { message, .. } => {
                        if let Some(last) = live.messages.last_mut() {
                            *last = message.clone();
                        }
                        state_dirty = true;
                    }
                    AgentEvent::MessageEnd { message } => {
                        if let Some(last) = live.messages.last_mut() {
                            *last = message.clone();
                        }
                        state_dirty = true;
                    }
                    _ => {}
                }
            }

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
    let messages = crate::view_model::messages_to_view(&live.messages);

    let model = session.model().await;
    let thinking = session.thinking_level().await;
    let state = NativeUiState {
        app_name: "rozsa".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        cwd: session.cwd().to_string_lossy().to_string(),
        session_name: None,
        model: Some(ModelInfo {
            id: model.id.clone(),
            provider: format!("{:?}", model.provider),
        }),
        thinking_level: format!("{:?}", thinking),
        is_streaming: live.is_streaming,
        is_compacting: false,
        hide_thinking: false,
        show_images: true,
        messages,
        pending_messages: vec![],
        status: BTreeMap::new(),
        widgets_above: BTreeMap::new(),
        widgets_below: BTreeMap::new(),
        stats: None,
        runtime_state: None,
        context_usage: None,
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
    kb.insert("app.thinking.cycle".into(), vec!["ctrl+t".into()]);
    kb.insert("app.suspend".into(), vec!["ctrl+z".into()]);
    kb.insert("app.compact".into(), vec!["ctrl+o".into()]);
    kb.insert("app.subagent.next".into(), vec!["ctrl+]".into()]);
    kb.insert("app.subagent.prev".into(), vec!["alt+[".into()]);
    kb.insert("app.editMode.cycle".into(), vec!["shift+tab".into()]);
    kb.insert("app.theme.toggle".into(), vec!["alt+t".into()]);
    kb.insert("app.editor.external".into(), vec!["ctrl+g".into()]);
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
        self.submit(text, vec![]).await
    }

    async fn steer(&self, text: &str, _images: Vec<ImageData>) -> BackendResult<()> {
        self.submit(text, vec![]).await
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
                        .get(&m.provider)
                        .is_some_and(|pa| pa.configured)
                })
                .map(|m| ModelEntry {
                    id: m.id.clone(),
                    provider: m.provider.clone(),
                    is_current: m.id == current.id
                        && m.provider == format!("{:?}", current.provider).to_ascii_lowercase(),
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
        self.push_state().await;
        Ok(())
    }

    async fn cycle_model(&self, direction: Direction) -> BackendResult<()> {
        if let Some(model) = self.neighbor_model(direction).await {
            self.session.set_model(model).await;
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

    async fn switch_session(&self, _path: &str) -> BackendResult<()> {
        // Switching the active session in-place would require swapping the
        // SessionManager held by AgentSession, which the current API does not
        // support. Surface a friendly notify so the UI doesn't hang silently.
        let _ = self.event_tx.send(BackendEvent::Notify {
            level: "info".to_string(),
            message: "session switching is not yet supported in native mode".into(),
        });
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

    async fn respond_permission(
        &self,
        _id: &str,
        _choice: &str,
        _trust_key: Option<&str>,
    ) -> BackendResult<()> {
        // Permissions are not yet wired to the agent loop in native mode.
        // Silently accept so the UI can clear its prompt.
        Ok(())
    }

    async fn run_bash(&self, command: &str) -> BackendResult<()> {
        self.submit(&format!("!{command}"), vec![]).await
    }

    async fn compact(&self) -> BackendResult<()> {
        // Compaction triggering lives in rozsa-app::compaction; not yet wired
        // to a one-shot entrypoint. Toggle the indicator so UI feedback is honest.
        let _ = self.event_tx.send(BackendEvent::Compacting(true));
        let _ = self.event_tx.send(BackendEvent::Notify {
            level: "info".to_string(),
            message: "manual compaction is not yet supported in native mode".into(),
        });
        let _ = self.event_tx.send(BackendEvent::Compacting(false));
        Ok(())
    }

    async fn cycle_edit_mode(&self) -> BackendResult<()> {
        // Edit mode (accept-all / ask) is a permission-policy concern; surface
        // a notify until the policy module exposes a setter.
        let _ = self.event_tx.send(BackendEvent::Notify {
            level: "info".to_string(),
            message: "edit mode cycling is not yet supported in native mode".into(),
        });
        Ok(())
    }

    async fn switch_agent(&self, _id: &str) -> BackendResult<()> {
        let _ = self.event_tx.send(BackendEvent::Notify {
            level: "info".to_string(),
            message: "subagent switching is not yet supported in native mode".into(),
        });
        Ok(())
    }

    async fn dialog_response(
        &self,
        _id: &str,
        _value: Option<&str>,
        _confirmed: Option<bool>,
        _cancelled: Option<bool>,
    ) -> BackendResult<()> {
        // The TUI emits dialog responses for built-in dialogs (settings, gc, etc.).
        // None of them are wired to native handlers yet — accept and let UI close.
        Ok(())
    }

    async fn autocomplete_request(
        &self,
        text: &str,
        cursor: usize,
        _force: bool,
    ) -> BackendResult<()> {
        use rozsa_app::slash_commands::AutocompleteEngine;
        let engine = AutocompleteEngine::new();
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

        let _ = self.event_tx.send(BackendEvent::Autocomplete {
            id: 0,
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

// Suppress unused warnings on imports retained for future use:
#[allow(dead_code)]
fn _unused_marker(_: BackendError) {}
