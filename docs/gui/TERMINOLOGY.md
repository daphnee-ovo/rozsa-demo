# Rózsa GUI 术语表与沟通图

这份文档是 GUI 讨论的共同词汇。它描述当前实现，不把视觉原型、旧架构文档或历史命名当成运行时事实。

判断问题落在哪一层时，先用这组边界：

```text
main DOM / 输入 / Settings form -> frontend/app.js + index.html
sidebar scene / session 导航    -> frontend/sidebar.js + sidebar.html
Tauri command / event / tab 状态 -> crates/rozsa-gui/src/
Agent loop / tool / session 文件 -> crates/rozsa-app/ + crates/rozsa-core/
macOS pane / 窗口行为           -> native_split_view.rs + native_titlebar.rs
```

## 1. 总体分层

```text
┌─────────────────────────────────────────────────────────────┐
│ macOS NSWindow                                               │
│  native chrome / traffic lights / fullscreen / window zoom    │
│  └─ TitlebarDragView + sidebar accessory                     │
│                                                             │
│  NativeSplitHost / NSSplitViewController                     │
│  ├─ sidebar pane -> persistent sidebar WebView               │
│  │  MainSidebar | SettingsSidebar                            │
│  └─ main pane -> persistent main WebView                     │
│     MainContent | SettingsContent                            │
│                  │ invoke / targeted event                   │
│                  │ invoke(command) / listen(event)             │
│  ┌───────────────▼─────────────────────────────────────────┐  │
│  │ rozsa-gui Rust runtime                                   │  │
│  │  GuiState / SessionTab / LiveState / commands / events   │  │
│  │  SharedResources / PermissionController                  │  │
│  └───────────────┬─────────────────────────────────────────┘  │
│                  │                                              │
│  ┌───────────────▼─────────────────────────────────────────┐  │
│  │ rozsa-app + rozsa-core                                   │  │
│  │  AgentSession / agent loop / tools / permission policy    │  │
│  └─────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
          │                         │
          ▼                         ▼
     session/*.jsonl          .rozsa/settings.json
                              ~/.rozsa/themes/*.json
```

### 分层术语

| 术语 | 当前含义 | 不要混称为 |
| --- | --- | --- |
| GUI | 整个 Tauri 桌面应用，包含 WebView、Rust runtime 和原生窗口层 | 只有前端页面 |
| frontend | 两个 WebView 内的 `index.html`/`app.js` 与 `sidebar.html`/`sidebar.js`，负责 DOM、输入和显示状态 | GUI runtime |
| GUI runtime | `rozsa-gui` Rust 层，负责 tabs、commands、events 和后端 session 的拥有关系 | 浏览器状态 |
| IPC | frontend 与 Rust 之间的命令/事件通道；前端 `invoke`，后端 `emit` | Agent event stream 本身 |
| backend | 需要按上下文说明：通常指 `rozsa-gui` Rust runtime；若指 agent loop，应说 `AgentSession` 或 `rozsa-app` | 一个没有边界的“后端” |
| bootstrap | CLI 传给 GUI 的初始配置和资源；不等于 CLI 长期持有 live session | session owner |
| GUI-owned | GUI 通过 `SharedResources` factory 创建、恢复和管理 `AgentSession` | frontend 自己执行 agent |

代码锚点：[`lib.rs`](../../crates/rozsa-gui/src/lib.rs)、[`state.rs`](../../crates/rozsa-gui/src/state.rs)、[`commands.rs`](../../crates/rozsa-gui/src/commands.rs)、[`events.rs`](../../crates/rozsa-gui/src/events.rs)。

## 2. Session、Tab 与持久化

### 2.1 三个容易混淆的对象

| 术语 | 含义 | 生命周期/位置 |
| --- | --- | --- |
| `session_id` | 一个会话的稳定 ID，当前由 session 文件名得到 | Rust 与 frontend event 都带它 |
| `SessionTab` | GUI 对一个会话 tab 的状态模型 | `Idle`、`Loaded`、`Active` |
| `AgentSession` | 处理 prompt、事件、工具、取消和 session history 的 live agent backend | 只由 GUI factory 创建或恢复 |
| session file | 会话历史和 entries 的持久化文件 | `session_dir/<session_id>.jsonl` |
| active session | 当前 UI 正在显示、可接收 snapshot 的 session | 由 `GuiState.active_tab` 选出 |
| session-scoped UI memory | 只保存在当前 GUI 进程中的草稿、selection、滚动、展开项、permission UI 进度 | frontend 的对象 map；不是 `.jsonl` 持久化 |

