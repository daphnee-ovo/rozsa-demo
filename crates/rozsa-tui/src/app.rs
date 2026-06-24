// app.rs — 应用主循环 (async event loop with tokio::select!)
//
// 内部结构:
// app.rs
// ├── AppState              # 应用状态（单任务独占）
// ├── DialogState           # 对话框状态
// ├── Notification          # 通知消息
// ├── RetryState            # 重试倒计时
// ├── run()                 # async 入口
// ├── run_app()             # async 事件循环（select! 多路复用）
// └── apply_backend_event() # 将 BackendEvent 映射到 AppState
//
// 相关文档:
// - [SPEC](../../../dev-doc/refactor/tui/SPEC.md)

use std::{
    env,
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};

use crossterm::{
    event::{Event, EventStream, KeyEvent, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

use crate::{
    backend::{AgentBackend, BackendEvent, socket::SocketBackend},
    components::autocomplete::AutocompleteState,
    components::graph::GraphState,
    components::model_selector::ModelSelectorState,
    components::permission::PermissionState,
    components::session_selector::SessionSelectorState,
    input::{InputState, handle_key},
    keymap::KeybindingsManager,
    protocol::{ClientMessage, NativeUiState},
    ui::render,
};

#[derive(Clone, Debug)]
pub struct DialogState {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub message: Option<String>,
    pub options: Vec<String>,
    pub selected: usize,
    pub input: String,
    /// Tab 筛选（仅 settings dialog 使用）
    pub active_tab: usize,
    /// 当前 tab 筛选后的原始 options 索引
    pub filtered_indices: Vec<usize>,
}

impl DialogState {
    /// 从 options 提取分类 tab 列表：["All", cat1, cat2, ...]
    pub fn tabs(&self) -> Vec<&str> {
        let mut tabs: Vec<&str> = vec!["All"];
        for opt in &self.options {
            if let Some(cat) = extract_category(opt) {
                if !tabs.iter().any(|t| *t == cat) {
                    tabs.push(cat);
                }
            }
        }
        tabs
    }

    /// 是否为带分类的 settings dialog
    pub fn has_tabs(&self) -> bool {
        self.title.to_lowercase().contains("settings") && self.tabs().len() > 1
    }

    pub fn next_tab(&mut self) {
        let count = self.tabs().len();
        self.active_tab = (self.active_tab + 1) % count;
        self.apply_tab_filter();
    }

    pub fn prev_tab(&mut self) {
        let count = self.tabs().len();
        self.active_tab = (self.active_tab + count - 1) % count;
        self.apply_tab_filter();
    }

    pub fn apply_tab_filter(&mut self) {
        let tabs = self.tabs();
        let active_cat = if self.active_tab == 0 {
            None
        } else {
            tabs.get(self.active_tab).copied()
        };

        self.filtered_indices = self
            .options
            .iter()
            .enumerate()
            .filter_map(|(i, opt)| {
                if let Some(cat) = active_cat {
                    if extract_category(opt) != Some(cat) {
                        return None;
                    }
                }
                Some(i)
            })
            .collect();
        self.selected = self.selected.min(self.filtered_indices.len().saturating_sub(1));
    }
}

/// 从 "[Category] label" 格式中提取 category
fn extract_category(s: &str) -> Option<&str> {
    if s.starts_with('[') {
        if let Some(end) = s.find(']') {
            return Some(&s[1..end]);
        }
    }
    None
}

#[derive(Clone, Debug)]
pub struct Notification {
    pub level: String,
    pub message: String,
    pub created_at: Instant,
}

#[derive(Clone, Debug)]
pub struct RetryState {
    pub reason: String,
    pub started_at: Instant,
    pub total_seconds: u32,
}

impl RetryState {
    pub fn remaining(&self) -> u32 {
        self.total_seconds
            .saturating_sub(self.started_at.elapsed().as_secs() as u32)
    }
    pub fn is_done(&self) -> bool {
        self.remaining() == 0
    }
}

#[derive(Clone, Debug, Default)]
pub struct AppState {
    pub ui: NativeUiState,
    pub dialog: Option<DialogState>,
    pub autocomplete: Option<AutocompleteState>,
    pub permission: Option<PermissionState>,
    pub graph: Option<GraphState>,
    pub session_selector: Option<SessionSelectorState>,
    pub model_selector: Option<ModelSelectorState>,
    pub notifications: Vec<Notification>,
    pub retry: Option<RetryState>,
    pub input_override: Option<String>,
    pub scroll: usize,
    pub auto_scroll: bool,
    pub tools_expanded: bool,
    pub thinking_visible: bool,
    pub show_images: bool,
    pub compacting: bool,
    pub attached_images: Vec<String>,
    pub should_exit: bool,
    pub last_escape: Option<Instant>,
    pub last_ctrl_c: Option<Instant>,
    /// 当前输入前缀是否有有效的 autocomplete 匹配（由 autocomplete 结果驱动）
    /// 用于 / 和 @ 前缀的高亮
    pub input_has_valid_match: bool,
    /// 快捷键管理器：合并后端绑定 + 用户自定义覆盖
    pub keybindings_manager: KeybindingsManager,
    /// Compaction summary 默认折叠
    pub compaction_collapsed: bool,
    /// 动态 working 消息（如 "Running bash..."）
    pub working_message: Option<String>,
    /// 上次后端发送的 hideThinking 值（用于检测 settings 变更）
    pub last_backend_hide_thinking: bool,
    /// 上次后端发送的 showImages 值
    pub last_backend_show_images: bool,
    /// 需要强制全量重绘（从外部编辑器/suspend 返回后）
    pub needs_full_redraw: bool,
    /// Overlay 焦点栈 — 管理 permission/dialog/graph 等浮层的焦点优先级
    pub overlay_stack: crate::overlay::OverlayStack,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            auto_scroll: true,
            thinking_visible: true,
            show_images: true,
            last_backend_show_images: true,
            compaction_collapsed: true,
            ..Default::default()
        }
    }

    pub fn expire_notifications(&mut self) {
        self.notifications
            .retain(|n| n.created_at.elapsed() < Duration::from_secs(5));
    }
}

