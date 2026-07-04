# rozsa-tui — Terminal User Interface

## 概述

`rozsa-tui` 是 Rózsa AI 编码助手的 ratatui 驱动 TUI 前端，提供交互式终端界面。支持两种后端模式：

- **NativeBackend**: 同进程 Rust 后端，直接持有 `Arc<AgentSession>`，通过 `AgentEvent` 广播流驱动 UI 状态。
- **SocketBackend**: Unix socket 连接到 TypeScript 遗留后端（过渡期）。

## 目录结构

参考 [docs/tui/architecture.md](../tui/architecture.md)，目录按职责划分：

```
crates/rozsa-tui/src/
├── app.rs              应用事件循环 + AppState
├── main.rs             入口（启动 SocketBackend）
├── lib.rs              库入口（暴露 run_native API）
├── protocol.rs         线格式 DTO（NativeUiState、ClientMessage 等）
│
├── backend/            后端通信抽象 + 实现
│   ├── mod.rs          AgentBackend trait + BackendEvent
│   ├── native.rs       NativeBackend（同进程）
│   ├── socket.rs       SocketBackend（过渡期）
│   ├── subagent_view.rs  SubagentView trait（sidebar 同步查询子代理）
│   └── mock.rs         MockBackend（测试用）
│
├── input/              输入处理：按键/鼠标 → 动作
│   ├── mod.rs          InputState + CommandSink trait + Writer type alias
│   ├── keys.rs         键盘事件 + grapheme 工具 + 文本编辑操作
│   ├── mouse.rs        鼠标事件 + 粘贴处理
│   ├── keymap.rs       快捷键管理器（合并后端 + 用户自定义）
│   ├── kill_ring.rs    剪切环（Emacs-style）
│   ├── undo.rs         撤销栈
│   └── editor.rs       编辑器模式（vim/normal）
│
├── render/             渲染调度 + 主区域渲染
│   ├── mod.rs          缓存 + render() 顶层入口
│   ├── layout.rs       布局高度计算
│   ├── messages.rs     消息区渲染（消费 AgentMessage）
│   ├── input_box.rs    输入框渲染
│   ├── status.rs       状态行 + 通知
│   ├── dialog.rs       对话框渲染
│   └── overlay.rs      焦点栈管理（OverlayStack）
│
├── panels/             独立交互面板（有 State + handle_key + render）
│   ├── graph.rs        会话历史图
│   ├── model_selector.rs  模型选择器
│   ├── session_selector.rs 会话选择器
│   ├── permission.rs   权限审批
│   ├── autocomplete.rs 自动补全
│   └── sidebar.rs      侧边栏
│
├── widgets/            可复用 UI 原子（无自有 state，接参数渲染）
│   ├── tab_bar.rs      可滚动 tab 栏
│   └── hints_bar.rs    底部快捷键提示
│
├── util/               纯函数工具（不依赖 TUI 框架）
│   ├── ansi.rs         ANSI 转 ratatui Style
│   ├── markdown.rs     Markdown → Lines
│   ├── highlight.rs    语法高亮
│   ├── hyperlink.rs    OSC 8 超链接
│   ├── fuzzy.rs        模糊匹配算法
│   └── terminal_caps.rs  终端能力检测 + 图片协议
│
├── data/               数据 provider（给 panels 提供数据）
│   ├── autocomplete_provider.rs
│   ├── session_search.rs
│   └── session_tree.rs
│
├── theme/              颜色/样式
│   ├── mod.rs
│   └── palette.rs
│
└── command/            命令注册 + 帮助文本
    ├── mod.rs
    └── builtin.rs
```

## 核心接口

### AgentBackend trait

后端通信抽象，定义所有前端→后端命令：

