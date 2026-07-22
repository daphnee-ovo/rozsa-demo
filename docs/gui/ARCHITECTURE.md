# Rózsa GUI 架构文档

本文档描述 Rózsa 桌面 GUI 的技术架构、目录结构、IPC 协议、状态管理、流式响应机制和权限系统集成。GUI 替代之前的 ratatui TUI 作为默认交互界面，CLI 通过 `--tui` 标志保留 TUI 回退。

## 1. 概述

Rózsa GUI 基于固定版本 **Tauri 2.11.5** 构建，采用 Rust 后端 + Web 前端的跨平台架构：

- **Rust 后端**：负责 agent 会话管理、工具执行、权限审批、状态持久化。
- **Tauri IPC 层**：前端通过 `invoke()` 调用后端命令，后端按 `main` / `sidebar` WebView 定向推送事件。
- **Web 前端**：macOS 使用两个持久 WebView；非 macOS 在 main WebView 中保留 CSS fallback 布局。
- **macOS 原生容器**：一个 `NSWindow` 内安装一个 `NativeSplitHost`，由 `NSSplitViewController` 管理 sidebar 与 main panel。

**设计目标**：

- 让用户清楚看到 agent 正在做什么、用了哪些工具、是否需要权限确认。
- 支持流式响应，逐步显示 thinking / message / tool calls。
- 保持高信息密度，适合长期停留和代码审阅的工程师。

## 2. 目录结构

```
crates/rozsa-gui/
├── Cargo.toml              — Rust crate 配置，依赖 tauri + rozsa-app/core/model
├── build.rs                — Tauri build 脚本（生成 schema + capabilities）
├── tauri.conf.json         — Tauri 配置：窗口尺寸、bundle、安全策略
├── src/
│   ├── lib.rs              — crate 入口，GuiConfig + run() 公开接口
│   ├── state.rs            — GuiState 共享状态 + 前端数据结构
│   ├── commands.rs         — Tauri IPC 命令处理器（send_message, get_sessions 等）
│   ├── events.rs           — main/sidebar targeted event + 权限监听任务
│   ├── scene_router.rs     — revisioned GuiScene 与 WebView ready 协调
│   ├── native_split_view.rs— macOS NSSplitViewController 与 sidebar backing
│   └── native_titlebar.rs  — macOS drag/toggle/zoom/fullscreen chrome
└── frontend/
    ├── index.html          — main WebView 的 MainContent/SettingsContent roots
    ├── app.js              — main scene、chat、permission、settings form
    ├── sidebar.html        — sidebar WebView 的两个 sidebar scene roots
    ├── sidebar.js          — session/status 与 settings navigation
    └── gui_shared.js       — scene/theme revision 与共享 DOM helper
```

**crate 定位**：

- `rozsa-gui` 是 library crate (`[lib]`)，不是可执行 crate。
- `rozsa-cli` 调用 `rozsa_gui::run()` 启动 GUI。
- 所有 Rust 依赖都在后端，前端不依赖 npm/yarn。

## 3. 架构分层

### 3.1 Rust 后端

后端职责：

- **会话管理**：`AgentSession` 持有对话历史、工具执行器、权限系统。
- **模型注册**：`ModelRegistry` 存储可用模型列表，支持运行时切换。
- **权限审批**：`PendingApprovals` (DashMap) 持有待审批请求，`permission_request_rx` 接收新请求。
- **Agent 提问**：`PendingUserQuestions` (DashMap) 持有 `askUserQuestion` 的待回答请求，`question_request_rx` 接收新请求。
- **状态共享**：`GuiState` 包装上述资源，通过 `tauri::State` 注入到 IPC 命令。

核心类型：

- `GuiConfig`：外部注入配置，包含 `session`, `model_registry`, `session_dir`, `pending_approvals`, `permission_request_rx` 和 `question_request_rx`。
- `GuiState`：Tauri managed state，所有 IPC 命令通过 `State<'_, GuiState>` 访问。

### 3.2 Tauri IPC 层

**前端 → 后端（Commands）**：

前端调用 `window.__TAURI__.core.invoke(command, payload)`，后端通过 `#[tauri::command]` 注册的函数处理。

**后端 → 前端（Events）**：

后端通过 `emit_to("main", ...)`、`emit_to("sidebar", ...)` 或显式的 `emit_both(...)` 推送事件。前端仍通过 `window.__TAURI__.event.listen(event_name, handler)` 监听，但两个 WebView 只订阅各自职责内的事件。