`SessionTab` 的状态可按下面理解：

```text
session directory scan
        │
        ▼
   Idle: 只有路径和元数据
        │ 用户切换到它
        ▼
 Loaded: 读入历史消息，但没有 live AgentSession
        │ 首次需要运行 agent
        ▼
 Active: 由 SharedResources factory 创建的 AgentSession + LiveState
```

切 session 时，用“切换 tab / active session”描述 UI，用“恢复 session backend”描述 Rust；不要说“前端创建了 AgentSession”。

### 2.2 Session 切换与 UI 恢复

```text
用户点击 session item
        │
        ├─ frontend 保存旧 session 的 draft / selection / permission UI / scroll
        │
        ▼
invoke("switch_session", { path })
        │
        ▼
GuiState.active_tab ──> SessionTab(session_id)
        │                    │
        │                    ├─ 已 Active：复用对应 AgentSession
        │                    └─ Idle/Loaded：通过同一 factory restore_agent()
        │
        ▼
emit("ui-state")
        │
        ▼
renderState(snapshot)
        ├─ 恢复该 session 的消息、tool activity、streaming 状态
        ├─ 恢复 sessionDraftState
        ├─ 恢复 permissionUiStateBySession（若仍是同一 request）
        └─ 恢复 sessionViewState
```

这里的“session memory”默认指 UI memory；如果要表达重启后仍存在，必须说“session file persistence”或“settings persistence”。

### 2.3 相关说法

- `new session`：创建新的 session file 和 GUI tab。
- `restore session`：从已有 `.jsonl` 打开历史，并按同一 factory 创建 live backend。
- `switch session`：改变 active tab，不是把多个会话合并。
- `fork session`：从某个历史消息点创建新的会话分支。
- `session view state`：该 session 的滚动位置、展开状态等显示记忆。
- `session draft state`：该 session 的输入文字、selection range 和运行中发送模式。

## 3. 一次 Turn、Streaming、Queue 与 Abort

这里的 `turn` 是一次用户提交触发的 agent 交互；`interaction` 是 GUI 为文件变更和验证摘要维护的同一轮边界。

```text
contenteditable input
        │
        ├─ idle: invoke("send_message")
        └─ running: invoke("send_running_message", queue|steer)
        │
        ▼
commands.rs 找到 active SessionTab
        │
        ▼
AgentSession::prompt()
        │
        ▼
agent loop 产生 AgentEvent
        ├─ MessageStart/Update/End ──> "ui-state" ──> streaming message
        ├─ ToolExecutionStart/End ──> "tool-event" ──> tool call / result
        ├─ Permission request ───────> "permission-request" ──> approval panel
        ├─ askUserQuestion ──────────> "question-request" ──> question panel
        └─ AgentEnd ─────────────────> turn 收束、持久化、TurnActivity summary
        │
        ├─ abort command -> CancellationToken.cancel() -> loop 在检查点停止
        └─ tool result details -> file delta / verification -> GUI summary
```

| 术语 | 当前含义 |
| --- | --- |
| streaming | Agent loop 尚未结束，frontend 按事件增量刷新当前消息或状态 |
| `ui-state` | 当前 active session 的 UI snapshot；不是每个后台 session 都能直接覆盖当前画面 |
| `tool-event` | 工具开始/结束的独立事件，带 `session_id`、`turn_id`、tool 名称和结果详情 |
| `tool call` | agent 请求执行某个工具的动作和参数 |
| `tool result` | 工具执行后的内容和 `details`；可能包含 `file_deltas` 或验证字段 |
| `askUserQuestion` | GUI-only 的 agent 交互工具；一次请求可包含 1–4 个问题，每题单选或多选 |
| `question-request` | 后端发给 main WebView 的待回答问题事件；按 `session_id` 和 request ID 隔离 |
| `Other` | 每个问题始终存在的自定义输入选项；不是 agent 可关闭的 schema 开关 |
| `question result` | `{"answers": {header: string | string[]}}`；单选是字符串，多选是字符串数组 |
| queue | GUI-owned FIFO；当前 prompt 返回后再启动下一条消息 |
| steer | 将运行中的用户输入交给当前 agent，在下一个工具结果之间处理；不等同于 queue |
| abort | Stop button 或 1 秒内连续两次 Escape 发送取消信号，并丢弃尚未执行的 queue / steer / follow-up；不删除 session、不清空历史，也不撤销已完成的文件变更 |
| partial output | abort 或错误前已经收到的流式输出；讨论时要说明它是 UI 已显示内容还是已持久化 message |