const NATIVE_BACKEND_ONLY_ENV: &str = "ROZSA_NATIVE_TUI_BACKEND_ONLY";
const NATIVE_SOCKET_ENV: &str = "ROZSA_NATIVE_TUI_SOCKET";
const NATIVE_TS_COMMAND_ENV: &str = "ROZSA_NATIVE_TUI_BACKEND_COMMAND";
const NATIVE_TS_ARGS_ENV: &str = "ROZSA_NATIVE_TUI_BACKEND_ARGS_JSON";

struct NativeTsBackend {
    child: Child,
    socket_path: PathBuf,
}

impl Drop for NativeTsBackend {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_file(&self.socket_path);
    }
}

fn default_ts_backend_command(cwd: &Path) -> Result<(PathBuf, Vec<String>), Box<dyn Error>> {
    let tsx = cwd.join("node_modules/.bin/tsx");
    let cli = cwd.join("packages/coding-agent/src/cli.ts");
    if tsx.exists() && cli.exists() {
        return Ok((tsx, vec![cli.to_string_lossy().into_owned()]));
    }
    Err(
        format!("missing TS backend command; set {NATIVE_TS_COMMAND_ENV} and {NATIVE_TS_ARGS_ENV}")
            .into(),
    )
}

fn resolve_ts_backend_command(cwd: &Path) -> Result<(PathBuf, Vec<String>), Box<dyn Error>> {
    let Some(command) = env::var_os(NATIVE_TS_COMMAND_ENV) else {
        return default_ts_backend_command(cwd);
    };
    let args = match env::var(NATIVE_TS_ARGS_ENV) {
        Ok(raw) => serde_json::from_str::<Vec<String>>(&raw)?,
        Err(_) => Vec::new(),
    };
    Ok((PathBuf::from(command), args))
}

