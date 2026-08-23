# Scene 与 Gallery

## 统一入口

从仓库根目录启动静态服务器后访问 `prototype/index.html`。无参数打开 `empty-session`；使用 `?scene=<id>` 打开 application scene；使用 `?gallery=visual-direction` 打开视觉 gallery。ID 只在 `scenarios/registry.js` 登记，未知 ID 必须显示错误。

## Application scenes

| ID | 初始状态/内容 | 主要操作 |
| --- | --- | --- |
| `empty-session` | 空会话、settings closed、无 interruption | 发送消息、打开 thinking effort |
| `complete-session` | 完整会话、tool call、queue、steering | 展开 tool、切换 thinking、打开 fork/queue/steering |
| `tool-calls` | tool call 组合 | 展开 tool、切换 thinking |
| `permission-request` | permission interruption | allow、trust、输入 hint、cancel |
| `ask-user-question` | AskUserQuestion interruption | 选择问题、编辑 other、提交、取消 |
| `settings-appearance` | Appearance settings | 切换 settings tab、theme、font size |
| `permissions` | Permissions settings | 打开 rule editor、切换 permission setting |
| `notifications` | notification 与 interruption 组合 | dismiss、打开 error tray、cancel permission |
| `dev-flow-runtime` | Dev Flow runtime detail 与 notification | 打开/关闭 detail、打开 dashboard |
| `dev-flow-active` | active Dev Flow settings | 打开 dashboard、切换 Dev Flow setting |
| `dev-flow` | Dev Flow settings | 打开 dashboard、切换 Dev Flow setting |

每个 scene 只选择公共布局、组件、初始 state、mock ID 和操作步骤；数据文件集中在 `mock/`。
`dev-flow-runtime`、`dev-flow-active` 和 `dev-flow` 的 session 列表优先来自 `mock/dev-flow/runtime.json`，workspace 展示数据仍可来自 `mock/sessions/complete.json`；这样不会因为共享 workspace fixture 把当前 Dev Flow session 替换成空会话。

运行态面板保持与重构前相同的归属：`subagentPanel` 位于 sidebar，`forkPicker` 位于 input frame 前，`queuedMessages` 与 `steeringConversation` 位于 Composer 的 `.input-wrapper` 内并排在 `#msgInput` 前。shell 挂载 input primitive 时只插入其 children，使 Queue、Steering、`#msgInput` 和 toolbar 仍是同一 input wrapper 的直接内容；scene 只提供状态数据，不复制这些组件的稳定 DOM。

## Gallery

| ID | 内容 | 视图 |
| --- | --- | --- |
| `visual-direction` | Independent demo 的视觉研究与组件变体 | Empty、Active、Appearance |

Gallery 不加入 application shell 的 scene 清单，也不承担 runtime 产品状态。其入口、脚本、样式和检查 ID 在 registry 与 [UI_COVERAGE.json](./UI_COVERAGE.json) 中闭合记录。

## 新增场景的记录

新增 scene 需要同时更新 registry、mock、coverage、本文和可重复 baseline。操作步骤必须明确初始 focus、选中/展开状态、成功/失败/取消结果和窄视口行为；不能用页面能加载代替交互验证。
