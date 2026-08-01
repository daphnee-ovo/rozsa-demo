# Rózsa 前端页面术语表

这份文档约定 WebView 前端中的页面区域、组件、控件、动态内容和交互状态怎么称呼。截图只是场景示例，不是术语范围；未出现在截图中的条件渲染面板、消息内容和 Settings 控件也必须按当前实现命名。main 前端以 [`index.html`](../../crates/rozsa-gui/frontend/index.html) 和 [`app.js`](../../crates/rozsa-gui/frontend/app.js) 为准；sidebar 前端以 [`sidebar.html`](../../crates/rozsa-gui/frontend/sidebar.html) 和 [`sidebar.js`](../../crates/rozsa-gui/frontend/sidebar.js) 为准；共享 revision 规则在 [`gui_shared.js`](../../crates/rozsa-gui/frontend/gui_shared.js)。session ownership、permission runtime、AgentSession 等运行时概念见 [`TERMINOLOGY.md`](./TERMINOLOGY.md)。

截图对应当前主界面：左侧是 `sidebar`，右侧是 `main panel`，底部整块是 `composer`。其中最底部的一排控件才叫 `input toolbar`；`tool call` 是 agent 执行工具的概念，不能和 `toolbar` 混用。动态内容出现在哪个场景，不改变它的术语。

## 1. 页面总览

```text
┌──────────────────────────────────────────────────────────────────────┐
│ macOS window chrome: traffic lights + native sidebar toggle          │
├──────────────────────┬───────────────────────────────────────────────┤
│ SIDEBAR              │ MAIN PANEL                                    │
│                      │                                               │
│ sidebar header       │ panel header                                  │
│   Sessions   [+]     │   current session title                       │
│                      │                                               │
│ session list         │ chat region                                   │
│   session item       │   └─ empty state / message stream              │
│   session item       │                                               │
│                      │                                               │
│ status section       │                                               │
│   git status row     │                                               │
│   quota meter        │                                               │
│   tool chips         │                                               │
│                      │                                               │
│ sidebar footer       │ composer host                                 │
│   Settings           │ ┌───────────────────────────────────────────┐ │
│                      │ │ composer surface                          │ │
│                      │ │ message editor                            │ │
│                      │ │ input toolbar                              │ │
│                      │ │ [attach] [slash] hint context model think Send│ │
│                      │ └───────────────────────────────────────────┘ │
└──────────────────────┴───────────────────────────────────────────────┘
```

macOS 页面层级。sidebar 与 main 是两个持久 WebView，scene 切换只改变预创建 root 的 visibility：

```text
NativeSplitHost
├─ sidebar WebView                               sidebar.html + sidebar.js
│  ├─ MainSidebar                                #mainSidebarScene
│  │  ├─ session list                            #sidebarSessionList
│  │  ├─ status panel
│  │  └─ settings action
│  └─ SettingsSidebar                            #settingsSidebarScene
│     └─ settings navigation                     [data-settings-pane]
└─ main WebView                                  index.html + app.js
   ├─ MainContent                                #mainContentScene
   │  ├─ panel header                            [data-od-id="panel-header"]
   │  ├─ chat region                             #chatMessages
   │  └─ composer host                           [data-od-id="chat-input"]
   └─ SettingsContent                            #settingsPanel
      └─ current settings pane                   #pane-*
```

非 macOS fallback 仍由 `index.html` 中的 templates materialize sidebar 和 settings navigation，并使用 CSS grid；这不是 macOS 的 pane owner。

## 2. 区域、组件、控件的边界

| 层级 | 英文约定 | 中文约定 | 判断标准 |
| --- | --- | --- | --- |
| 页面区域 | `page region` | 页面区域 | 承担布局职责，例如 `sidebar`、`main panel`、`chat region` |
| 结构外壳 | `shell` / `page shell` | 页面外壳 | 组织多个区域，例如 `app body` |
| 组件 | `component` | 组件 | 有独立职责和状态，例如 `session item`、`empty state`、`permission panel` |
| 控件 | `control` | 控件 | 用户直接操作的 button、select、input 或 contenteditable |
| 表面容器 | `surface` | 表面容器 | 视觉上的背景、边界和圆角，不一定有独立业务状态 |
| 面板 | `panel` | 面板 | 承载一组内容或操作，通常有自己的显示/隐藏或展开状态 |
| 卡片 | `card` | 卡片 | 相对独立的信息单元；不要把整个 composer 或 main panel 叫 card |
| 工具栏 | `toolbar` | 工具栏 | 一排按功能分组的操作或辅助控件；英文写一个词，不写 `tool bar` |
| 操作组 | `action group` | 操作组 | toolbar 内职责相近的一组按钮，例如附件和 slash action |

### `toolbar`、`tool call`、`composer` 的区别

```text
composer = 用户组织并提交消息的整个底部区域
├─ message editor
└─ input toolbar = composer 内的一排辅助控件
   ├─ input action group: attach / slash
   ├─ shortcut hint
   ├─ context meter
   ├─ model selector
   ├─ thinking effort indicator
   └─ submit control: Send

tool call = agent 请求执行 Read / Edit / Write / Bash 的运行时动作
```

推荐说法：

- “composer 的高度和底部间距不对。”
- “input toolbar 左侧的 input action group 溢出了。”
- “model selector 和 send button 在窄窗口下需要收缩。”
- “这个是 tool call 展示问题，不是 toolbar 问题。”

## 3. Window chrome 与页面顶部

| 术语 | 含义 | 当前实现/截图对应物 |
| --- | --- | --- |
| `window chrome` | 系统窗口装饰和窗口级行为 | traffic lights、原生标题栏、全屏和窗口缩放 |
| `traffic lights` | macOS 左上角关闭、最小化、缩放按钮 | 原生窗口提供，不属于 WebView 页面 |
| `native titlebar` | AppKit 提供的标题栏语义 | 当前由 `native_titlebar.rs` 接入 |
| `native sidebar toggle` | 原生标题栏里的 sidebar 显示/隐藏按钮 | 截图左上角 traffic lights 右侧的 split icon |
| `drag region` | 可拖动窗口的空白区域 | `TitlebarDragView` 的空白区域 |
| `native pane` | AppKit `NSSplitViewItem` 管理的 sidebar/main 区域 | divider、collapse、width 不由 CSS 管理 |
| `page header` / `panel header` | WebView 页面内部的内容标题区 | 当前主区的 `panel-header`，不是 native titlebar |