fn start_ts_backend() -> Result<NativeTsBackend, Box<dyn Error>> {
    let cwd = env::current_dir()?;
    let temp_dir = cwd.join("temp");
    fs::create_dir_all(&temp_dir)?;
    let socket_path = temp_dir.join(format!(
        "rozsa-native-tui-{}-{}.sock",
        std::process::id(),
        chrono::Utc::now().timestamp_millis()
    ));
    if socket_path.exists() {
        fs::remove_file(&socket_path)?;
    }

    let (command, args) = resolve_ts_backend_command(&cwd)?;
    let child = Command::new(command)
        .args(args)
        .current_dir(&cwd)
        .env(NATIVE_BACKEND_ONLY_ENV, "1")
        .env(NATIVE_SOCKET_ENV, &socket_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()?;

    Ok(NativeTsBackend { child, socket_path })
}

pub async fn run() -> Result<(), Box<dyn Error>> {
    let backend_process: Option<NativeTsBackend>;
    let socket_path = match env::var(NATIVE_SOCKET_ENV) {
        Ok(path) => {
            backend_process = None;
            path
        }
        Err(_) => {
            let process = start_ts_backend()?;
            let path = process.socket_path.to_string_lossy().into_owned();
            backend_process = Some(process);
            path
        }
    };

    let result = run_with_socket(socket_path).await;
    drop(backend_process);
    result
}

/// Run the TUI with a NativeBackend (pure Rust, no TS subprocess).
pub async fn run_native(session: rozsa_app::agent_session::AgentSession) -> Result<(), Box<dyn Error>> {
    run_native_with(session, crate::backend::native::NativeBackendConfig::default()).await
}

/// Run the TUI with a NativeBackend, supplying construction-time config
/// (model registry, session directory) that unlocks model/session commands.
pub async fn run_native_with(
    session: rozsa_app::agent_session::AgentSession,
    config: crate::backend::native::NativeBackendConfig,
) -> Result<(), Box<dyn Error>> {
    use crate::backend::native::NativeBackend;

    let native = Arc::new(NativeBackend::with_config(session, config));
    let mut event_rx = native.events();
    let _ = native.connect().await;

    let writer: crate::input::Writer = Arc::new(NativeCommandSink {
        backend: native.clone(),
    });

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        crossterm::event::EnableBracketedPaste,
        crossterm::event::EnableMouseCapture
    )?;
    let kitty_keyboard_enabled = {
        use crossterm::event::{KeyboardEnhancementFlags, PushKeyboardEnhancementFlags};
        execute!(
            stdout,
            PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                    | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
            )
        )
        .is_ok()
    };
    let backend_term = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend_term)?;

    let result = run_app(&mut terminal, &mut event_rx, &writer).await;

    if kitty_keyboard_enabled {
        let _ = execute!(terminal.backend_mut(), crossterm::event::PopKeyboardEnhancementFlags);
    }
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        crossterm::event::DisableMouseCapture,
        crossterm::event::DisableBracketedPaste,
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;
    result
}

/// Sink that bridges synchronous CommandSink::send_command into async AgentBackend method calls.
///
/// Holds an Arc<NativeBackend> and spawns a tokio task per ClientMessage so the UI render
/// loop never blocks on backend IO. All ClientMessage variants are forwarded — adding a new
/// variant means adding one branch here, never silently dropping.
struct NativeCommandSink {
    backend: Arc<crate::backend::native::NativeBackend>,
}