### 3.3 Web 前端

前端文件：

- `frontend/index.html` + `app.js`：持久 main WebView。Main scene 承载 chat/composer/permission/question；Settings scene 承载当前 settings pane。
- `frontend/sidebar.html` + `sidebar.js`：持久 sidebar WebView。Main scene 承载 session/status；Settings scene 承载 settings navigation。
- `frontend/gui_shared.js`：两个 WebView 共用的 scene/theme revision 应用规则。

前端职责：

- 渲染聊天流、工具调用、权限面板、agent question 面板、设置面板。
- main WebView 监听 `ui-state`、`tool-event`、`permission-request` 和 `question-request`，逐步更新 UI。
- 按 session 保存 draft、selection、scroll、展开状态和 permission UI progress。

### 3.4 macOS 原生 split 与 scene

macOS 运行时结构固定为：

```text
NSWindow
├─ NativeTitlebarHost
│  ├─ TitlebarDragView
│  └─ sidebar toggle -> NSSplitViewController.toggleSidebar
└─ NativeSplitHost / NSSplitViewController
   ├─ sidebar NSSplitViewItem -> sidebar WebView
   │  ├─ MainSidebar
   │  └─ SettingsSidebar
   └─ main NSSplitViewItem -> main WebView
      ├─ MainContent
      └─ SettingsContent
```

两个 split item 和两个 WebView 在 Main/Settings 切换期间保持 identity。`scene_router.rs` 持有窗口级 `GuiScene` 和 revision；前端只切换预创建 root 的 `hidden` / `inert`，不 reload 或重建 stateful roots。pane frame、divider、collapse、fullscreen overlay 和 width persistence 只由 AppKit 管理。

窗口配置为初始隐藏。main 与 sidebar WebView 分别完成 `gui_webview_ready` 后，`scene_router.rs` 才允许显示同一个 `NSWindow`，避免启动和原生重挂载期间先暴露单独 pane。非 macOS fallback 没有第二个 WebView，在完成单 WebView setup 后直接显示窗口。

实现锚点：[`native_split_view.rs`](../../crates/rozsa-gui/src/native_split_view.rs)、[`native_titlebar.rs`](../../crates/rozsa-gui/src/native_titlebar.rs)、[`scene_router.rs`](../../crates/rozsa-gui/src/scene_router.rs)、[`gui_shared.js`](../../crates/rozsa-gui/frontend/gui_shared.js)。验证记录见 [`NATIVE_SPLIT_VALIDATION.md`](./NATIVE_SPLIT_VALIDATION.md)。

## 4. IPC 协议

### 4.1 Commands（前端 → 后端）

所有 commands 返回 `Result<T, String>`，错误通过 `String` 传递给前端。

| 命令 | 参数 | 返回值 | 说明 |
|------|------|--------|------|
| `send_message` | `{ message: string }` | `()` | 发送用户消息，触发 agent loop，通过 `ui-state` / `tool-event` 定向返回 |
| `get_sessions` | - | `SessionInfo[]` | 列出所有会话（从 `session_dir` 读取 `.jsonl` 文件） |
| `switch_session` | `{ sessionId: string }` | `()` | 切换到指定会话 |
| `new_session` | - | `string` | 创建新会话并返回 ID |
| `approve_permission` | `{ requestId: string }` | `()` | 批准权限请求 |
| `deny_permission` | `{ requestId: string }` | `()` | 拒绝权限请求 |
| `respond_user_question` | `{ sessionId: string, id: string, answers: object }` | `()` | 提交 `askUserQuestion` 的单选/多选结果 |
| `get_settings` | - | `JsonValue` | 获取当前设置（从 `AgentSession.settings_manager()` 读取） |
| `update_settings` | `{ key: string, value: JsonValue }` | `()` | 更新单个设置项（目前支持 `thinking_enabled` / `model`） |

### 4.2 Events（后端 → 前端）

事件按消费者职责定向，不使用 app-global consumer：

| Event | Target | 说明 |
| --- | --- | --- |
| `gui-scene-snapshot` | ready 的 `main` / `sidebar` | 完整 `{revision, scene, selectedPane}`；旧 revision 丢弃 |
| `ui-state` | `main` | active session 的消息与流式 UI snapshot |
| `tool-event` | `main` | tool 生命周期 |
| `permission-request` | `main` | 权限审批请求 |
| `question-request` | `main` | `askUserQuestion` 的问题、选项和 request 标识 |
| `sidebar-state` | `sidebar` | session/status/git/quota/actions 完整 snapshot |
| `theme-state` | native host，再到 `main` + `sidebar` | 原生 backing 先更新，再发布同 revision 完整 theme snapshot |