代码锚点：[`agent_session.rs`](../../crates/rozsa-app/src/agent_session.rs)、[`events.rs`](../../crates/rozsa-gui/src/events.rs)、[`app.js`](../../crates/rozsa-gui/frontend/app.js)。

## 4. Tool、File Delta、Turn Activity 与 Workspace Diff

这几个词都和“改了什么”有关，但来源不同：

| 术语 | 来源 | 适合回答的问题 |
| --- | --- | --- |
| tool call/result | agent event + tool result | 这次调用了什么工具、结果怎样？ |
| `FileDelta` | `ToolResult.details.file_deltas` | 某个文件前后内容和 patch 是什么？ |
| `TurnActivity` | GUI 聚合本 turn 的 file delta 与 verification | 这次用户请求改了哪些文件、验证命令是否通过？ |
| `TurnSummary` | 按 assistant message 关联的持久化摘要 | 历史中哪条回复对应哪些变更？ |
| workspace diff | Git 工作区读取结果 | 当前工作区相对 Git 基线有什么差异？ |

```text
Read / Write / Edit / Bash tool result
                 │
                 ├─ details.file_deltas ──> FileDelta(path, status, patch, +/-)
                 ├─ details.changed_files -> 只有路径的 opaque summary
                 └─ Bash details ----------> VerificationResult(command, exit, timeout...)
                                │
                                ▼
                       TurnDiffAccumulator
                                │
                                ▼
                         TurnActivity
                    changed_files + file_changes
                    verification + capture status
```

沟通时说“看本 turn 的变更摘要”表示 `TurnActivity`；说“看当前工作区 diff”才表示 Git diff。不要用“diff”笼统代替两者。

代码锚点：[`file_delta.rs`](../../crates/rozsa-app/src/tools/file_delta.rs)、[`turn_diff.rs`](../../crates/rozsa-gui/src/turn_diff.rs)、[`git_diff.rs`](../../crates/rozsa-gui/src/git_diff.rs)。

## 5. Permission、Policy 与 Trust

### 5.1 决策层次

```text
PermissionController.evaluate(session_id, tool, args)
        │
        ├─ deny / blocked command / workspace boundary -> Block
        ├─ project ask rule or policy requires review  -> NeedApproval
        ├─ project allow rule                           -> Allow
        └─ session_approvals[session_id]               -> Allow
                         │
                         ▼
              NeedApproval 时生成 ApprovalInfo
              trust_groups / trust_levels / trust_key
                         │
             ┌───────────┼─────────────────────┐
             ▼           ▼                     ▼
       Allow         AllowSession           Deny / DenyWithHint
       仅本次        写入当前 session memory  停止或给替代建议
```

### 5.2 关键术语

| 术语 | 当前含义 | 边界 |
| --- | --- | --- |
| `PermissionController` | runtime-owned 权限控制器，按 `session_id` 评估并记录信任 | 不是只改 UI 的开关 |
| `PermissionMode` | `OnRequest`、`AutoApprove`、`Yolo` 三种策略模式 | `auto-approve` 是易读说法；讨论配置解析时保留代码枚举名 |
| `PolicyVerdict` | `Allow`、`Block`、`NeedApproval` | 一次调用的 runtime 裁定 |
| default allowed tool | 内置加入 `allowed_tools` 的无副作用工具，例如 `askUserQuestion` | 仍经过 `PermissionController`；显式 `deny` / `ask` 规则可以覆盖 |
| `ApprovalInfo` | 提交给 UI 的工具、参数摘要、风险和可选 trust scope | 不是最终用户决定 |
| `trust_key` | 一个可复用的授权范围的规范 key | 不要把它当 request ID |
| `TrustLevel` | 一个可选的信任范围，例如精确命令或更宽前缀 | 是“选择项” |
| `TrustGroup` | 一组属于同一目标的 `TrustLevel`；复合 shell 命令可有多个 group | 允许分别选择各段 |
| current-call allow | `Allow`，只放行当前 request | 不创建 session trust |
| session trust | `AllowSession`；记在 `session_approvals[session_id]` | 进程内、按 session 隔离 |
| project trust | `record_project_approval` 写入 project `permission.allow` | 可被同项目后续 session 复用 |
| `DenyWithHint` | 拒绝当前动作，同时给 agent 更安全的替代建议 | 不是普通错误字符串 |
| pending permission queue | frontend 的 `pendingPermissions[session_id]` | 后台 session 的请求不能覆盖当前 tab 的面板 |
| permission UI state | 当前 request 的 trust 页面、选择项和 focus 位置 | 不是实际授权结果 |