不要说“顶部 toolbar”来指截图左上角的原生标题栏。要说 `native titlebar`；如果指主内容里的当前 session title，则说 `panel header`。

## 4. Sidebar 术语

| 术语 | 中文 | 当前锚点/说明 |
| --- | --- | --- |
| `sidebar` | 侧栏 | 原生 sidebar pane 内的 persistent sidebar WebView |
| `sidebar header` | 侧栏标题区 | `Sessions` 与 `New session` action |
| `session section` | 会话区 | sidebar 中展示 session list 的上半区 |
| `session list` | 会话列表 | `#sidebarSessionList`，动态渲染多个 session item |
| `session item` | 会话项 | `.session-item`，包含状态点、名称和时间 |
| `session status indicator` | 会话状态指示点 | idle、running、approval 等状态的视觉入口 |
| `session name` | 会话名称 | `.session-name` |
| `session meta` | 会话元信息 | `.session-meta`，例如最近时间 |
| `new session action` | 新建会话操作 | sidebar header 右侧的 `+` button |
| `status section` | 状态区 | `#mainSidebarScene` 内的 Status section |
| `status panel` | 状态面板 | `.status-panel`，包住 Git、限额和工具摘要 |
| `status group` | 状态分组 | `.status-group`，例如 quota 或 tool count |
| `git status row` | Git 状态行 | 当前分支、增删行数、文件数 |
| `quota meter` | 限额计量器 | `5 hours` / `This week` 标签和进度条 |
| `tool chips` | 工具计数标签 | Bash、Read、Edit 等工具名与调用次数 |
| `sidebar footer` | 侧栏底部操作区 | `.sidebar-bottom` |
| `settings action` | 设置入口 | `openSidebarSettings()`，不要叫 settings toolbar |
| `Dev Flow status summary` | 开放项摘要 | `#sidebarDevFlowGroup` 中的 `3 Tasks · 1 Issue`；不另加 Dev Flow 标题 |
| `claimed row` | 执行中条目 | 带状态点、ID 与截断 title 的 task/issue 行 |
| `more action` | 更多条目入口 | sidebar 空间不足时显示 `more <count>`，打开只读 detail surface |
| `Dashboard action` | Dashboard 入口 | sidebar 底部、Settings 上方的网页 dashboard 操作 |
| `Dev Flow detail surface` | 只读详情浮层 | hover delay 临时显示，click 固定；不提供 mutation controls |
| `collapsed sidebar` | 折叠侧栏 | AppKit 折叠 sidebar item，WebView identity 不变 |

`sidebar` 是一个页面区域；`session list` 是其中的内容组件；`session item` 是列表项；`New session` 和 `Settings` 是控件/操作，不是新的 sidebar。

## 5. Main panel 术语

| 术语 | 中文 | 当前锚点/说明 |
| --- | --- | --- |
| `main panel` | 主面板 | `[data-od-id="main-panel"]`，承载当前 session 的主要工作区 |
| `panel header` | 主面板头部 | `[data-od-id="panel-header"]` |
| `session title` | 当前会话标题 | `#currentSessionName`；优先手动名称、自动生成名称、首条用户消息 preview，选中空 session 时为 `Untitled` |
| `header spacer` | 头部弹性占位 | `.header-spacer`，用于把可选控件推到右侧 |
| `chat region` | 聊天内容区 | `#chatMessages`，可滚动显示 empty state、消息和 tool call |
| `message stream` | 消息流 | chat region 中按事件增量更新的消息内容 |
| `empty state` | 空状态 | `#emptyState`，没有消息时的图标、标题、提示和快捷键 |
| `empty-state hint` | 空状态提示 | “Describe your coding task…” 及快捷键说明 |
| `keyboard hint` | 键盘提示 | `<kbd>` 视觉元素和对应文字，不等同于 input toolbar |

只有未选中任何 session 时，`#currentSessionName` 才显示产品名 `Rózsa`。session name 更新由后端同时推送 main `ui-state` 与 `sidebar-state`。

## 6. Composer 与 Input toolbar 术语

### 6.1 三层命名

| 层级 | 推荐英文名 | 中文 | 当前锚点 |
| --- | --- | --- | --- |
| 区域 | `composer host` / `input area` | 消息输入区 | `[data-od-id="chat-input"]` |
| 可见容器 | `composer surface` | 输入表面/输入容器 | `.input-wrapper` |
| 编辑器 | `message editor` / `rich input` | 消息编辑器 | `#msgInput.rich-input`，`contenteditable` |
| 子区域 | `input toolbar` | 输入工具栏 | `.input-toolbar` |
| 操作组 | `input action group` | 输入操作组 | `.input-tool-btn` 的左侧按钮组 |
| 控件 | `attachment action` | 附件操作 | `#attachFileButton`、可选目录按钮 |
| 控件 | `slash action` | Slash 命令操作 | title 为 `Slash commands` 的按钮 |
| 辅助文本 | `shortcut hint` | 快捷键提示 | `.input-hint` |
| 指示器 | `context meter` | 上下文用量指示器 | `.context-ring` + `#contextTokens` |
| 控件 | `model selector` | 模型选择器 | `#modelSelector.model-selector` |
| 指示器 | `thinking effort indicator` | 思考强度指示器 | `#thinkingEffort`，紧跟 model selector |
| 控件 | `submit control` / `send button` | 提交控件/发送按钮 | `.send-btn`，文字为 `Send` |

### 6.2 Composer 结构图