```rust
// crates/rozsa-tui/src/backend/mod.rs

#[async_trait]
pub trait AgentBackend: Send + Sync {
    // --- 对话 ---
    async fn submit(&self, text: &str, images: Vec<ImageData>) -> BackendResult<()>;
    async fn abort(&self) -> BackendResult<()>;
    async fn follow_up(&self, text: &str, images: Vec<ImageData>) -> BackendResult<()>;
    async fn steer(&self, text: &str, images: Vec<ImageData>) -> BackendResult<()>;

    // --- 模型管理 ---
    async fn list_models(&self) -> BackendResult<()>;
    async fn switch_model(&self, provider: &str, id: &str) -> BackendResult<()>;
    async fn cycle_model(&self, direction: Direction) -> BackendResult<()>;

    // --- 会话管理 ---
    async fn list_sessions(&self) -> BackendResult<()>;
    async fn switch_session(&self, path: &str) -> BackendResult<()>;
    async fn delete_session(&self, path: &str) -> BackendResult<()>;
    async fn rename_session(&self, path: &str, name: &str) -> BackendResult<()>;
    async fn fork_session(&self, message_index: usize) -> BackendResult<()>;

    // --- 权限审批 ---
    async fn respond_permission(
        &self,
        id: &str,
        choice: &str,
        trust_key: Option<&str>,
    ) -> BackendResult<()>;

    // --- 工具/Shell ---
    async fn run_bash(&self, command: &str) -> BackendResult<()>;

    // --- 上下文/编辑 ---
    async fn compact(&self) -> BackendResult<()>;
    async fn cycle_edit_mode(&self) -> BackendResult<()>;
    async fn switch_agent(&self, id: &str) -> BackendResult<()>;

    // --- 对话框 ---
    async fn dialog_response(
        &self,
        id: &str,
        value: Option<&str>,
        confirmed: Option<bool>,
        cancelled: Option<bool>,
    ) -> BackendResult<()>;

    // --- 自动补全 ---
    async fn autocomplete_request(
        &self,
        text: &str,
        cursor: usize,
        force: bool,
    ) -> BackendResult<()>;

    // --- 设置 ---
    async fn update_setting(&self, key: &str, value: &str) -> BackendResult<()>;

    // --- 生命周期 ---
    async fn connect(&self) -> BackendResult<()>;
    async fn disconnect(&self) -> BackendResult<()>;
    async fn exit(&self) -> BackendResult<()>;

    // --- 事件流（后端主动推送） ---
    fn events(&self) -> mpsc::UnboundedReceiver<BackendEvent>;
}

pub type BackendResult<T> = Result<T, BackendError>;

#[derive(Debug, Clone)]
pub enum BackendError {
    NotConnected,
    ConnectionLost,
    Protocol(String),
    Internal(String),
}
```

### BackendEvent

后端推送给前端的事件流：

```rust
#[derive(Debug, Clone)]
pub enum BackendEvent {
    /// 全量 UI 状态更新（包含消息列表、模型信息、token 统计等）
    State(NativeUiState),

    /// 对话框弹出
    Dialog {
        id: String,
        kind: String,
        title: String,
        message: Option<String>,
        options: Vec<String>,
        text: Option<String>,
        selected: Option<usize>,
    },

    /// 通知消息（info / warn / error）
    Notify { level: String, message: String },

    /// 终端标题（OSC 2）
    SetTitle(String),

    /// 覆盖输入框内容（用于外部编辑器返回、slash command 自动填充等）
    SetInput(String),

    /// 自动补全结果
    Autocomplete {
        id: u64,
        prefix: String,
        items: Vec<NativeAutocompleteItem>,
    },

    /// 权限审批请求
    Permission(NativePermissionPrompt),

    /// 会话历史图（/graph 命令触发）
    Graph(Vec<NativeGraphNode>),

    /// Fork 图（/fork 命令触发）
    ForkGraph(Vec<NativeGraphNode>),

    /// 会话列表（/resume 或 Ctrl+R 触发）
    Sessions {
        entries: Vec<SessionEntry>,
        current_session_path: String,
    },

    /// 会话删除结果
    SessionDeleted {
        path: String,
        method: String,
        error: Option<String>,
    },

    /// 模型列表（/model 命令触发）
    Models(Vec<ModelEntry>),

    /// 重试倒计时（rate limit / 临时故障时显示）
    Retry { seconds: u32, reason: String },

    /// Compacting 状态（true=开始压缩，false=完成）
    Compacting(bool),

    /// 后端请求关闭（/quit 或 Ctrl+D）
    Shutdown,

    /// 连接已断开
    Disconnected,
}
```