### 5.3 Trust 范围图

```text
授权范围从窄到宽

当前调用
  └─ exact command / exact file
       └─ command prefix / file subtree within workspace
            └─ project permission.allow rule
                 └─ 仅在用户明确选择时写入项目配置

session trust:  session_approvals[session-A]
                ├─ 可复用给 session-A 的后续调用
                └─ 不可泄漏给 session-B

project trust:  <project>/.rozsa/settings.json
                ├─ 同一项目的新 session 可读取
                └─ 不能突破 deny 或 workspace boundary
```

讨论权限问题时，至少同时说清：`session_id`、tool、目标（命令/文件）、裁定（allow/block/ask）和 trust scope。只说“权限弹窗不对”信息不够。

代码锚点：[`permissions/mod.rs`](../../crates/rozsa-app/src/permissions/mod.rs)、[`app.js` permission flow](../../crates/rozsa-gui/frontend/app.js)、[`commands.rs` permission commands](../../crates/rozsa-gui/src/commands.rs)。

### 5.4 Agent question 与 Permission 的边界

`askUserQuestion` 的 question panel 是 agent 主动请求用户提供上下文，不是授权操作本身。tool 调用仍先经过 `PermissionController`，默认命中 `allowed_tools` 白名单而直接得到 `Allow`；显式 `deny` / `ask` 规则仍按 permission 语义处理。question 结果只返回给当前 tool call，不写入 session trust，也没有允许 agent 关闭 `Other` 的配置项。

代码锚点：[`ask_user_question.rs`](../../crates/rozsa-app/src/tools/ask_user_question.rs)、[`state.rs` question state](../../crates/rozsa-gui/src/state.rs)、[`app.js` question flow](../../crates/rozsa-gui/frontend/app.js)。

## 6. 输入框、Caret、IME 与 Token

当前输入框是 `contenteditable` 的 `#msgInput`，不是 `textarea`。这一区分会直接影响 DOM 更新、selection 恢复和 IME 行为。

| 术语 | 当前含义 |
| --- | --- |
| contenteditable input | 浏览器 DOM 输入节点，承载用户可见文本和 token 高亮 |
| caret | 光标位置；当前按文本 offset 计算 |
| selection | 选区的 start/end offset；切 session 时随 draft 保存 |
| caret fidelity | 重绘高亮、autocomplete 或恢复 draft 后，光标仍在原位置 |
| IME composition | 中文/日文等输入法的未提交组合态；`compositionstart/update/end` 期间不能随意替换输入 DOM |
| input highlight | 对合法 slash/skill/file reference token 的轻量标记；不是独立 overlay 输入框 |
| slash token | `/...` 命令候选匹配单元；匹配依据必须围绕 cursor 当前 token |
| skill token | `/skill:name` 或可扩展 skill 命令；展开后可保留 display text 与模型实际文本的区别 |
| file reference token | `@path` 或 `@"path with spaces"`，用于把文件/目录引用附加到消息 |
| autocomplete | 根据当前文本和 cursor 请求候选；composition 期间暂停或丢弃过期请求 |

```text
用户输入
  │
  ├─ compositionstart/update -> 只维护组合态和尺寸，不重建高亮 DOM
  │
  └─ compositionend
       ├─ 读取 text + selection/caret offset
       ├─ autocomplete_input(text, cursor)
       ├─ 生成 highlight ranges / candidates
       └─ 重建 token span 后恢复同一 selection
```

沟通时说“输入框 bug”还不够。应指出是 `text extraction`、`caret/selection`、`IME composition`、`token parser`、`autocomplete` 还是 `auto-resize`。