impl crate::input::CommandSink for NativeCommandSink {
    fn send_command(&self, msg: &ClientMessage<'_>) -> Result<(), Box<dyn Error>> {
        use crate::backend::{AgentBackend, Direction};

        let backend = self.backend.clone();
        // Each branch clones the strings it needs, then spawns one task.
        // The cost of an extra spawn per command is negligible vs. the alternative of
        // routing through an enum + queue — and it removes the silent-drop trap.
        match msg {
            ClientMessage::Submit { text, .. } => {
                let text = text.to_string();
                tokio::spawn(async move { let _ = backend.submit(&text, vec![]).await; });
            }
            ClientMessage::Abort => {
                tokio::spawn(async move { let _ = backend.abort().await; });
            }
            ClientMessage::Exit => {
                tokio::spawn(async move { let _ = backend.exit().await; });
            }
            ClientMessage::FollowUp { text, .. } => {
                let text = text.to_string();
                tokio::spawn(async move { let _ = backend.follow_up(&text, vec![]).await; });
            }
            ClientMessage::Steer { text, .. } => {
                let text = text.to_string();
                tokio::spawn(async move { let _ = backend.steer(&text, vec![]).await; });
            }
            ClientMessage::Compact => {
                tokio::spawn(async move { let _ = backend.compact().await; });
            }
            ClientMessage::AutocompleteRequest { text, cursor, force, .. } => {
                let text = text.to_string();
                let cursor = *cursor;
                let force = *force;
                tokio::spawn(async move {
                    let _ = backend.autocomplete_request(&text, cursor, force).await;
                });
            }
            ClientMessage::CycleModel { direction } => {
                let dir = if *direction == "backward" { Direction::Backward } else { Direction::Forward };
                tokio::spawn(async move { let _ = backend.cycle_model(dir).await; });
            }
            ClientMessage::CycleThinking => {
                // No dedicated backend call yet; left as no-op until thinking-level cycling lands.
            }
            ClientMessage::CycleEditMode => {
                tokio::spawn(async move { let _ = backend.cycle_edit_mode().await; });
            }
            ClientMessage::DialogResponse { id, value, confirmed, cancelled } => {
                let id = id.to_string();
                let value = value.map(|s| s.to_string());
                let confirmed = *confirmed;
                let cancelled = *cancelled;
                tokio::spawn(async move {
                    let _ = backend
                        .dialog_response(&id, value.as_deref(), confirmed, cancelled)
                        .await;
                });
            }
            ClientMessage::PermissionResponse { id, choice, trust_key } => {
                let id = id.to_string();
                let choice = choice.to_string();
                let trust_key = trust_key.map(|s| s.to_string());
                tokio::spawn(async move {
                    let _ = backend
                        .respond_permission(&id, &choice, trust_key.as_deref())
                        .await;
                });
            }
            ClientMessage::Bash { command } => {
                let command = command.to_string();
                tokio::spawn(async move { let _ = backend.run_bash(&command).await; });
            }
            ClientMessage::SwitchAgent { id } => {
                let id = id.to_string();
                tokio::spawn(async move { let _ = backend.switch_agent(&id).await; });
            }
            ClientMessage::SwitchModel { provider, id } => {
                let provider = provider.to_string();
                let id = id.to_string();
                tokio::spawn(async move { let _ = backend.switch_model(&provider, &id).await; });
            }
            ClientMessage::SwitchSession { path } => {
                let path = path.to_string();
                tokio::spawn(async move { let _ = backend.switch_session(&path).await; });
            }
            ClientMessage::DeleteSession { path } => {
                let path = path.to_string();
                tokio::spawn(async move { let _ = backend.delete_session(&path).await; });
            }
            ClientMessage::RenameSession { path, name } => {
                let path = path.to_string();
                let name = name.to_string();
                tokio::spawn(async move { let _ = backend.rename_session(&path, &name).await; });
            }
            ClientMessage::ListSessions { .. } => {
                tokio::spawn(async move { let _ = backend.list_sessions().await; });
            }
            ClientMessage::ListModels => {
                tokio::spawn(async move { let _ = backend.list_models().await; });
            }
            ClientMessage::UpdateSetting { key, value } => {
                let key = key.to_string();
                let value = value.to_string();
                tokio::spawn(async move { let _ = backend.update_setting(&key, &value).await; });
            }
        }
        Ok(())
    }
}