### NativeBackend

同进程后端实现，持有 `Arc<AgentSession>`：

```rust
// crates/rozsa-tui/src/backend/native.rs

pub struct NativeBackend {
    session: Arc<AgentSession>,
    event_tx: mpsc::UnboundedSender<BackendEvent>,
    event_rx: Mutex<Option<mpsc::UnboundedReceiver<BackendEvent>>>,
    live: Arc<Mutex<LiveState>>,
    model_registry: Option<Arc<ModelRegistry>>,
    session_dir: Option<PathBuf>,
    global_settings_path: Option<PathBuf>,
    runtime_settings: Mutex<Settings>,
    autocomplete_id: AtomicU64,
    pending_approvals: Option<PendingApprovals>,
}

/// Live snapshot of session state used to build NativeUiState payloads.
/// Maintained by the background forwarder task as AgentEvents arrive.
pub struct LiveState {
    pub messages: Vec<AgentMessage>,
    pub is_streaming: bool,
    pub turn_base: usize,  // AgentEnd 时用于 truncate+extend
    pub hide_thinking: bool,
}

impl NativeBackend {
    pub fn new(session: AgentSession) -> Self;
    pub fn with_config(session: AgentSession, config: NativeBackendConfig) -> Self;
}

pub struct NativeBackendConfig {
    pub model_registry: Option<Arc<ModelRegistry>>,
    pub session_dir: Option<PathBuf>,
    pub global_settings_path: Option<PathBuf>,
    pub pending_approvals: Option<PendingApprovals>,
    pub permission_request_rx: Option<mpsc::UnboundedReceiver<(String, ApprovalInfo)>>,
}
```

**关键设计点：**

- **事件转发任务**：`spawn_event_forwarder()` 后台订阅 `AgentSession` 的 `AgentEvent` 广播，应用 `apply_event()` 更新 `LiveState`，推送 `BackendEvent::State`。
- **消息累积**：`LiveState.messages` 随 `MessageStart` / `MessageUpdate` / `MessageEnd` 增量更新。`AgentEnd` 时 **truncate + extend**（从 `turn_base` 截断，替换为权威消息列表），避免与 streaming 消息重复。
- **Slash command 本地分发**：`dispatch_slash_command()` 拦截 `/model` / `/thinking` / `/help` / `/graph` / `/session` / `/lsp` 等，直接操作 session 或推送事件，不经过 agent prompt。
- **Bang command 执行**：`execute_bang_command()` 拦截 `!command`（或 `!!command` 不存历史），启动 `tokio::process::Command`，流式推送输出到对话区。

### SubagentView trait

sidebar 同步查询子代理状态的窄接口（非阻塞）：

```rust
// crates/rozsa-tui/src/backend/subagent_view.rs

pub trait SubagentView: Send + Sync {
    /// Best-effort 列出当前子代理。锁被占用时返回空列表。
    fn list_subagents_sync(&self) -> Vec<SubagentInfo>;

    /// 当前正在查看的子代理 id（None = 主 session）。
    fn viewing_subagent_id_sync(&self) -> Option<String>;
}
```

NativeBackend 实现此 trait，用 `try_lock` 非阻塞读取 subagent 状态，供 sidebar 渲染。

### AppState

应用主状态结构（单线程独占）：