代码锚点：[`index.html` input](../../crates/rozsa-gui/frontend/index.html)、[`app.js` input flow](../../crates/rozsa-gui/frontend/app.js)、[`slash_commands.rs`](../../crates/rozsa-app/src/slash_commands.rs)、[`commands.rs` token handling](../../crates/rozsa-gui/src/commands.rs)。

## 7. Window Chrome、Titlebar、Sidebar 与 Fullscreen

```text
NSWindow（系统拥有窗口语义）
├─ traffic lights：关闭 / 最小化 / 缩放
├─ native titlebar
│  ├─ TitlebarDragView：空白区域拖动窗口
│  ├─ double click：performZoom，触发 macOS 标题栏缩放语义
│  └─ sidebar accessory：调用 NSSplitViewController.toggleSidebar
└─ NativeSplitHost
   ├─ sidebar NSSplitViewItem：AppKit 管理 divider/collapse/overlay
   │  └─ sidebar WebView：MainSidebar | SettingsSidebar
   └─ main NSSplitViewItem
      └─ main WebView：MainContent | SettingsContent
```

| 术语 | 当前含义 | 讨论重点 |
| --- | --- | --- |
| window chrome | 系统窗口装饰和窗口行为 | 关闭、最小化、缩放、全屏、拖动 |
| native titlebar | AppKit `NSWindow` 的原生标题栏语义，加上必要的 accessory/drag view | 不是 WebView 里画的一条 header |
| `TitlebarDragView` | 原生拖动视图；同时处理 sidebar action 和双击 zoom | 事件命中范围必须避开 traffic lights 和可交互控件 |
| traffic lights | macOS 左上角系统按钮 | 不承担品牌或导航 |
| native pane | `NSSplitViewItem` 管理的 sidebar 或 main 区域 | pane frame 不由 CSS/Tauri bounds API 管理 |
| sidebar scene | 同一 sidebar WebView 内的 `MainSidebar` 或 `SettingsSidebar` | scene 变化不创建第二个 sidebar 容器 |
| main content scene | 同一 main WebView 内的 `MainContent` 或 `SettingsContent` | stateful Main roots 不重建 |
| sidebar collapse | AppKit 隐藏 sidebar item；恢复后仍是同一 WebView | 不等同于删除 session list |
| translucent sidebar | `NativeSplitHost` 的 `NSVisualEffectView` sidebar material 与主题设置 | sidebar WebView 自身保持透明 |
| opaque sidebar backing | `translucentSidebar=false` 时由原生 host 提供的主题色背景 | 更新发生在同 revision WebView theme event 之前 |
| native fullscreen | macOS 全屏状态；AppKit 管理 pane 与 overlay，frontend 只处理内容可见性 | 不用 JS 重算 pane frame |
| double-click zoom | 双击标题栏空白区域触发 `NSWindow.performZoom` | 是窗口语义，不是浏览器缩放 |

### 窗口问题的定位顺序

```text
按钮/文字/布局错       -> app.js / index.html / CSS
divider / collapse 错   -> native_split_view.rs / NSSplitViewController
拖动或双击缩放错       -> native_titlebar.rs / NSWindow event routing
全屏进出错             -> NativeSplitHost + native titlebar observer
sidebar material 错     -> native_split_view.rs + revisioned theme-state
```

代码锚点：[`native_split_view.rs`](../../crates/rozsa-gui/src/native_split_view.rs)、[`native_titlebar.rs`](../../crates/rozsa-gui/src/native_titlebar.rs)、[`scene_router.rs`](../../crates/rozsa-gui/src/scene_router.rs)、[`gui_shared.js`](../../crates/rozsa-gui/frontend/gui_shared.js)、[`themes.md`](./themes.md)。

## 8. Appearance、Theme 与 Visual State

| 术语 | 当前含义 |
| --- | --- |
| Appearance | Settings 中的显示设置场景，不是整个 GUI 的视觉代称 |
| theme mode | `system`、`light`、`dark`；`system` 跟随系统色彩偏好 |
| theme profile | Light 或 Dark 下当前选中的主题定义 |
| theme field | Accent、Background、Foreground、UI font、Code font、Translucent sidebar 等可编辑字段 |
| custom theme | 写入 `~/.rozsa/themes/<theme_id>.json` 的主题副本 |
| CSS variable | 主题 `variables` 中传给 frontend CSS 根节点的扩展变量 |
| semantic state | running、success、error、approval、idle 等行为状态；不能只靠颜色表达 |
| visual state | 同一 runtime 状态在 UI 上的呈现，例如 `sessionStreamingState` 的 running/approval/idle |