async fn run_with_socket(socket_path: String) -> Result<(), Box<dyn Error>> {
    // T027 (StdinBuffer): Crossterm 的 parse_event 内置了分段 escape 序列处理
    //   — buffer.len()==1 且 input_available 时返回 None 等待后续字节
    //   — 超时后作为独立 Esc 处理（mio poll_timeout 机制）
    // T028 (Windows VT): Crossterm enable_raw_mode() 在 Windows 上配置控制台输入
    //   — 不需要额外调用 ENABLE_VIRTUAL_TERMINAL_INPUT（Crossterm 自有事件解析）
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        crossterm::event::EnableBracketedPaste,
        crossterm::event::EnableMouseCapture
    )?;

    // Kitty 键盘协议增强（如终端支持）
    let kitty_keyboard_enabled = {
        use crossterm::event::{KeyboardEnhancementFlags, PushKeyboardEnhancementFlags};
        execute!(
            stdout,
            PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                    | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
            )
        )
        .is_ok()
    };

    let backend_term = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend_term)?;

    // 创建 SocketBackend 并连接
    let backend = SocketBackend::new(socket_path);
    let mut event_rx = backend.events();

    let connect_result = backend.connect().await;
    let writer: crate::input::Writer = match connect_result {
        Ok(()) => {
            let raw_writer = backend.writer().expect("writer available after connect");
            Arc::new(crate::protocol::SocketCommandSink::new(raw_writer))
        }
        Err(e) => {
            disable_raw_mode()?;
            execute!(
                terminal.backend_mut(),
                crossterm::event::DisableBracketedPaste,
                LeaveAlternateScreen
            )?;
            terminal.show_cursor()?;
            return Err(Box::new(e));
        }
    };

    let result = run_app(&mut terminal, &mut event_rx, &writer).await;

    // 恢复 Kitty 键盘协议
    if kitty_keyboard_enabled {
        let _ = execute!(
            terminal.backend_mut(),
            crossterm::event::PopKeyboardEnhancementFlags
        );
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        crossterm::event::DisableMouseCapture,
        crossterm::event::DisableBracketedPaste,
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;

    result
}