```rust
// crates/rozsa-tui/src/app.rs

pub struct AppState {
    pub ui: NativeUiState,               // 后端推送的 UI 状态
    pub dialog: Option<DialogState>,     // 当前打开的对话框
    pub autocomplete: Option<AutocompleteState>,
    pub permission: Option<PermissionState>,
    pub graph: Option<GraphState>,
    pub session_selector: Option<SessionSelectorState>,
    pub model_selector: Option<ModelSelectorState>,
    pub notifications: Vec<Notification>,  // 自动过期（5 秒）
    pub retry: Option<RetryState>,         // 重试倒计时
    pub input_override: Option<String>,    // 外部覆盖输入框（如 /fork 自动填充）
    pub scroll: usize,                     // 消息区滚动偏移（从底部向上计数）
    pub auto_scroll: bool,                 // 新消息到达时是否自动滚动
    pub tools_expanded: bool,              // Tool call 详情展开/折叠
    pub thinking_visible: bool,            // 显示/隐藏 thinking blocks
    pub show_images: bool,                 // 显示/隐藏图片
    pub compacting: bool,                  // Compaction 进行中
    pub attached_images: Vec<String>,      // 附加图片（base64）
    pub should_exit: bool,                 // 退出标志
    pub last_escape: Option<Instant>,      // 双击 Esc 退出检测
    pub last_ctrl_c: Option<Instant>,      // 双击 Ctrl+C 退出检测
    pub input_has_valid_match: bool,       // 输入前缀有 autocomplete 匹配（高亮 / @）
    pub keybindings_manager: KeybindingsManager,  // 快捷键管理器
    pub compaction_collapsed: bool,        // Compaction summary 默认折叠
    pub working_message: Option<String>,   // 动态 working 消息
    pub last_backend_hide_thinking: bool,  // 后端 hideThinking 值（检测 settings 变更）
    pub last_backend_show_images: bool,    // 后端 showImages 值
    pub needs_full_redraw: bool,           // 需要强制全量重绘（从外部编辑器返回后）
    pub overlay_stack: OverlayStack,       // Overlay 焦点栈
    pub last_autocomplete_id: u64,         // 最近接受的 autocomplete 响应 id（丢弃乱序旧响应）
}
```

### NativeUiState

后端推送的 UI 状态 DTO（线格式协议）：

```rust
// crates/rozsa-tui/src/protocol.rs

pub struct NativeUiState {
    pub app_name: String,
    pub version: String,
    pub cwd: String,
    pub session_name: Option<String>,
    pub model: Option<ModelInfo>,
    pub thinking_level: String,
    pub is_streaming: bool,
    pub is_compacting: bool,
    pub hide_thinking: bool,
    pub show_images: bool,
    pub messages: Vec<AgentMessage>,  // 消息列表（AgentMessage 枚举）
    pub pending_messages: Vec<String>,  // 后续提示
    pub status: BTreeMap<String, String>,
    pub widgets_above: BTreeMap<String, Vec<String>>,
    pub widgets_below: BTreeMap<String, Vec<String>>,
    pub stats: Option<Value>,
    pub runtime_state: Option<Value>,  // modelUsage 等
    pub context_usage: Option<Value>,  // percent / tokens / contextWindow
    pub keybindings: BTreeMap<String, Vec<String>>,
    pub error: Option<String>,
}
```

**消息反序列化**：`deserialize_messages_flat()` 将线格式 flat camelCase JSON 对象转换为 `AgentMessage` 枚举：

- `role: "user"/"assistant"/"toolResult"` → `AgentMessage::Standard { message: Message }`
- 其他 `role` 值 → `AgentMessage::Custom { message: CustomAgentMessage }`

## Panels

每个 panel 是独立交互面板，有自己的 State + `handle_key()` + `render()` 方法。

### GraphState

会话历史图（`/graph` 或 `/fork` 触发）：

```rust
// crates/rozsa-tui/src/panels/graph.rs

pub struct GraphState {
    pub nodes: Vec<NativeGraphNode>,
    pub selected: usize,
    pub mode: GraphMode,  // Normal / Fork
}

pub enum GraphMode {
    Normal,  // /graph
    Fork,    // /fork — 仅显示 user 消息，选中后创建分叉会话
}

pub fn render_graph(frame: &mut Frame, area: Rect, state: &GraphState);
```

**交互**：

- `↑` / `↓` 导航节点
- `Enter` 确认选择（Fork 模式下创建分叉会话并切换）
- `Esc` 关闭

### ModelSelectorState

模型选择器（`/model` 或 `Ctrl+L` 触发）：

```rust
// crates/rozsa-tui/src/panels/model_selector.rs

pub struct ModelSelectorState {
    pub entries: Vec<ModelEntry>,
    pub selected: usize,
    pub filter: String,
}

pub struct ModelEntry {
    pub id: String,
    pub provider: String,
    pub is_current: bool,
}

pub fn render_model_selector(frame: &mut Frame, area: Rect, state: &ModelSelectorState);
```