`theme-state` 中包含 light/dark theme definition。`translucentSidebar=true` 时原生 host 显示系统 sidebar material；关闭时显示当前主题的 opaque sidebar color。sidebar WebView 本身始终透明。

### 4.3 权限事件流

权限审批通过专用通道推送到前端：

1. 后端权限系统生成审批请求，通过 `permission_request_rx` (mpsc channel) 发送 `(request_id, ApprovalInfo)`。
2. `events.rs::spawn_permission_listener()` 在后台任务中监听通道，收到请求后向 main WebView 发送 `permission-request`。
3. 前端弹出权限面板，用户点击 Approve/Deny。
4. 前端调用 `approve_permission(requestId)` 或 `deny_permission(requestId)`。
5. 后端从 `PendingApprovals` (DashMap) 中取出对应的 oneshot sender，发送 `PermissionResponse::Allow` 或 `Deny`。
6. Agent loop 收到响应，继续执行或回退。

### 4.4 Agent question 事件流

`askUserQuestion` 是强制可用的交互工具；它只有在 GUI 注入 question channel 时注册。它仍然经过 `PermissionController`，但在默认 `allowed_tools` 白名单中，因此不会产生 permission request；显式 `permission.deny` / `permission.ask` 规则仍可覆盖默认允许。每个问题在 GUI 中始终附带 `Other` 自定义输入项，agent-facing schema 不提供关闭该项的开关。

1. Agent tool 校验 `questions`，为本次调用创建 request ID，并通过 `question_request_tx` 发送问题与 oneshot sender。
2. `events.rs::spawn_user_question_listener()` 将请求放入按 `session_id + request_id` 隔离的 `PendingUserQuestions`，再向 main WebView 发送 `question-request`。
3. 前端按页显示问题；单选返回一个字符串，多选返回字符串数组；自定义输入直接作为答案值。
4. 前端调用 `respond_user_question(sessionId, id, answers)`，后端校验问题数量、单/多选形状并原子移除 pending request。
5. Agent tool 收到 `Answered` 后返回 `{"answers": ...}`；窗口关闭、abort、session 删除或 channel 断开则返回取消/错误，不伪造答案。

## 5. 状态管理

### 5.1 后端状态（GuiState）

`GuiState` 是 Tauri managed state，所有 IPC 命令通过 `State<'_, GuiState>` 访问。

```rust
pub struct GuiState {
    pub session: Arc<AgentSession>,              // Agent 会话，驱动 LLM 交互
    pub model_registry: Option<Arc<ModelRegistry>>, // 模型注册表，用于切换模型
    pub session_dir: Option<PathBuf>,            // 会话持久化目录
    pub pending_approvals: Option<PendingApprovals>, // 待审批权限 (DashMap)
    pub active_session_id: Arc<Mutex<Option<String>>>, // 当前会话 ID
    pub is_running: Arc<Mutex<bool>>,            // Agent loop 是否运行中
}
```

**线程安全**：

- `Arc` 包装的类型可跨 IPC 命令共享（Tauri 命令是异步执行的）。
- `Mutex` 保护可变状态（`active_session_id`, `is_running`）。
- `PendingApprovals` 是 `Arc<DashMap<...>>`，支持并发访问。

### 5.2 前端状态（SessionStore）

前端通过 `SessionStore` 管理本地会话数据：

```javascript
class SessionStore {
  currentSessionId = null;
  sessions = {}; // { sessionId: { messages: [...] } }
  
  addMessage(sessionId, role, content) { ... }
  getMessages(sessionId) { ... }
  clear(sessionId) { ... }
}
```

**流式更新**：

- main WebView 监听 `ui-state` / `tool-event`，根据 snapshot 和事件更新 UI：
  - `MessageStart`：创建新消息占位符。
  - `MessageDelta`：追加文本到当前消息。
  - `ToolCallStart`：插入工具调用占位符。
  - `ToolCallEnd`：更新工具调用状态和输出。

### 5.3 权限审批状态（PendingApprovals）

`PendingApprovals` 是 `Arc<DashMap<String, (ApprovalInfo, oneshot::Sender<PermissionResponse>)>>`。

**生命周期**：