/// 将 BackendEvent 映射到 AppState 变更
fn apply_backend_event(state: &mut AppState, event: BackendEvent) {
    match event {
        BackendEvent::State(ui) => {
            if !state.auto_scroll && state.ui.is_streaming {
                // 用户在 streaming 中手动滚动了 — 增加 scroll 偏移以补偿新增内容
                // 防止视口在内容增长时向下漂移
                let old_msg_count = state.ui.messages.len();
                let new_msg_count = ui.messages.len();
                if new_msg_count > old_msg_count {
                    state.scroll = state.scroll.saturating_add(new_msg_count - old_msg_count);
                }
            }
            // OSC 9;4 进度指示器：streaming 状态变化时更新
            let was_streaming = state.ui.is_streaming;
            let was_compacting = state.compacting;
            state.compacting = ui.is_compacting;
            // compaction 完成 — 重置滚动到底部
            let new_has_compaction = ui
                .messages
                .first()
                .and_then(|m| m.get("role").and_then(|v| v.as_str()))
                == Some("compactionSummary");
            let old_has_compaction = state
                .ui
                .messages
                .first()
                .and_then(|m| m.get("role").and_then(|v| v.as_str()))
                == Some("compactionSummary");
            if (was_compacting && !ui.is_compacting)
                || ui.messages.len() < state.ui.messages.len().saturating_sub(2)
                || (new_has_compaction && !old_has_compaction)
            {
                state.scroll = 0;
                state.auto_scroll = true;
            }
            // 同步后端 settings 到本地渲染状态（仅在 settings 变更时覆盖，保留 Ctrl+T 本地 toggle）
            if ui.hide_thinking != state.last_backend_hide_thinking {
                state.thinking_visible = !ui.hide_thinking;
                state.last_backend_hide_thinking = ui.hide_thinking;
            }
            if ui.show_images != state.last_backend_show_images {
                state.show_images = ui.show_images;
                state.last_backend_show_images = ui.show_images;
            }
            // 同步 KeybindingsManager
            state
                .keybindings_manager
                .update_from_backend(&ui.keybindings);
            state.ui = ui;
            if state.ui.is_streaming && !was_streaming {
                // 开始 streaming → 显示进度
                let _ = std::io::Write::write_all(&mut std::io::stdout(), b"\x1b]9;4;3\x07");
            } else if !state.ui.is_streaming && was_streaming {
                // 结束 streaming → 清除进度
                let _ = std::io::Write::write_all(&mut std::io::stdout(), b"\x1b]9;4;0;\x07");
            }
        }
        BackendEvent::Dialog {
            id,
            kind,
            title,
            message,
            mut options,
            text,
            selected,
        } => {
            // dialog 出现时清除 autocomplete — 防止 autocomplete handler 拦截 dialog 按键
            state.autocomplete = None;
            // 在 settings 对话框中注入本地 theme 选项
            if title.to_lowercase().contains("settings") || title.to_lowercase().contains("设置")
            {
                let theme_label = if crate::theme::is_dark_theme() {
                    "Theme: dark → light"
                } else {
                    "Theme: light → dark"
                };
                options.push(theme_label.to_string());
            }
            let max_sel = options.len().saturating_sub(1);
            let filtered_indices = (0..options.len()).collect::<Vec<_>>();
            let mut dialog = DialogState {
                id,
                kind,
                title,
                message,
                options,
                selected: selected.unwrap_or(0).min(max_sel),
                input: text.unwrap_or_default(),
                active_tab: 0,
                filtered_indices,
            };
            dialog.apply_tab_filter();
            state.dialog = Some(dialog);
        }
        BackendEvent::Notify { level, message } => {
            state.notifications.push(Notification {
                level,
                message,
                created_at: Instant::now(),
            });
        }
        BackendEvent::SetTitle(title) => {
            let _ = execute!(std::io::stdout(), crossterm::terminal::SetTitle(title));
        }
        BackendEvent::SetInput(text) => {
            state.input_override = Some(text);
        }
        BackendEvent::Autocomplete { prefix, items, .. } => {
            if items.is_empty() {
                state.autocomplete = None;
                state.input_has_valid_match = false;
            } else {
                state.input_has_valid_match = true;
                state.autocomplete = Some(AutocompleteState::new(prefix, items));
            }
        }
        BackendEvent::Permission(prompt) => {
            state.permission = Some(PermissionState::new(prompt));
        }
        BackendEvent::Graph(nodes) => {
            state.graph = Some(GraphState::new(nodes));
        }
        BackendEvent::Sessions {
            entries,
            current_session_path,
        } => {
            let current = if current_session_path.is_empty() {
                None
            } else {
                Some(current_session_path)
            };
            if let Some(ref mut sel) = state.session_selector {
                sel.set_entries(entries, current);
            } else {
                state.session_selector = Some(SessionSelectorState::new(entries, current));
            }
        }
        BackendEvent::SessionDeleted {
            path,
            method,
            error,
        } => {
            if let Some(ref mut sel) = state.session_selector {
                sel.handle_session_deleted(&path, &method, error.as_deref());
            }
        }
        BackendEvent::Models(entries) => {
            state.model_selector = Some(ModelSelectorState::new(entries));
        }
        BackendEvent::Retry { seconds, reason } => {
            state.retry = Some(RetryState {
                reason,
                started_at: Instant::now(),
                total_seconds: seconds,
            });
        }
        BackendEvent::Compacting(active) => {
            if state.compacting && !active {
                // compaction 刚完成 — 重置滚动到底部
                state.scroll = 0;
                state.auto_scroll = true;
            }
            state.compacting = active;
        }
        BackendEvent::Shutdown => {
            state.should_exit = true;
        }
        BackendEvent::Disconnected => {
            state.should_exit = true;
        }
    }
}