**交互**：

- `↑` / `↓` 导航
- `Enter` 选择模型
- `/` 开始过滤
- `Esc` 关闭

### SessionSelectorState

会话选择器（`/resume` 或 `Ctrl+R` 触发）：

```rust
// crates/rozsa-tui/src/panels/session_selector.rs

pub struct SessionSelectorState {
    pub entries: Vec<SessionEntry>,
    pub selected: usize,
    pub filter: String,
    pub current_path: Option<String>,
    pub mode: SessionMode,  // Normal / Delete / Rename
}

pub struct SessionEntry {
    pub path: String,
    pub name: Option<String>,
    pub first_message: String,
    pub cwd: String,
    pub message_count: usize,
    pub last_modified: i64,
    pub parent_session_path: Option<String>,
    pub all_messages_text: String,
}

pub fn render_session_selector(frame: &mut Frame, area: Rect, state: &SessionSelectorState);
```

**交互**：

- `↑` / `↓` 导航
- `Enter` 切换会话
- `d` 进入删除模式，再次确认删除
- `r` 进入重命名模式，输入新名称
- `/` 开始过滤
- `Esc` 关闭

### PermissionState

权限审批面板（工具调用触发）：

```rust
// crates/rozsa-tui/src/panels/permission.rs

pub struct PermissionState {
    pub prompt: NativePermissionPrompt,
    pub selected: usize,
    pub created_at: Instant,
}

impl PermissionState {
    /// 权限请求 60 秒后自动 reject
    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() > Duration::from_secs(60)
    }
}

pub fn render_permission(frame: &mut Frame, area: Rect, state: &PermissionState);
```

**交互**：

- `↑` / `↓` 导航选项（Allow once / Allow for session / Deny）
- `Enter` 确认选择
- `a` 快捷键：Allow once
- `d` 快捷键：Deny
- 60 秒超时自动 Deny

### AutocompleteState

自动补全面板（输入 `/` 或 `@` 触发）：

```rust
// crates/rozsa-tui/src/panels/autocomplete.rs

pub struct AutocompleteState {
    pub prefix: String,
    pub items: Vec<NativeAutocompleteItem>,
    pub selected: usize,
}

pub struct NativeAutocompleteItem {
    pub value: String,
    pub label: String,
    pub description: Option<String>,
}

pub fn render_autocomplete(frame: &mut Frame, area: Rect, state: &AutocompleteState);
```

**交互**：

- `↑` / `↓` 导航
- `Tab` 或 `Enter` 确认补全
- `Esc` 关闭
- 继续输入自动过滤

### Sidebar

侧边栏（宽度 ≥ 108 时显示）：

```rust
// crates/rozsa-tui/src/panels/sidebar.rs

pub fn render_sidebar(
    frame: &mut Frame,
    area: Rect,
    state: &AppState,
    agents: Option<&dyn SubagentView>,
);
```

**内容**：

- Git 状态（分支、未提交文件）
- 当前模型信息
- Token 统计（input / output / total）
- Context usage 进度条
- Subagent 列表（通过 `SubagentView` trait 同步查询）

## Widgets

可复用 UI 原子，无自有状态，接参数渲染。

### tab_bar

可滚动 tab 栏（settings 对话框分类、model selector 分组等）：

```rust
// crates/rozsa-tui/src/widgets/tab_bar.rs

pub struct TabBarState {
    pub tabs: Vec<String>,
    pub active: usize,
}

pub fn render_tab_bar(frame: &mut Frame, area: Rect, state: &TabBarState);
```

### hints_bar

底部快捷键提示（panel 特定的上下文提示）：

```rust
// crates/rozsa-tui/src/widgets/hints_bar.rs

pub struct HintItem {
    pub key: String,
    pub label: String,
}

pub fn render_hints_bar(frame: &mut Frame, area: Rect, items: &[HintItem]);
```

## Input 系统

### InputState

输入框状态（多行编辑器）：