```text
composer host [data-od-id="chat-input"]
├─ autocomplete popup                    #autocomplete
├─ running panels                        #subagentPanel / #forkPicker / queue
└─ composer surface                      .input-wrapper
   ├─ permission panel                   #permPanel（出现时可替代编辑器）
   ├─ message editor                     #msgInput
   └─ input toolbar                      .input-toolbar
      ├─ input action group
      │  ├─ attachment action            #attachFileButton
      │  ├─ directory action             #attachDirectoryButton（可选）
      │  └─ slash action
      ├─ flex spacer                     .input-spacer
      ├─ shortcut hint                   .input-hint
      ├─ context meter                   .context-ring
      ├─ model selector                  #modelSelector
      └─ submit control                  .send-btn
```

当前 `input toolbar` 是 `.input-toolbar` 这一行，不包括 `#msgInput` 本身。讨论尺寸、换行和 IME 时说 `message editor`；讨论附件、Slash、上下文、模型和发送按钮横向排列时说 `input toolbar`。

### 6.3 不推荐的叫法

| 不推荐 | 问题 | 推荐 |
| --- | --- | --- |
| `tool bar` | `tool` 容易被理解成 agent tool | `toolbar` 或 `input toolbar` |
| “底部 toolbar” | 不清楚是整块输入区还是底部控件行 | `composer` / `input toolbar` |
| “输入框按钮” | 没有区分附件、Slash、模型和发送 | `attachment action`、`slash action`、`model selector`、`send button` |
| “右边那个圆圈” | 无法区分上下文用量和状态指示 | `context meter` / `context ring` |
| “模型按钮” | 可能被误解为操作按钮而非选择器 | `model selector` |
| “消息卡片” | 可能指 empty state、message 或 composer | `message editor`、`empty state` 或 `composer surface` |

## 7. Settings 页面术语

设置不是另一个 sidebar 容器。Settings scene 复用同一 native split：sidebar WebView 显示 `SettingsSidebar`，main WebView 显示 `SettingsContent`。

```text
settings scene
├─ sidebar WebView: SettingsSidebar    #settingsSidebarScene
│  ├─ back action
│  └─ settings navigation              [data-settings-pane]
└─ main WebView: SettingsContent       #settingsPanel
   └─ settings content                 .settings-content
      └─ settings pane                 .settings-pane
         ├─ pane title
         ├─ settings group             .settings-group
         └─ setting item               .setting-item
```

| 术语 | 中文 | 不要混称为 |
| --- | --- | --- |
| `settings scene` | 设置场景 | modal toolbar |
| `settings navigation` | 设置导航 | settings sidebar button group |
| `settings tab` | 设置页签 | toolbar item |
| `settings pane` | 当前设置页 | main panel |
| `settings group` | 设置分组 | card，除非视觉上确实是独立卡片 |
| `setting item` | 单行设置项 | generic row |
| `setting control` | select、input、switch 等具体控件 | setting item 本身 |

## 8. 沟通例句

- “这是 `sidebar` 与 `main panel` 的 boundary 问题，不是 window chrome 问题。”
- “`session item` 的 active state 不应改变 `session list` 的行高。”
- “`composer surface` 要保持 bottom-anchored；内部 `message editor` 可以自动增高。”
- “`input toolbar` 左侧的 `input action group` 在窄窗口下收缩，`shortcut hint` 可以隐藏。”
- “`context meter` 和 `model selector` 属于 toolbar 的 utility controls，不属于 attachment actions。”
- “permission panel 出现时覆盖 composer surface 的输入态；这是 composer state，不是新的 main panel。”
- “settings scene 与 main scene 复用同一 sidebar WebView，只切换 `SettingsSidebar` / `MainSidebar` root。”

## 9. 当前实现索引

| 页面部分 | 代码入口 |
| --- | --- |
| sidebar WebView scene roots | [`sidebar.html`](../../crates/rozsa-gui/frontend/sidebar.html)、[`sidebar.js`](../../crates/rozsa-gui/frontend/sidebar.js) |
| session list、status section、settings action | [`sidebar.html`](../../crates/rozsa-gui/frontend/sidebar.html) |
| main WebView scene roots | [`index.html`](../../crates/rozsa-gui/frontend/index.html)、`renderNativeMainScene()` |
| panel header、chat region、empty state | [`index.html:2483`](../../crates/rozsa-gui/frontend/index.html:2483) |
| composer、message editor、input toolbar | [`index.html:2502`](../../crates/rozsa-gui/frontend/index.html:2502) |
| settings content scene | [`index.html`](../../crates/rozsa-gui/frontend/index.html) 的 `#settingsPanel` |
| session list rendering | [`sidebar.js`](../../crates/rozsa-gui/frontend/sidebar.js) 的 `renderSidebarSessions()` |
| composer input、autocomplete、IME | [`app.js`](../../crates/rozsa-gui/frontend/app.js) 的 `handleInput()`、`updateAutocomplete()`、`handleComposition*()` |
| scene/theme revision | [`gui_shared.js`](../../crates/rozsa-gui/frontend/gui_shared.js) 的 `applySceneSnapshot()`、`applyThemeSnapshot()` |

## 10. 消息流与内容渲染

消息区不能只按“用户气泡”和“助手气泡”理解。一个 assistant message 可能同时包含 thinking、tool call、Markdown 正文和 turn summary；这些是不同的前端组件。

```text
chat region                                      #chatMessages
└─ message stream
   ├─ user message                               .msg.msg-user
   │  ├─ avatar                                  .msg-avatar
   │  ├─ role label                              .msg-role
   │  └─ message body                            .msg-body
   │     └─ Markdown body                        .markdown-body
   └─ assistant message                          .msg.msg-assistant
      ├─ avatar + role label                     .msg-avatar + .msg-role
      ├─ thinking block                          .thinking-block
      │  ├─ thinking header                      .thinking-header
      │  └─ thinking content                     .thinking-content
      ├─ tool call                               .tool-call
      │  ├─ tool header                          .tool-header
      │  └─ tool call body                       .tool-call-body
      │     ├─ tool output                       .tool-output
      │     ├─ code view                         .code-view
      │     └─ diff view                         .diff-view
      ├─ assistant message body                  .msg-content
      │  └─ Markdown / code / table / image
      └─ turn summary                            .changes-card
         ├─ changed file row                     .change-entry
         └─ verification result                  .changes-footer
```