/// async 事件循环：tokio::select! 多路复用 terminal event + backend event + tick
async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    event_rx: &mut mpsc::UnboundedReceiver<BackendEvent>,
    writer: &crate::input::Writer,
) -> Result<(), Box<dyn Error>> {
    let mut state = AppState::new();
    let mut editor = InputState::default();
    let mut term_events = EventStream::new();
    // 事件驱动渲染：仅在收到事件或定时器到期时重绘
    // streaming 时使用 100ms tick 驱动 spinner 动画，idle 时用 1 秒 tick
    let mut tick_interval = tokio::time::interval(Duration::from_millis(100));
    tick_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut needs_redraw = true;
    // 帧率限制：最快 8ms (~120 FPS)
    let mut last_draw = Instant::now();
    let min_frame_interval = Duration::from_millis(8);

    loop {
        if let Some(text) = state.input_override.take() {
            editor.set_text(text);
            needs_redraw = true;
        }
        state.expire_notifications();
        if state.retry.as_ref().is_some_and(|r| r.is_done()) {
            state.retry = None;
            needs_redraw = true;
        }

        if state.needs_full_redraw {
            terminal.clear()?;
            state.needs_full_redraw = false;
            needs_redraw = true;
        }

        if needs_redraw && last_draw.elapsed() >= min_frame_interval {
            terminal.draw(|frame| render(frame, &state, &editor))?;
            last_draw = Instant::now();
            needs_redraw = false;
        }

        if state.should_exit {
            break;
        }

        // 权限超时自动 reject
        if let Some(perm) = &state.permission {
            if perm.is_expired() {
                let id = perm.prompt.id.clone();
                let _ = crate::protocol::send(
                    writer,
                    &ClientMessage::PermissionResponse {
                        id: &id,
                        choice: "reject",
                        trust_key: None,
                    },
                );
                state.permission = None;
                needs_redraw = true;
            }
        }

        tokio::select! {
            maybe_event = term_events.next() => {
                match maybe_event {
                    Some(Ok(Event::Key(key))) => {
                        if should_process_key_event(key) {
                            handle_key(key, &mut state, writer, &mut editor)?;
                            needs_redraw = true;
                        }
                    }
                    Some(Ok(Event::Mouse(mouse))) => {
                        crate::input::mouse::handle_mouse(mouse, &mut state);
                        needs_redraw = true;
                    }
                    Some(Ok(Event::Paste(data))) => {
                        crate::input::mouse::handle_paste(&data, &mut state, &mut editor);
                        needs_redraw = true;
                    }
                    Some(Ok(Event::Resize(_, _))) => {
                        needs_redraw = true;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                    None => break,
                }
            }
            maybe_msg = event_rx.recv() => {
                match maybe_msg {
                    Some(event) => {
                        apply_backend_event(&mut state, event);
                        needs_redraw = true;
                    }
                    None => {
                        state.should_exit = true;
                    }
                }
            }
            _ = tick_interval.tick() => {
                // streaming/compacting/retry 时需要定期重绘以驱动 spinner 动画
                if state.ui.is_streaming || state.compacting || state.retry.is_some() || state.permission.is_some() {
                    needs_redraw = true;
                }
            }
        }
    }

    Ok(())
}

fn should_process_key_event(key: KeyEvent) -> bool {
    // 仅处理 Press 和 Repeat 事件；Release 事件忽略。
    // IME 组合输入由终端自身处理——crossterm 将最终确认的字符作为
    // Press 事件传递（可能包含多字节 CJK 字符），我们通过 grapheme-aware
    // 编辑逻辑正确处理。光标位置通过 set_cursor_position 发送给终端，
    // 供 IME 候选窗口定位使用。
    matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{
        KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind,
    };

    #[test]
    fn ignores_key_release_events() {
        assert!(should_process_key_event(KeyEvent::new_with_kind(
            KeyCode::Char('w'),
            KeyModifiers::NONE,
            KeyEventKind::Press,
        )));
        assert!(should_process_key_event(KeyEvent::new_with_kind(
            KeyCode::Char('w'),
            KeyModifiers::NONE,
            KeyEventKind::Repeat,
        )));
        assert!(!should_process_key_event(KeyEvent::new_with_kind(
            KeyCode::Char('w'),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        )));
    }

    #[test]
    fn mouse_wheel_scrolls_one_line_per_tick() {
        let mut state = AppState::new();
        crate::input::mouse::handle_mouse(
            MouseEvent {
                kind: MouseEventKind::ScrollUp,
                column: 0,
                row: 0,
                modifiers: KeyModifiers::NONE,
            },
            &mut state,
        );
        assert_eq!(state.scroll, 1);
        assert!(!state.auto_scroll);

        crate::input::mouse::handle_mouse(
            MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: 0,
                row: 0,
                modifiers: KeyModifiers::NONE,
            },
            &mut state,
        );
        assert_eq!(state.scroll, 0);
        assert!(state.auto_scroll);
    }
}