```rust
// crates/rozsa-tui/src/input/mod.rs

pub struct InputState {
    pub lines: Vec<String>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub history: Vec<String>,
    pub history_index: Option<usize>,
    pub undo_stack: UndoStack<EditorSnapshot>,
    pub kill_ring: KillRing,
    pub last_action: Option<LastAction>,
    pub yank_len: usize,
    pub jump_mode: Option<JumpDirection>,
    pub folded_ranges: Vec<(usize, usize)>,
    pub selection_anchor: Option<SelectionAnchor>,
    pub pastes: Vec<String>,
    pub paste_counter: usize,
    pub atomic_spans: Vec<AtomicSpan>,
}

impl InputState {
    pub fn text(&self) -> String;
    pub fn expanded_text(&self) -> String;  // 展开 paste marker
    pub fn set_text(&mut self, text: String);
    pub fn is_empty(&self) -> bool;
    pub fn clear(&mut self);
    pub fn push_undo(&mut self);
}
```

### keys.rs

键盘事件处理 + grapheme 工具：

```rust
pub fn handle_key(
    key: KeyEvent,
    state: &mut AppState,
    writer: &Writer,
    editor: &mut InputState,
) -> Result<(), Box<dyn Error>>;

// Grapheme 工具函数
pub fn grapheme_count(s: &str) -> usize;
pub fn grapheme_at(s: &str, idx: usize) -> Option<&str>;
pub fn grapheme_byte_offset(s: &str, grapheme_idx: usize) -> usize;
pub fn move_cursor_left(editor: &mut InputState, shift: bool);
pub fn move_cursor_right(editor: &mut InputState, shift: bool);
pub fn move_cursor_up(editor: &mut InputState, shift: bool);
pub fn move_cursor_down(editor: &mut InputState, shift: bool);
```

**核心功能**：

- Overlay 焦点分发（permission / dialog / graph / autocomplete / model_selector / session_selector 优先）
- 编辑器按键绑定（基于 `KeybindingsManager`）
- Emacs 风格快捷键（`Ctrl+A` / `Ctrl+E` / `Ctrl+K` / `Ctrl+Y` / `Alt+Y` / `Ctrl+W` / `Alt+W` / `Ctrl+U` / `Ctrl+/` 等）
- Grapheme-aware 光标移动（处理多字节 Unicode 字符）
- 文本选区（`Shift+方向键`）
- 折叠/展开（`Ctrl+Shift+L` 折叠当前行，`Ctrl+Shift+K` 展开）
- Jump 模式（`Alt+J` / `Alt+K` 跳转到下一个/上一个输入字符）

### keymap.rs

快捷键管理器（合并后端绑定 + 用户自定义覆盖）：

```rust
// crates/rozsa-tui/src/input/keymap.rs

pub struct KeybindingsManager {
    backend_bindings: BTreeMap<String, Vec<String>>,
    user_overrides: BTreeMap<String, Vec<String>>,
}

impl KeybindingsManager {
    pub fn update_from_backend(&mut self, bindings: &BTreeMap<String, Vec<String>>);
    pub fn resolve(&self, action: &str) -> Vec<String>;
    pub fn matches_any(&self, key: &KeyEvent, actions: &[&str]) -> Option<String>;
}
```

### kill_ring.rs

剪切环（Emacs-style）：

```rust
pub struct KillRing {
    entries: Vec<String>,
    max_size: usize,
}

impl KillRing {
    pub fn push(&mut self, text: String);
    pub fn current(&self) -> Option<&str>;
    pub fn rotate(&mut self);
}
```

### undo.rs

撤销栈：

```rust
pub struct UndoStack<T> {
    stack: Vec<T>,
    max_size: usize,
}

impl<T> UndoStack<T> {
    pub fn push(&mut self, snapshot: T);
    pub fn pop(&mut self) -> Option<T>;
    pub fn clear(&mut self);
}
```

## Render 系统

### mod.rs

顶层渲染调度 + 缓存机制：

```rust
// crates/rozsa-tui/src/render/mod.rs

pub fn render(
    frame: &mut Frame,
    state: &AppState,
    input: &InputState,
    agents: Option<&dyn SubagentView>,
);

/// 消息渲染缓存 — 避免对未变化的消息重复格式化
/// key: hash(message + tools_expanded + thinking_visible + ...), value: Lines
thread_local! {
    static MSG_CACHE: RefCell<LruCache<u64, Vec<Line<'static>>>> = ...;
}

pub(crate) fn cached_message_lines(
    message: &AgentMessage,
    tools_expanded: bool,
    thinking_visible: bool,
    show_images: bool,
    compaction_collapsed: bool,
    is_last_streaming: bool,
    width: usize,
) -> Vec<Line<'static>>;
```