| 推荐英文名 | 中文约定 | 职责 | 当前 DOM/CSS/JS 锚点 |
| --- | --- | --- | --- |
| `message stream` | 消息流 | 按时间顺序承载当前 session 的消息项 | `#chatMessages`；`renderMessages()` |
| `user message` | 用户消息 | role 为 `user` 的消息项 | `.msg.msg-user`；`renderMessage()` |
| `assistant message` | 助手消息 | role 为 `assistant` 的消息项，可包含多个内容块 | `.msg.msg-assistant`；`renderMessage()` |
| `tool result message` | 工具结果消息 | 工具执行返回的结果展示；通常以内嵌 tool call 形式出现 | `role === 'toolResult'`；`renderMessage()` |
| `avatar` | 头像/角色标识 | 标识消息发送方；当前是字母方块，不要叫 status icon | `.msg-avatar` |
| `role label` | 角色标签 | 显示 `You`、`Rozsa` 等角色名 | `.msg-role` |
| `message body` | 消息主体容器 | 包住角色信息和消息内容，不等于正文文本 | `.msg-body` |
| `message content` | 消息内容 | 一条消息的可见正文或错误正文 | `.msg-content`；`extractText()` |
| `Markdown body` | Markdown 正文 | Markdown 渲染后的正文容器 | `.markdown-body`；`renderMarkdown()` |
| `inline code` | 行内代码 | Markdown 正文中的短代码片段 | `.msg-content p code` 等选择器；`inlineMd()` |
| `code block` | 代码块 | Markdown 三反引号代码的展示单元 | `.md-code-block`、`.md-code-head`、`.md-code-lang`；`codeBlock()` |
| `copy action` | 复制操作 | 将代码块或消息文本复制到剪贴板 | `.md-copy`；`copyCode()`、`copyText()` |
| `table` | 表格 | Markdown 表格渲染结果 | `.md-table-wrap`、`.md-table`；`renderTable()` |
| `task list item` | 任务列表项 | Markdown checkbox 风格的列表项 | `.task-list-item`；`renderMarkdown()` |
| `thinking block` | 思考块 | 展示 assistant 的 thinking 内容及其时长 | `.thinking-block`；`toggleThinking()` |
| `thinking header` | 思考块头部 | 展示 `THINKING`/`THINKED`、时长和折叠入口 | `.thinking-header`、`.thinking-label`、`.thinking-duration` |
| `thinking content` | 思考块正文 | thinking 的 Markdown 内容 | `.thinking-content`、`.thinking-markdown` |
| `stream cursor` | 流式光标 | 标记正在增量更新的文本尾部 | `.stream-cursor`、`[data-stream-cursor-target]`；`attachStreamCursor()` |
| `tool call` | 工具调用项 | 展示 agent 请求执行某个工具的完整项 | `.tool-call`；`renderMessage()` |
| `tool call row` | 工具调用行 | tool call 在消息流中的一行摘要入口 | `.tool-call`、`.tool-header`；`renderMessage()` |
| `tool header` | 工具调用头部 | 展示工具名、参数摘要、状态和展开入口 | `.tool-header`、`.tool-name`、`.tool-call-args`、`.tool-call-toggle` |
| `tool status` | 工具状态 | 表示工具正在运行、成功或失败 | `.tool-call-status.s-success` / `.s-error`；`renderMessage()` |
| `tool call body` | 工具调用正文 | 展开后展示参数、输出、代码或 diff | `.tool-call-body` |
| `tool output` | 工具输出 | 工具执行返回的文本或步骤摘要 | `.tool-output`、`.tool-output-steps`、`.tool-step` |
| `code view` | 代码视图 | 工具写入内容的带行号源码视图 | `.code-view`、`.code-line`、`.code-ln`、`.code-text`；`renderCodeView()` |
| `diff view` | 差异视图 | 展示新增、删除及行号的 patch 视图 | `.diff-view`、`.diff-line`、`.diff-add`、`.diff-del`；`renderDiffView()` |
| `file delta` | 文件变更数据 | 描述某个文件修改前后的结构化数据，不是 UI 本身 | `result.details.file_deltas`；`renderMessage()` |
| `turn summary` | 回合摘要 | 一轮 agent 工作后的文件变化和验证摘要 | `.changes-card`；`renderTurnActivityCard()` |
| `changed file row` | 变更文件行 | 摘要中展示单个文件及增删统计的行 | `.change-entry`、`.change-row`、`.change-name` |
| `inline turn diff` | 回合内联 diff | 在变更文件行下展开的 patch | `.turn-diff-inline`；`toggleTurnDiff()` |
| `verification result` | 验证结果 | 展示验证成功、失败、退出码和耗时 | `.changes-footer`、`Verified`、`Verification failed` |
| `verification runtime` | 验证运行信息 | 展示命令、exit code、timeout、truncated 和耗时 | `.changes-runtime` |
| `error message` | 错误消息 | 消息本身的错误正文，不要泛称为 toast | `.msg-error`；`renderMessage()` |

这些术语的边界：`tool call` 是消息内容中的执行项，`tool output` 是它的结果，`code view`/`diff view` 是结果的展示方式，`turn summary` 是一轮工作的汇总。不要把它们都叫“工具卡片”。

## 11. 截图之外的动态面板

下列组件在空会话截图中通常不可见，但它们是当前页面的真实组成部分。它们仍然属于 `composer host` 附近的前端交互，不是新的 main panel。

```text
composer host                                     [data-od-id="chat-input"]
├─ autocomplete popup                             #autocomplete
├─ running panels
│  ├─ subagent panel                              #subagentPanel
│  ├─ fork picker                                 #forkPicker
│  ├─ queue panel                                 #queuedMessages
│  └─ steering panel                              #steeringConversation
└─ composer surface                               .input-wrapper
   ├─ permission panel                            #permPanel
   │  ├─ permission context                       #permPanelContext
   │  ├─ permission actions                       #permPanelActions
   │  ├─ hint page                                #permPanelHint
   │  └─ trust page                               #permPanelTrust
   ├─ message editor                              #msgInput
   └─ input toolbar                               .input-toolbar
```