1. Agent 执行工具调用，权限系统拦截。
2. 权限系统生成唯一 `request_id`，将 `(ApprovalInfo, oneshot_tx)` 插入 `PendingApprovals`。
3. 通过 `permission_request_rx` 通知 GUI。
4. GUI emit `PermissionRequired` 事件到前端。
5. 用户点击 Approve/Deny，前端调用 `approve_permission` 或 `deny_permission`。
6. 后端从 `PendingApprovals` 中 `remove(request_id)`，取出 `oneshot_tx` 并发送响应。
7. Agent 收到响应，继续或中止工具执行。

### 5.4 Agent question 状态（PendingUserQuestions）

`PendingUserQuestions` 是按 `session_id + request_id` 索引的 `Arc<DashMap<...>>`，值包含问题定义和 `oneshot::Sender<AskUserQuestionResponse>`。响应命令只允许消费一次 pending request；取消和窗口关闭按 session 清理，避免后台 session 的问题覆盖当前 tab。

## 6. 流式响应机制

### 6.1 Agent 事件流

`AgentSession` 内部使用 `tokio::sync::broadcast` channel 广播事件：

```rust
impl AgentSession {
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<AgentEvent> { ... }
}
```

**事件类型**：

- `MessageStart` / `MessageUpdate` / `MessageEnd`：消息生命周期。
- `ToolExecutionStart` / `ToolExecutionEnd`：工具调用生命周期。
- `AgentEnd`：Agent loop 结束。

### 6.2 GUI 订阅流程

`send_message` 命令的实现：

```rust
#[tauri::command]
pub async fn send_message(
    state: State<'_, GuiState>,
    app_handle: AppHandle,
    message: String,
) -> Result<(), String> {
    // 1. 检查 is_running，避免并发执行
    // 2. 订阅 session.subscribe() 获取 event receiver
    // 3. 克隆 app_handle 到 spawned task
    // 4. spawn agent loop: session.prompt(&message)
    // 5. event forwarder 把 active session snapshot 定向发送到 main WebView
    // 6. agent loop 结束后设置 is_running = false
}
```

**关键点**：

- **订阅先于 spawn**：避免丢失事件（broadcast channel 会 lag 但不会丢失已发送的事件）。
- **两个并行任务**：
  - Task 1: `session.prompt()` 执行 agent loop。
  - Task 2: 监听 event stream，更新 live state 并向 main WebView 发布定向事件。
- **事件监听终止**：收到 `AgentEnd` 或 `RecvError::Closed` 时退出 loop。

### 6.3 前端流式渲染

main WebView 分别监听定向事件：

```javascript
window.__TAURI__.event.listen("ui-state", event => renderState(event.payload));
window.__TAURI__.event.listen("tool-event", event => handleToolEvent(event.payload));
window.__TAURI__.event.listen("permission-request", event => showPermission(event.payload));
window.__TAURI__.event.listen("question-request", event => showUserQuestion(event.payload));
```

## 7. 权限系统集成

### 7.1 权限模式

Rózsa 支持三种权限模式（配置在 settings.json）：

| 模式 | 行为 |
|------|------|
| `on-request` | 所有敏感操作都需要用户批准 |
| `auto-approve` | 自动批准所有操作（开发模式） |
| `free-permission` | 跳过权限检查（无防护模式，仅用于测试） |

### 7.2 审批流程

1. Agent 调用工具（如 `Bash { command: "rm -rf ..." }`）。
2. `PermissionGuard` 拦截，检查权限模式和 allowlist。
3. 如果需要审批：
   - 生成 `request_id`（UUID）。
   - 创建 oneshot channel `(tx, rx)`。
   - 将 `(ApprovalInfo, tx)` 插入 `PendingApprovals`。
   - 通过 `permission_request_tx` 发送 `(request_id, ApprovalInfo)` 到 GUI。
   - 阻塞等待 `rx.recv()`。
4. GUI emit `PermissionRequired` 事件到前端。
5. 前端弹出权限面板，显示风险类型、工具名、命令预览。
6. 用户点击 Approve/Deny。
7. 前端调用 `approve_permission(request_id)` 或 `deny_permission(request_id)`。
8. 后端从 `PendingApprovals` 中取出 `tx`，发送 `PermissionResponse::Allow` 或 `Deny`。
9. `PermissionGuard` 收到响应，返回 Allow/Deny 给工具执行器。
10. Agent 继续执行或报错中止。

### 7.3 数据结构

**ApprovalInfo**：