**渲染流程**：

1. 计算布局高度（notification / messages / pending / status / input / widgets）
2. 渲染主区域（notification / messages / pending / status / input / widgets）
3. 渲染 sidebar（宽度 ≥ 108 时）
4. 渲染 overlays（dialog / graph / permission / session_selector / model_selector）

**缓存策略**：

- 非 streaming 消息：根据 `hash(message + render flags + width)` 缓存格式化结果
- Streaming 最后一条消息：不缓存（内容持续变化）
- LRU 淘汰（容量 500）

### messages.rs

消息区渲染（消费 `AgentMessage` 枚举）：

```rust
pub fn render_messages(frame: &mut Frame, area: Rect, state: &AppState);

pub fn message_lines(
    message: &AgentMessage,
    tools_expanded: bool,
    thinking_visible: bool,
    show_images: bool,
    compaction_collapsed: bool,
    is_last_streaming: bool,
    width: usize,
) -> Vec<Line<'static>>;
```

**消息类型支持**：

- `AgentMessage::Standard { message: Message::User }` → 用户消息（蓝色边框）
- `AgentMessage::Standard { message: Message::Assistant }` → 助手消息（文本 + thinking + tool calls）
- `AgentMessage::Standard { message: Message::ToolResult }` → 工具结果
- `AgentMessage::Custom { message: CustomAgentMessage }` → 自定义消息（compactionSummary / bashExecution 等）

**格式化功能**：

- Markdown 渲染（标题、列表、代码块、链接）
- 语法高亮（通过 `syntect`）
- OSC 8 超链接（点击跳转到文件）
- 图片渲染（iTerm2 / Kitty / Sixel 协议）
- Thinking blocks 折叠/展开
- Tool calls 折叠/展开

### input_box.rs

输入框渲染：

```rust
pub fn render_input(
    frame: &mut Frame,
    area: Rect,
    input: &InputState,
    state: &AppState,
);
```

**功能**：

- 多行编辑器
- 光标定位（grapheme-aware）
- 折叠行显示（`[3 lines collapsed]`）
- 选区高亮（浅蓝背景）
- Paste marker 渲染（`[paste #1 +10 lines]`）
- Jump 模式高亮（跳转字符红色显示）
- Slash/At 前缀高亮（有 autocomplete 匹配时）

### status.rs

状态行 + 通知 + pending + widgets：

```rust
pub fn render_notifications(frame: &mut Frame, area: Rect, state: &AppState);
pub fn render_pending(frame: &mut Frame, area: Rect, ui: &NativeUiState);
pub fn render_status(frame: &mut Frame, area: Rect, state: &AppState);
pub fn render_widgets(frame: &mut Frame, area: Rect, widgets: &BTreeMap<String, Vec<String>>);
```

### dialog.rs

对话框渲染（settings / confirm 等）：

```rust
pub fn render_dialog(frame: &mut Frame, area: Rect, state: &DialogState);
pub fn centered_rect(percent_x: u16, percent_y: u16, base: Rect) -> Rect;
```

### overlay.rs

Overlay 焦点栈管理：

```rust
pub struct OverlayStack {
    stack: Vec<OverlayKind>,
}

pub enum OverlayKind {
    Permission,
    Dialog,
    Graph,
    SessionSelector,
    ModelSelector,
    Autocomplete,
}

impl OverlayStack {
    pub fn push(&mut self, kind: OverlayKind);
    pub fn pop(&mut self) -> Option<OverlayKind>;
    pub fn top(&self) -> Option<&OverlayKind>;
    pub fn clear(&mut self);
}
```

## 与其他 crate 的关系