### 11.1 Permission panel

| 推荐英文名 | 中文约定 | 职责 | 当前 DOM/CSS/JS 锚点 |
| --- | --- | --- | --- |
| `permission panel` | 权限审批面板 | 在工具需要用户决定时替代 message editor 展示审批流程 | `#permPanel.perm-panel`；`displayPermPanelIfNeeded()` |
| `permission context` | 权限上下文 | 展示工具名、命令、描述和当前请求背景 | `#permPanelContext`、`#permTool`、`#permCmd`、`#permDesc` |
| `permission command` | 权限命令 | 用户需要批准或拒绝的命令/操作文本 | `#permCmd`；`renderPermissionCommand()` |
| `command disclosure` | 命令展开控件 | 展开或收起过长命令 | `#permCmdToggle`；`togglePermissionCommand()` |
| `permission actions` | 权限操作组 | 承载批准、拒绝、提示和信任等决策入口 | `#permPanelActions`、`.perm-panel-opt` |
| `permission action` | 权限操作 | 单个审批选项，不要叫 toolbar button | `.perm-panel-opt`、`.perm-panel-opt-key`、`.perm-panel-opt-label` |
| `permission hint page` | 权限补充说明页 | 用户选择拒绝并提供说明时的输入页面 | `#permPanelHint`、`#permHintInput`；`enterPermissionHint()` |
| `permission trust page` | 权限信任页 | 选择信任范围或 trust level 的页面 | `#permPanelTrust`；`renderPermissionTrustPage()` |
| `trust level` | 信任级别 | 决定此次或后续相似操作的授权范围 | `currentPermissionTrustGroups`；`choosePermissionTrust()` |
| `permission approval state` | 待审批状态 | 请求已经到达、尚未做出决定的状态 | `sessionStreamingState[id] = 'approval'`；`showPermission()` |

权限面板出现时应说“permission panel 替代了 composer 的 message editor”，不要说“弹出了一个新的聊天窗口”。`permission panel` 是页面组件，`permission runtime` 是运行时授权逻辑，两者需要区分。

### 11.2 Autocomplete、fork 和运行中面板

| 推荐英文名 | 中文约定 | 职责 | 当前 DOM/CSS/JS 锚点 |
| --- | --- | --- | --- |
| `autocomplete popup` | 自动补全弹层 | 根据当前 token 展示可选命令或模型 | `#autocomplete.autocomplete-popup`；`updateAutocomplete()` |
| `autocomplete item` | 自动补全项 | 单个可选命令、描述和快捷提示 | `.ac-item`、`.ac-cmd`、`.ac-desc`、`.ac-hint` |
| `selected autocomplete item` | 已选自动补全项 | 键盘导航当前指向的 item | `.ac-item.selected`；`navigateAutocomplete()` |
| `file reference` | 文件引用 | 输入中识别出的文件路径或文件 token | `formatFileReference()`；`updateInputHighlight()` |
| `valid token highlight` | 有效 token 高亮 | 对可解析的 slash/file token 做视觉标记 | `.valid-token-text`、`.input-wrapper.valid-token` |
| `running panel` | 运行中面板 | agent 运行期间展示排队、steering、fork 或 subagent 信息的面板类别 | `.running-messages` |
| `queue panel` | 排队消息面板 | 展示等待当前运行结束后发送的消息 | `#queuedMessages`；`renderRunningMessages()` |
| `queued message` | 排队消息 | 已提交但等待执行的消息 | `#queuedMessages li` |
| `steering panel` | Steering 对话面板 | 展示运行中发送、等待工具结果的 steering 消息 | `#steeringConversation`；`renderRunningMessages()` |
| `steering message` | Steering 消息 | 介入当前运行流程的消息 | `#steeringConversation li` |
| `running send mode` | 运行中发送模式 | 决定发送时进入 queue 还是 steer | `#runningSendMode`；`sendMessage()` |
| `fork picker` | Fork 选择器 | 选择历史消息作为新 session 起点 | `#forkPicker`；`showForkPicker()` |
| `fork point` | Fork 起点 | 可用于创建分叉会话的历史消息位置 | `get_fork_points` 返回项；`forkAtMessage()` |
| `subagent panel` | 子代理面板 | 展示 subagent 名称、状态、模型和消息数 | `#subagentPanel`；`showSubagentPanel()` |
| `subagent row` | 子代理行 | 单个 subagent 的信息行 | `#subagentPanel li`、`.subagent-meta` |

### 11.3 Notification center 与 unresolved errors

| 术语 | 中文 | 当前锚点/说明 |
| --- | --- | --- |
| `notification center` | 应用内通知区 | main WebView 右上角承载 toast stack |
| `notification toast` | 瞬时通知 | 每条独立计时 6 秒并向下堆叠；非必要 info/success 不显示 |
| `error toast` | 错误通知 | 到时后转入 unresolved error tray，不代表错误已解决 |
| `unresolved error tray` | 未解决错误入口 | 圆圈 `!` 与数量；hover 展开，click 固定 |

## 12. 前端交互状态

状态词描述的是同一个组件在某个时刻的 UI 状态，不是新的组件名称。沟通时应同时说组件和状态，例如“autocomplete item 进入 selected state”。

### 12.1 通用状态词