```rust
pub struct ApprovalInfo {
    pub tool_name: String,
    pub args_summary: String,
    pub risk: RiskLevel,
}
```

**PendingApprovals**：

```rust
pub type PendingApprovals = Arc<DashMap<String, (ApprovalInfo, oneshot::Sender<PermissionResponse>)>>;
```

**PermissionResponse**：

```rust
pub enum PermissionResponse {
    Allow,
    Deny,
}
```

## 8. 构建与运行

### 8.1 开发模式

```bash
cd crates/rozsa-gui
cargo tauri dev
```

**行为**：

- Cargo 编译 Rust 后端。
- Tauri 启动 webview，加载 `frontend/index.html`。
- 支持热重载（修改 `index.html` 自动刷新）。

### 8.2 生产构建

```bash
cargo tauri build
```

**行为**：

- 编译 release 版 Rust 后端。
- 打包 `frontend/` 到可执行文件。
- 输出 `.dmg` (macOS) / `.deb` / `.AppImage` (Linux) / `.exe` (Windows)。

### 8.3 CLI 集成

`rozsa-cli` 通过 `--gui` 标志启动 GUI：

```rust
// crates/rozsa-cli/src/main.rs
if matches.get_flag("gui") {
    let gui_config = GuiConfig {
        session,
        model_registry: Some(registry),
        session_dir: Some(session_dir),
        pending_approvals: Some(pending_approvals),
        permission_request_rx: Some(permission_rx),
    };
    rozsa_gui::run(gui_config).await?;
} else if matches.get_flag("tui") {
    // 启动 TUI
} else {
    // 默认 GUI
}
```

## 9. 设计规范引用

GUI 视觉设计和交互规范见 [UI_USAGE_GUIDELINES.md](./UI_USAGE_GUIDELINES.md)。

核心原则：

- **安静的结构感**：1px 发丝线、轻背景、清晰分区，不靠厚重卡片和阴影。
- **代码是主角**：聊天、工具调用、代码块、diff 是页面核心。
- **玫红只做信号**：品牌色 `#c0737a` 用于焦点、激活、主操作，不做大面积背景。
- **状态真实**：运行中、成功、失败、等待审批等状态必须具体可见。
- **工具调用轻量**：折叠态无竖线、低透明度，展开态竖线从图标下方开始。

设计参考原型：`docs/gui/index.html`。

## 10. 未来扩展

### 10.1 优先扩展

- **多会话搜索与分组**：在 `get_sessions` 中支持过滤和分类。
- **工具调用过滤和错误定位**：在 `ToolCallEnd` 中附加错误堆栈和文件位置。
- **权限策略模板**：在 settings 中支持保存/加载自定义 allowlist。
- **代码预览与 diff 独立面板**：在 `ToolCallEnd` 中支持打开独立 diff viewer。
- **周限额明细和重置时间**：在 left sidebar 显示限额使用趋势图。

### 10.2 谨慎扩展

- **暗色模式**：必须保持 Rózsa 的温暖气质，不变成黑色终端。配色需重新调校，避免高对比度伤眼。
- **多窗口**：需要同步 `GuiState` 到多个窗口，或将状态移到后端 daemon。
- **插件系统**：需要设计插件 IPC 协议和沙箱隔离。
- **移动端**：应重排信息架构（单栏或底部抽屉），不能简单压缩桌面三栏。

---

## 相关文档

- [UI 使用规范](./UI_USAGE_GUIDELINES.md) — GUI 视觉设计和交互规范
- [GUI 术语表](./TERMINOLOGY.md) — session、permission、输入、tool/diff 和窗口层的共同词汇与文字图
- [Agent Session](../../crates/rozsa-app/src/agent_session.rs) — Agent 会话管理
- [Permission System](../../crates/rozsa-app/src/permissions/mod.rs) — 权限系统实现
- [Core Events](../../crates/rozsa-core/src/events.rs) — Agent 事件定义

## 架构图

```
NSWindow
├─ NativeTitlebarHost
└─ NativeSplitHost / NSSplitViewController
   ├─ sidebar NSSplitViewItem -> persistent sidebar WebView
   │  └─ MainSidebar | SettingsSidebar
   └─ main NSSplitViewItem -> persistent main WebView
      └─ MainContent | SettingsContent
             │
             ├─ invoke(command)
             ▼
      commands.rs + scene_router.rs + events.rs
             │
             ├─ emit_to(main/sidebar)
             ▼
      GuiState + AgentSession + PermissionController
```