- **rozsa-app**: 持有 `Arc<AgentSession>`，调用 `session.prompt()` / `session.compact()` / `session.set_model()` 等方法。
- **rozsa-core**: 消费 `AgentMessage` / `AgentEvent` 枚举，订阅 `AgentSession::subscribe()` 事件流。
- **rozsa-model**: 使用 `Model` / `ThinkingLevel` / `Message` / `ContentBlock` 等类型。
- **crossterm**: 终端控制（raw mode / alternate screen / event stream）。
- **ratatui**: TUI 渲染框架。

## 开发指南

### 新增 Panel

1. 在 `panels/` 下创建新文件（如 `panels/my_panel.rs`）。
2. 定义 State 结构：

```rust
pub struct MyPanelState {
    pub items: Vec<Item>,
    pub selected: usize,
}

impl MyPanelState {
    pub fn new(items: Vec<Item>) -> Self {
        Self { items, selected: 0 }
    }
}
```

3. 实现 `render_my_panel()` 函数：

```rust
pub fn render_my_panel(frame: &mut Frame, area: Rect, state: &MyPanelState) {
    // ratatui 渲染代码
}
```

4. 在 `input/keys.rs` 的 `handle_key()` 中添加按键处理分支：

```rust
if let Some(ref mut my_panel) = state.my_panel {
    match key.code {
        KeyCode::Up => my_panel.selected = my_panel.selected.saturating_sub(1),
        KeyCode::Down => my_panel.selected = (my_panel.selected + 1).min(my_panel.items.len() - 1),
        KeyCode::Enter => {
            // 确认选择
            state.my_panel = None;
        }
        KeyCode::Esc => {
            state.my_panel = None;
        }
        _ => {}
    }
    return Ok(());
}
```

5. 在 `render/mod.rs` 的 `render()` 中添加渲染调用：

```rust
if let Some(my_panel) = &state.my_panel {
    render_my_panel(frame, centered_rect(60, 50, frame.area()), my_panel);
}
```

6. 在 `app.rs` 的 `AppState` 中添加 `pub my_panel: Option<MyPanelState>` 字段。

### 新增 Widget

1. 在 `widgets/` 下创建新文件（如 `widgets/my_widget.rs`）。
2. 实现纯函数渲染接口：

```rust
pub fn render_my_widget(
    frame: &mut Frame,
    area: Rect,
    items: &[Item],
    selected: usize,
) {
    // ratatui 渲染代码
}
```

3. 在 `widgets/mod.rs` 中导出：

```rust
pub mod my_widget;
pub use my_widget::render_my_widget;
```

4. 在需要使用的地方调用：

```rust
use crate::widgets::render_my_widget;

render_my_widget(frame, area, &state.items, state.selected);
```

### 新增 slash command

在 `backend/native.rs` 的 `dispatch_slash_command()` 中添加分支：

```rust
match cmd {
    // ...
    "mycommand" => {
        // 处理逻辑
        self.notify("info", "Command executed");
    }
    // ...
}
```

如需添加到 help 文档，在 `rozsa-app/src/slash_commands.rs` 的 `BUILTIN_SLASH_COMMANDS` 中添加条目。

### 新增 BackendEvent 变体

1. 在 `backend/mod.rs` 的 `BackendEvent` 枚举中添加变体：

```rust
#[derive(Debug, Clone)]
pub enum BackendEvent {
    // ...
    MyEvent { data: String },
}
```

2. 在 `app.rs` 的 `apply_backend_event()` 中添加处理分支：

```rust
fn apply_backend_event(state: &mut AppState, event: BackendEvent, editor: &InputState) {
    match event {
        // ...
        BackendEvent::MyEvent { data } => {
            // 更新 state
        }
    }
}
```

3. 在 `backend/native.rs` 中推送事件：

```rust
let _ = self.event_tx.send(BackendEvent::MyEvent {
    data: "example".to_string(),
});
```

## 参考

- [TUI Architecture](../tui/architecture.md) — 目录规划 + 分类判定规则
- [Protocol](../../packages/coding-agent/src/modes/native/protocol.ts) — TypeScript 遗留协议定义（参考）
- [rozsa-app README](../rozsa-app/README.md) — AgentSession API 文档
- [rozsa-core README](../rozsa-core/README.md) — AgentEvent / AgentMessage 定义