| 推荐英文状态 | 中文约定 | 适用含义 | 当前实现信号 |
| --- | --- | --- | --- |
| `idle` | 空闲 | 没有执行、等待用户操作 | `.session-status.idle`；`sessionStreamingState` |
| `hover` | 悬停 | 指针位于可交互元素上 | `:hover` 选择器 |
| `focus` | 聚焦 | 元素拥有键盘焦点 | `:focus`、`focus()` |
| `focus-visible` | 键盘聚焦 | 需要显式显示键盘焦点环 | `:focus-visible` |
| `active` | 当前/活动 | 当前 session、当前 settings tab 或正在进行的流 | `.active`；`renderSessionList()`、`renderThemeModeCards()` |
| `selected` | 已选中 | 列表、补全项或选项中的当前选择 | `.selected`；`acHighlight()` |
| `disabled` | 禁用 | 控件暂时不可操作 | `:disabled`；`.send-btn:disabled` |
| `visible` | 可见 | 组件正在显示 | `.visible`；`#permPanel`、`#autocomplete` |
| `hidden` | 隐藏 | 组件不参与当前可见布局 | `hidden` 属性；running panels、settings panes |
| `expanded` | 展开 | 正文或详情已经显示 | `.expanded`；tool call、thinking、permission command、turn diff |
| `collapsed` | 收起 | 正文或详情被折叠 | `.collapsed`；`#permCmd`、tool body |
| `running` | 执行中 | agent、session 或 tool 尚未结束 | `s-running`、streaming state；`renderMessage()` |
| `streaming` | 流式更新中 | assistant 内容正在增量到达 | `isStreaming`、`data-stream-cursor-target` |
| `partial output` | 部分输出 | 当前已渲染但尚未完成的 assistant 文本或 thinking | `.stream-cursor`；`patchStreamingThinking()` |
| `approval` | 待审批 | 等待用户处理权限请求 | `sessionStreamingState[id] = 'approval'` |
| `success` | 成功 | 操作或工具正常完成 | `.s-success`、`.change-add`、`Verified` |
| `error` | 失败/错误 | 操作或工具执行失败 | `.s-error`、`.msg-error`、`Verification failed` |
| `on` / `off` | 开启/关闭 | switch 或二值设置的值 | `.setting-toggle.on`、`aria-checked` |

`active` 和 `selected` 不要混用：`active` 更适合当前页面、当前 session 或当前运行；`selected` 更适合列表项或候选项。`visible/hidden` 描述显示状态；`expanded/collapsed` 描述组件内部详情是否展开。

### 12.2 Message editor 的输入状态

```text
message editor                                  #msgInput.rich-input
├─ plain input text
├─ caret / selection
├─ IME composition (preedit)
├─ slash token / file reference
│  ├─ valid token highlight                    .valid-token-text
│  └─ autocomplete popup                       #autocomplete
└─ focus state
   ├─ focus-within                              .input-wrapper:focus-within
   └─ valid-token                               .input-wrapper.valid-token
```

| 推荐英文名 | 中文约定 | 职责 | 当前 DOM/CSS/JS 锚点 |
| --- | --- | --- | --- |
| `message editor` | 消息编辑器 | 用户编辑待发送文本的 contenteditable | `#msgInput.rich-input`；`handleInput()` |
| `caret` | 插入光标 | 文本插入位置 | `getInputCursor()`、`setInputSelection()` |
| `selection` | 文本选区 | 当前选中的输入文本范围 | `getInputSelection()`、`setInputSelection()` |
| `IME composition` / `preedit` | 输入法组合态/预编辑文本 | 中文、日文等输入法尚未提交的中间文本 | `handleCompositionStart/Update/End()` |
| `slash token` | Slash token | 以 `/` 开始、可触发命令或动作的输入 token | `dispatchSlashCommand()`、`updateAutocomplete()` |
| `file reference` | 文件引用 | 输入中可识别的文件路径引用 | `formatFileReference()`、`updateInputHighlight()` |
| `input highlight` | 输入高亮 | 不改变实际文本的 token 视觉标记 | `.valid-token-text`；`updateInputHighlight()` |

处理输入问题时，说明是 `caret/selection`、`IME composition`、`autocomplete popup` 还是 `input highlight`，不要只说“输入框坏了”。

## 13. Settings 控件全量术语

Settings pane 的固定顺序是 Skills、Tools、Extensions、General、Appearance、Keyboard
shortcuts、Permissions、Dev Flow。Models 在 General 中；Permissions 与 Dev Flow 是独立 pane。

```text
settings scene
├─ sidebar WebView                                #settingsSidebarScene
│  ├─ back action                                 .settings-back
│  └─ settings navigation                         [data-settings-pane]
└─ main WebView                                   #settingsPanel
   └─ settings content                            .settings-content
      └─ settings pane                            .settings-pane
         ├─ pane title                            .settings-pane-title
         ├─ settings group                        .settings-group
         │  ├─ group label                        .settings-group-label
         │  └─ setting item                       .setting-item
         │     ├─ setting label                   .setting-label
         │     ├─ setting control                 select / input / switch
         │     └─ setting value                   .setting-value
         └─ theme action row                      .appearance-theme-actions
```

### 13.1 Settings 结构层级

| 推荐英文名 | 中文约定 | 职责 | 当前 DOM/CSS/JS 锚点 |
| --- | --- | --- | --- |
| `settings scene` | 设置场景 | 两个 WebView 同步显示 SettingsSidebar/SettingsContent | `gui-scene-snapshot`；`requestGuiScene()` |
| `settings dialog` | 设置对话框 | main WebView 内 SettingsContent 的无障碍 dialog | `.settings-dialog`；`role="dialog"` |
| `settings backdrop` | 设置背景层 | main WebView 的 SettingsContent 外围 | `#settingsPanel` |
| `settings workspace` | 设置工作区 | main WebView 内组织 settings content | `.settings-workspace` |
| `settings navigation` | 设置导航 | sidebar WebView 中的 pane 切换入口 | `#settingsSidebarScene [data-settings-pane]` |
| `back action` | 返回应用操作 | 请求切回 Main scene | `.settings-back`；`closeSidebarSettings()` |
| `settings tab` | 设置页签 | 按固定顺序切换 Skills、Tools、Extensions、General、Appearance、Keyboard shortcuts、Permissions、Dev Flow | `[data-settings-pane]`；`selectSidebarSettingsPane()` |
| `settings content` | 设置内容区 | 承载当前 pane 的滚动内容 | `.settings-content` |
| `settings pane` | 设置页面 | 一个完整的 Settings 分类页面 | `.settings-pane`、`#pane-*`；`renderSettingsPane()` |
| `pane title` | 页面标题 | 当前 pane 的标题 | `.settings-pane-title` |
| `settings group` | 设置分组 | 按 AI、Display、Network 等主题分组 | `.settings-group` |
| `group label` | 分组标签 | 标识一个 settings group 的主题 | `.settings-group-label` |
| `setting item` | 设置项 | 一行 label 与 value/control 的组合 | `.setting-item` |
| `setting label` | 设置项标签 | 解释具体 setting 的名称 | `.setting-label` |
| `setting control` | 设置控件 | select、input、range、color picker 或 switch | `.setting-select`、`.setting-input`、`.setting-toggle` |
| `setting value` | 设置值 | 只读的 provider、context 或 shortcut 值 | `.setting-value` |
| `close action` | 关闭操作 | 请求切回 Main scene | `.settings-close`；`closeSettings()` |