主题设置影响颜色、字体和 sidebar material；它不改变 permission policy、session ownership 或 agent loop。

## 9. 推荐用语

| 推荐说法 | 具体指向 | 避免的模糊说法 |
| --- | --- | --- |
| “frontend 的 caret restore” | `app.js` 重建 DOM 后恢复 selection | “输入框自己跳了” |
| “GUI runtime 的 session ownership” | `GuiState`、`SessionTab`、`SharedResources`、factory | “后端 session 有点乱” |
| “session-scoped UI memory” | draft、permission UI、scroll、展开状态的 session map | “session 已持久化” |
| “runtime permission verdict” | `PermissionController.evaluate()` 返回的裁定 | “权限 UI 状态” |
| “project trust / session trust” | 写入 project allow 规则 / 当前 session memory | “记住权限” |
| “turn activity” | 当前用户 turn 的 file delta 和 verification 聚合 | “workspace diff” |
| “native titlebar event routing” | AppKit drag、double-click、traffic lights 的命中与转发 | “顶部栏点击有问题” |
| “native split divider position” | AppKit 保存和恢复 sidebar divider | “左边宽度不对” |

## 10. 代码索引

| 领域 | 主要入口 |
| --- | --- |
| GUI 启动与 session factory | [`crates/rozsa-gui/src/lib.rs`](../../crates/rozsa-gui/src/lib.rs)、[`state.rs`](../../crates/rozsa-gui/src/state.rs) |
| IPC commands | [`crates/rozsa-gui/src/commands.rs`](../../crates/rozsa-gui/src/commands.rs) |
| session event forwarding | [`crates/rozsa-gui/src/events.rs`](../../crates/rozsa-gui/src/events.rs) |
| frontend state/input/rendering | [`crates/rozsa-gui/frontend/app.js`](../../crates/rozsa-gui/frontend/app.js)、[`index.html`](../../crates/rozsa-gui/frontend/index.html) |
| sidebar scene/rendering | [`sidebar.js`](../../crates/rozsa-gui/frontend/sidebar.js)、[`sidebar.html`](../../crates/rozsa-gui/frontend/sidebar.html) |
| scene revision / shared frontend | [`scene_router.rs`](../../crates/rozsa-gui/src/scene_router.rs)、[`gui_shared.js`](../../crates/rozsa-gui/frontend/gui_shared.js) |
| live agent loop and abort | [`crates/rozsa-app/src/agent_session.rs`](../../crates/rozsa-app/src/agent_session.rs) |
| askUserQuestion tool | [`crates/rozsa-app/src/tools/ask_user_question.rs`](../../crates/rozsa-app/src/tools/ask_user_question.rs)、[`crates/rozsa-gui/src/events.rs`](../../crates/rozsa-gui/src/events.rs) |
| permission runtime | [`crates/rozsa-app/src/permissions/mod.rs`](../../crates/rozsa-app/src/permissions/mod.rs) |
| tool file delta | [`crates/rozsa-app/src/tools/file_delta.rs`](../../crates/rozsa-app/src/tools/file_delta.rs) |
| turn summary / verification | [`crates/rozsa-gui/src/turn_diff.rs`](../../crates/rozsa-gui/src/turn_diff.rs) |
| workspace Git diff | [`crates/rozsa-gui/src/git_diff.rs`](../../crates/rozsa-gui/src/git_diff.rs) |
| macOS titlebar | [`crates/rozsa-gui/src/native_titlebar.rs`](../../crates/rozsa-gui/src/native_titlebar.rs) |
| macOS split/sidebar backing | [`crates/rozsa-gui/src/native_split_view.rs`](../../crates/rozsa-gui/src/native_split_view.rs) |
| Appearance / theme behavior | [`docs/gui/themes.md`](./themes.md) |

相关规范：[`GUI 架构`](./ARCHITECTURE.md)、[`GUI 使用规范`](./UI_USAGE_GUIDELINES.md)。
