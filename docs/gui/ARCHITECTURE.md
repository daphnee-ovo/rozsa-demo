# Rózsa GUI 架构文档

本文档描述 Rózsa 桌面 GUI 的技术架构、目录结构、IPC 协议、状态管理、流式响应机制和权限系统集成。GUI 是唯一受支持的交互式前端；CLI 用于执行单次 prompt 或启动 GUI。

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

main WebView 还包含一个位于 MainContent/SettingsContent scene root 之外的公共 native interaction layer。sidebar edge trigger、collapsed state、native overlay reveal/hide 与 sidebar width range 由同一套前端行为共享；Settings scene 的全屏 settings surface 不得遮挡该公共 layer，也不应复制一套 sidebar hover 逻辑。

macOS 26 的 AppKit 会为 sidebar behavior 默认采用 floating appearance。Rózsa 在创建 sidebar item 前，通过应用自己的 `NSUserDefaults` domain 将 `NSSplitViewItemSidebarDefaultsToFloatingAppearance` 设为 `false`，保持 sidebar 与 main pane 的 inline 分栏视觉；不要写入全局 `-g` defaults。main item 不启用 `automaticallyAdjustsSafeAreaInsets`，因为两个 WebView 的内容边界由 split pane 直接管理，浮动 overlay 会遮挡 main WebView 的 composer。

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
| `get_settings` | - | `SettingsSnapshot` | 获取当前有效设置 |
| `update_setting` | `{ key: string, value: string }` | `()` | 更新 General / Appearance 中有运行时消费者的设置 |
| `get_capability_settings` | - | `CapabilitySettingsSnapshot` | 从实际注册 tools 和分层 skill 目录构建全局/项目能力清单 |
| `update_capability_setting` | `{ kind, scope, name, enabled }` | `CapabilitySettingsSnapshot` | 写入对应 `settings.json` 层；`enabled: null` 删除覆盖并恢复继承 |
| `update_permission_rules` | `{ scope, kind, rules }` | `PermissionSettingsSnapshot` | 替换一个 scope 的 deny/ask/allow 列表并校验 glob/RegExp 与路径边界 |
| `update_permission_rule_set` | `{ scope, deny, ask, allow }` | `PermissionSettingsSnapshot` | 单次写入三个规则列表，供跨容器拖拽原子迁移 |

`SettingsManager` 在 app 层负责 `settings.json` 的全局+项目逐项合并。
`AgentSession` 创建时读取合并结果；`/reload` 会重新加载 settings、过滤 skill registry，
并更新传给主 agent 与 subagent 的 tool 集合。GUI 只呈现 app 提供的能力清单，不复制
core tool 名称。

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

`askUserQuestion` 是强制可用的交互工具；它只有在 GUI 注入 question channel 时注册。
它仍然经过 `PermissionController`。默认全局 `permission.allow` 中的
`askUserQuestion(*)` 使其默认不产生 permission request；该规则在设置页可见、可删除，
并可被显式 `permission.deny` / `permission.ask` 覆盖。每个问题在 GUI 中始终附带
`Other` 自定义输入项，agent-facing schema 不提供关闭该项的开关。

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

### 5.5 Dev Flow 项目运行时

Dev Flow 集成由 app 层 adapter 和项目级 dashboard runtime 承担，GUI 不直接拼接
`dow` 参数、REST 路径或 SSE payload。adapter 负责 CLI 发现、版本探测、只读 REST
解码与兼容错误；runtime 以规范化 project root 和 revision 为 identity，使同一项目的
多个 session 共享服务与状态，而不是把 dashboard 绑定到当前 session。

sidebar 和 Settings 消费同一份只读 snapshot。项目文件变化经 dashboard watcher、
SSE `update` 事件和 adapter refresh 更新 snapshot；断线重连与 revision 切换不能销毁
用户可能返回的旧服务。没有项目活跃 session 15 分钟后才进入回收候选，并受
`max(系统内存 5%, 256 MiB)` 的总内存阈值约束。错误通过公共 notification center
呈现，6 秒后折叠进 unresolved error tray；成功与普通就绪状态不主动打扰。

完整约定见 [`DEV_FLOW_INTEGRATION.md`](./DEV_FLOW_INTEGRATION.md)。

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
| `auto-approve` | 预留给 small-model reviewer；当前 GUI 拒绝保存并报告未实现 |
| `yolo` | 跳过普通审批，但保留内置破坏性命令保护 |

### 7.2 审批流程

1. Agent 调用工具（如 `Bash { command: "rm -rf ..." }`）。
2. `PermissionController` 拦截，先检查内置危险命令，再按 `deny > ask > allow` 处理分层规则。
   默认全局 allow 显式包含 `subagent(*)`、`askUserQuestion(*)`，以及末尾
   `Read-only Bash` 分组中的安全项目内 Shell 规则。只读 Bash 规则仍由运行时
   校验命令参数、管道、写入选项和项目路径，并且和其他 allow 规则一样可编辑；
   GUI 将该分组放在独立且默认收起的折叠容器中。
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
- [Dev Flow GUI integration](./DEV_FLOW_INTEGRATION.md) — Settings、sidebar、runtime ownership 与 adapter 边界
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