Skills、Tools 与 Permissions pane 中的 `capability scope` 是 Global/Project 子页；
`capability override` 有 inherit/enabled/disabled 三态。Extensions pane 只使用
`empty settings state` 说明功能预留。General 的 Models、Permissions、AI 和 Network
group 只展示已经接到 `update_setting` 的控件。

### 13.2 Appearance pane

| 推荐英文名 | 中文约定 | 职责 | 当前 DOM/CSS/JS 锚点 |
| --- | --- | --- | --- |
| `appearance pane` | 外观页面 | 调整主题、字号和字体 | `#pane-appearance`；`renderAppearanceSettings()` |
| `display group` | 显示分组 | 承载 Theme Mode 与 Font Size | `.settings-group` 下的 `Display` |
| `theme mode` | 主题模式 | `System`、`Light`、`Dark` 三种模式 | `#settingsThemeMode`；`selectThemeModeCard()` |
| `theme mode card` | 主题模式卡片 | 用预览图选择主题模式的控件 | `.theme-mode-card`、`data-theme-mode-card` |
| `theme preview` | 主题预览 | 卡片中的视觉预览缩略图 | `.theme-preview`、`.theme-preview-body` |
| `font size` | 字体大小 | 页面 UI 字号设置 | `#settingsFontSizeRange`、`#settingsFontSizeInput` |
| `settings help tip` | 设置帮助提示 | 在 Appearance 标签旁显示原型定义的 `?` 图标；悬停或键盘聚焦时展示说明 | `.settings-hint`、`data-tooltip` |
| `range slider` | 范围滑块 | 连续调整 font size 的 range 控件 | `.appearance-range`；`renderAppearanceSettings()` |
| `numeric input` | 数值输入框 | 直接输入字号数值的 number 控件 | `.appearance-font-size-input`、`#settingsFontSizeInput` |
| `theme section` | 主题配置区 | 承载 Light Theme 或 Dark Theme 的完整配置 | `.appearance-theme-section`、`#appearanceLightSection` / `#appearanceDarkSection` |
| `theme selector` | 主题选择器 | 从可用主题列表选择具体 theme definition | `#settingsLightTheme`、`#settingsDarkTheme`；`renderThemeSelect()` |
| `color picker` | 颜色选择器 | 通过原生 color input 选择颜色 | `.theme-color-picker`；`renderThemeControls()` |
| `HEX input` | HEX 颜色输入框 | 直接编辑 `#RRGGBB` 颜色值 | `.theme-hex-input`；`isHexColor()` |
| `color control` | 颜色控件组 | 把 color picker 和 HEX input 绑定成一个 setting control | `.theme-color-control`、`updateThemeColorVisual()` |
| `UI font input` | UI 字体输入框 | 配置界面字体 | `#lightThemeUiFont`、`#darkThemeUiFont` |
| `code font input` | 代码字体输入框 | 配置代码和等宽内容字体 | `#lightThemeCodeFont`、`#darkThemeCodeFont` |
| `translucent sidebar switch` | 半透明侧栏开关 | 配置当前主题是否使用半透明 sidebar | `#lightThemeTranslucentSidebar`、`#darkThemeTranslucentSidebar`；`role="switch"` |
| `theme action row` | 主题操作行 | 承载保存自定义主题的操作和说明 | `.appearance-theme-actions` |
| `save custom theme action` | 保存自定义主题操作 | 将当前主题保存为 custom theme | `.appearance-theme-actions button`；`saveThemeAsCustom()` |
| `theme note` | 主题说明 | 补充 custom theme 的存储位置说明 | `.appearance-theme-note` |

### 13.3 General、Skills、Tools、Extensions panes

| 推荐英文名 | 中文约定 | 职责 | 当前 DOM/CSS/JS 锚点 |
| --- | --- | --- | --- |
| `AI settings group` | AI 设置分组 | Thinking、compact、steering 和 follow-up 设置 | `#pane-general .settings-group`；`renderGeneralSettings()` |
| `thinking effort selector` | 思考强度选择器 | 选择 Off/Low/Medium/High/Xhigh/Max | `#settingsThinkingEffort` |
| `auto compact switch` | 自动压缩开关 | 控制上下文自动 compact | `#settingsAutoCompact`；`wireSettingSwitch()` |
| `auto session naming switch` | 自动会话命名开关 | 控制首次真实用户 turn 的并发命名；短输入本地直用，长输入调用 small model | `#settingsAutoSessionNaming`；`wireSettingSwitch()` |
| `steering mode selector` | Steering 模式选择器 | 选择一次处理一条或全部 steering 消息 | `#settingsSteeringMode` |
| `follow-up mode selector` | Follow-up 模式选择器 | 选择 follow-up 消息的处理方式 | `#settingsFollowUpMode` |
| `default running send mode selector` | 运行中默认发送模式选择器 | 选择 Queue 或 Steer | `#settingsRunningSendMode` |
| `block images switch` | 阻止图片开关 | 控制 Markdown 图片是否被阻止 | `#settingsBlockImages` |
| `network settings group` | 网络设置分组 | 承载传输方式配置 | `#pane-general` 的 `Network` group |
| `transport selector` | 传输方式选择器 | 选择 Auto、SSE 或 WebSocket | `#settingsTransport` |
| `models settings group` | 模型设置分组 | 查看和切换当前模型、provider、context window | `#pane-general` |
| `model setting selector` | 模型设置选择器 | 在 Settings 中切换模型 | `#settingsModelSelect`；`onModelChange()` |
| `small model selector` | 小模型选择器 | 为长输入的 session title 请求选择低成本模型；请求固定使用 Low reasoning，Disabled 时只使用本地短标题和 preview fallback | `#settingsSmallModelSelect`；`saveSetting('small_model', ...)` |
| `provider value` | Provider 值 | 展示当前模型提供方 | `#settingsProvider` |
| `context window value` | 上下文窗口值 | 展示当前模型的 context window | `#settingsContextWindow` |
| `permissions pane` | 权限页面 | 分 Global/Project 配置 permission mode 与 deny/ask/allow 规则 | `#pane-permissions`；`renderPermissionSettings()` |
| `permission mode selector` | 权限模式选择器 | 选择 auto-approve、on-request 或 yolo；auto-approve 尚未实现时显示错误且不保存 | `#settingsPermMode` |
| `permission rule row` | 权限规则行 | 单行展示 `ToolName(pattern)`、继承状态与删除操作；可在 deny/ask/allow 间 pointer 拖拽 | `.permission-rule-row`；`wirePermissionRulePointerDrag()` |
| `permission rule editor` | 权限规则添加器 | 在对应规则容器内部组合可输入/Tab 补全/下拉单选的 tool combobox、rich pattern input 与 RegExp switch | `#permissionRuleEditor`；`#permissionRuleEditorTemplate` |
| `permission rule pattern` | 权限规则 pattern | 输入 glob 或 RegExp，并高亮 `*`/正则元字符 | `#permissionRuleTarget.permission-rule-pattern`；`renderRichInputHighlights()` |
| `capability scope` | 能力配置层 | 切换 Global 或 Project | `.capability-scope` |
| `capability switch` | 能力开关 | 切换生效状态；同时显示 Default/Inherited 和恢复继承操作 | `.capability-row .setting-toggle` |
| `skills pane` | Skills 页面 | 展示分层发现的 skills | `#pane-skills`；`#settingsSkillList` |
| `tools pane` | Tools 页面 | 展示实际 registered tools | `#pane-tools`；`#settingsToolList` |
| `extensions pane` | Extensions 预留页 | 明确说明尚未实现 | `#pane-extensions` |

### 13.4 Keyboard shortcuts pane

| 推荐英文名 | 中文约定 | 职责 | 当前 DOM/CSS/JS 锚点 |
| --- | --- | --- | --- |
| `keyboard shortcuts pane` | 快捷键页面 | 查看实际支持的快捷键并编辑稳定动作 | `#pane-keyboard-shortcuts`；`renderKeyBindings()` |
| `shortcut search` | 快捷键搜索 | 按动作、说明或当前按键筛选 | `#shortcutSearch` |
| `customizable shortcuts group` | 可自定义快捷键分组 | 展示由 GUI 注册表驱动的当前绑定 | `#keyBindingList` |
| `shortcut row` | 快捷键行 | 展示动作、说明、当前绑定和重置入口 | `.shortcut-row` |
| `shortcut binding action` | 快捷键绑定操作 | 进入按键捕获并保存新绑定 | `.shortcut-binding`；`beginKeyBindingCapture()` |
| `context controls group` | 上下文控制分组 | 只读展示权限、问题和自动补全等固定按键 | `#fixedKeyBindingList` |

### 13.5 Dev Flow pane

| 术语 | 中文 | 当前锚点/说明 |
| --- | --- | --- |
| `Dev Flow pane` | Dev Flow 设置页 | 标题下显示版本、description、Overview 与 Settings；沿用普通 settings surface |
| `Dashboard Availability` | Dashboard 可用状态 | 只读 overview row，例如 Ready 或 Unavailable |
| `Dashboard address` | Dashboard 地址 | 当前项目只读服务地址 |
| `Memory Use` | 内存使用 | 当前 project dashboard runtime 的 MiB 值 |
| `Path field` | CLI 路径字段 | 唯一的 `dow` path 编辑位置，配套 Choose action |
| `Enable Dev Flow` | 集成总开关 | 控制已实现联动；system prompt 注入仍为 TODO |
| `sidebar status setting` | sidebar 状态开关 | 控制 status summary、claimed row 与 detail surface |
| `Dashboard button setting` | Dashboard 按钮开关 | 控制 sidebar Dashboard action |

## 14. 术语表的范围与使用规则

```text
截图场景
   └─ 说明视觉位置和一个具体 state

当前 frontend source of truth
   ├─ index.html: 静态结构、CSS class、ARIA role、hidden/visible 状态
   └─ app.js: 动态渲染、事件处理、状态切换和控件绑定

术语表
   └─ 为两者提供稳定的 English / 中文 / 职责 / 锚点映射
```

沟通时遵循四条规则：

- 先说页面区域或组件，再说状态，例如“permission panel 的 expanded state”。
- `toolbar` 始终写成一个词；`input toolbar` 只指 composer 内的控件行。
- `panel` 表示承载一组内容或流程的组件；`card` 只用于相对独立的信息单元；`action` 是动作入口，`control` 是可操作的 UI 控件。
- 讨论实现时优先给出 `data-od-id`、`id`、CSS class 或 JS function，避免只说“左边那个”“下面那块”。

如果截图中没有显示某个组件，仍然可以直接使用本表术语。例如：`#queuedMessages` 是 `queue panel`，`#permPanelTrust` 是 `permission trust page`，`#pane-tools` 是 `tools pane`；它们不需要先出现在截图里才算页面术语。

相关文档：[`GUI 运行时术语表`](./TERMINOLOGY.md)、[`GUI 使用规范`](./UI_USAGE_GUIDELINES.md)、[`Dev Flow GUI integration`](./DEV_FLOW_INTEGRATION.md)。
