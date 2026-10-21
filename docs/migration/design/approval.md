# Rózsa 权限与问题流程设计审阅

状态：已批准（2026-10-21）。批准版本为本目录下的 `approval.html` 与 `approval.md`。本设计未修改产品代码或原型；实际实现由 TASK-T039 负责，见文末范围。

## 语言约定

审阅页的标题、说明和状态导航使用中文。应用窗口内保留现有英文文案，与当前桌面界面和源码一致。

## 与实际界面对照

应用窗口按现有桌面结构与权限组件复刻：

- 保留原生窗口控制、左侧会话列表与状态卡片、设置入口、主区标题和输入框位置。
- 使用两个 “(no messages)” 会话、run-HdHEYs 状态卡片和 HOW_PERMISSION_DEMO 内容。
- 权限面板位于主区输入框位置。面板不显示 `Permission Request` 标题，也不显示风险标签；摘要内容按文件、命令或工具调用参数变化。
- 保留半透明面板、圆角、最大宽度和背景模糊；不增加侧栏、新卡片标题或中间确认步骤。
- 窄窗口通过侧栏抽屉展示，不改为另一套布局。
- 问题面板使用进度标题、编号选项和自定义回答输入框。
- `ArrowLeft`、`X` 和 `Check` 的预览图形取自项目 `lucide-react` 组件的静态渲染结果；产品实现直接使用 Lucide 组件。

应用预览复刻现有界面布局；其中信任路径文字为指定的目标值，不代表当前后端已生成这些值。

## 权限面板

### 高度与间距

- 审批面板高度随内容增长，上限为主区可用高度的三分之二。主区网格行与 `.main-panel` 设为 `min-height: 0` 和 `minmax(0, 1fr)`，使主区高度由窗口决定，不随内容撑大。
- 代码块与选项区之间的间距，与 description 和代码块之间的间距一致，均为 9px（`.perm-context` 的 gap）。
- 选项区超出高度时只滚动选项区，使用 8px 滚动轨道。轨道仅在溢出时显示，滑块使用降低透明度的 `--accent` 混合色。滚动容器四周保留 4px 内边距（外边距同值抵消），避免焦点环与 hover 边框被裁切。

### 工具类型与 description

- 工具类型 pill 左浮动，与其后的 description 行内相连：与 pill 同行的文字位于 pill 右侧 8px 处；换行到 pill 下方的文字左侧缩进 8px，不超出 pill 的左边缘。
- description 最多三行，超出截断，点击或按 `z` 展开/收起。description 仅在 agent 工具调用（如 `subagent`）中出现。

### 命令与文件路径

- 默认只显示两行：第一行显示开头，第二行显示结尾，中间省略。仅当内容超过两行时可折叠。
- 点击或按 `z` 展开完整内容，再次点击或按 `z` 收起。
- 可折叠内容 hover 时指针变为 pointer，并通过 `title` 提示 “Click or press Z to expand/collapse”。`z` 作用于当前面板中所有可折叠内容。

### 信任层级

- 只有文件路径与 Bash 命令需要第二层级（逐级扩大的 trust 列表）。
- 其他工具调用（如 `subagent`）在第一层级即可完成 `Trust in project`：主操作的 T 直接提交，不进入第二层级。
- 主操作选项列表与 trust 列表一样，溢出时只滚动选项区并显示滚动轨道。

### 圆点

- 圆点表示待审批的权限请求数量，不表示信任层级。
- Agent 连续发起多次调用时，每个待审批请求各占一个圆点；每完成一个审批移除一个圆点；只剩一个待审批请求时不显示圆点。
- 复合命令的两段信任选择依次出现在同一请求内，不计入圆点。

## 选项样式

### 默认与 hover

- Permission Request 与 AskUserQuestion 的选项行默认无可见边框。hover 时显示 `--border` 边框，背景与默认态相同（`color-mix(in oklch, var(--surface) 74%, transparent)`），文字颜色不变。
- hover 边框在默认态预留，hover 不推动相邻内容。Back 圆形图标按钮保留自身边框。
- Permission Request 的主操作与 trust 内容左对齐；主操作使用方框字母快捷键，字母为常规字重，选项内容加粗并匹配 AskUserQuestion 标签字重。

### 间距与布局

- Permission Request 选项容器上下各留 12px padding。文字按自然行高换行，换行只增加整行高度；24px 快捷键块在整行内垂直居中。hover 保持相同的上下 padding 和 1px 边框宽度。
- 选项行不设固定最小高度，行高由 12px padding 与内容决定（列表使用 `grid-auto-rows: max-content`）。
- 卡片变窄时，请求摘要和选项文字在卡片内换行并完整显示；选项按钮占满可用宽度，24px 快捷键列固定，文字列使用剩余宽度，长路径允许换行。
- 正常与 hover 状态不出现预期外的元素重叠。通知等明确设计的浮层可以遮挡。主操作与路径摘要之间留有明确间距。
- AskUserQuestion 保持现有选项留白。

### 选中态

- 选项行点击或聚焦后不增加边框、背景或 outline。
- 选中态只由按键指示器表达：已选项的按键块使用降低透明度的 `--accent`（约 22%），文字为 `--fg`，不使用实色强调块。
- Permission 选项的 hover 不改变按键指示器，与 AskUserQuestion 的 hover 一致。
- Other 行内输入框的边框使用选中边框色 `color-mix(in oklch, var(--accent) 35%, var(--border))`。输入框聚焦时同样不显示 outline。

## 问题面板

### 标题与按钮

- 多问题时标题不加 `[x/xx]` 序号前缀。
- Cancel 位于右上角。
- 多问题时，左下角为返回上一个问题的 Back 圆形按钮；第一个问题左下角留空。非最后一个问题的右下角为 Next question 按钮（`aria-label="Next question (N)"`，Lucide `ArrowRight`）；最后一个问题的右下角为 Done 按钮（`aria-label="Done (D)"`，Lucide `Check`）。
- 只有一个问题时，Cancel 位于左下角，不显示进度圆点，右下角为 Done 按钮。

### 进度圆点

- 底部按钮之间的圆点表示问题数量，仅多问题时显示。
- 未作答的圆点为灰色、低不透明度（0.3）；已作答的圆点为完整不透明度（1）。

### Other 选项

- 选择 Other 后，Other 选项行内的描述文字被输入框原位替换；输入框与该选项是同一个元素。
- 回答非空前，确认按钮保持置灰。无效答案不触发提交，不显示校验错误。

### 确认后的展示

问题确认后面板关闭，结果以普通工具调用样式显示在会话消息流中、输入框之上：`AskUserQuestion` 工具名、问题数量摘要，以及每个问题的答案。样式复用项目的 `desktop/renderer/templates/composites/tool-call.html` 与 `desktop/renderer/styles/features/tools.css`，不另起一套样式。

## 审批操作与请求归属

- 权限主操作的最终操作（Allow Once、Deny、任一信任范围）立即关闭当前请求并恢复输入框。T 进入信任范围页，H 进入拒绝提示页，二者属于操作路径切换，不是最终操作。取消提示返回主操作。
- 信任列表末项为圆形 `ArrowLeft` 控件（无障碍名称 `Back`），返回权限主操作，不提交信任。
- 同一 session 收到多个权限请求时，按到达顺序依次成为当前请求；界面不单列排队状态。
- 切换 session 后，旧请求的结果按原 `sessionId` 和 `requestId` 归属，不关闭或改变当前选中会话的面板。这是请求身份约束，不额外显示界面。
- 界面不显示提交中、响应重试、答案校验错误或迟到响应。

## 文件路径与信任范围

目标路径相对于启动项目根目录显示，trust 范围依次为：

1. `tmp/interruption-state-showcase/run-HdHEYs/permission-demo.txt`
2. `tmp/interruption-state-showcase/run-HdHEYs/`
3. `tmp/interruption-state-showcase/`
4. `tmp/`

末项为圆形控件内居中的 `ArrowLeft` 图标（无障碍名称 `Back`）。选择任一 trust 范围为逐行直接提交。当前权限辅助函数尚未生成这组相对路径与祖先目录范围；实现时须把上述路径顺序作为回归验收标准，并保留用于匹配和持久化的规范化路径身份。

## 权限请求类型

同一 Permission Request 面板展示三种请求数据形态，只改变工具名、参数摘要、description 和 trust 数据：

- **文件操作**：`Write`，显示相对项目根目录的路径，trust 范围为上面四级。
- **命令操作**：`Bash`，显示复合命令 `$ npm install lodash --workspace desktop --save-dev && git status --short`。两段信任选择依次出现：`npm install` 六级，`git status` 三级。层级对应 Rust `command_trust_levels` 的空格前缀规则。
- **通用 tool call**：使用 Rust 中真实的 `subagent` 工具 `spawn` 参数作为 Ask 规则命中示例。工具名 pill 旁显示 description，下方为 JSON 参数摘要；只有一层 trust（`Trust in project`）。

通用工具调用示例依赖显式 Ask 规则；Rust 默认规则可能直接允许 `subagent`，因此该样例不代表默认会弹出审批。

## 状态导航

审阅页的中文状态导航覆盖：主操作、信任范围、拒绝提示、操作后关闭、单选/多选未选、已选择、自定义回答、回答后关闭、单个问题、空会话和取消后恢复。

- **取消后恢复**：取消后关闭当前审批或问题面板，底部输入框重新出现；其他会话里的待审批请求和已作答问题保持不变。

后台队列、提交中、响应重试、答案校验错误和旧请求结果不作为产品界面状态。

## 对照来源

- Electron 窗口结构：`desktop/renderer/styles/layouts/app-shell.css`、`desktop/renderer/styles/layouts/sidebar-shell.css`。
- 侧栏结构：`desktop/renderer/styles/features/sidebar.css` 与 `prototype/scenes/permission-request.html`。
- 权限与问题卡片结构：`desktop/renderer/templates/composites/interruption-panel.html`、`desktop/renderer/styles/composites/permission-question.css`、`desktop/renderer/scripts/components/interruption-panel.tsx`。
- 工具调用：`desktop/renderer/templates/composites/tool-call.html`、`desktop/renderer/styles/features/tools.css`。
- Rust 对照：`crates/rozsa-gui/frontend/ui/templates/composites/interruption-panel.html` 与 `crates/rozsa-gui/frontend/ui/styles/composites/permission-question.css`。

## 预期实现范围与负责人

经用户审阅通过后，TASK-T039 负责实现和验收。候选文件范围：

- `backend/cmd/rozsa-backend/main.go`：向 runtime 传递稳定启动项目根目录。
- `backend/internal/app/runtime/runtime.go` 和 `backend/internal/app/runtime/permission_tools.go`：将稳定项目根目录与会话 Bash 工作目录区分，并用于权限策略。
- `backend/internal/app/runtime/permission_tools_test.go`：启动根目录与会话工作目录分离的回归测试。
- `backend/internal/app/permissions/policy.go` 和 `backend/internal/app/permissions/permissions_test.go`：返回相对显示目标及四级祖先范围，内部仍按规范化路径 key 匹配。
- `devtools/show-interruption-states.mjs` 和 `desktop/tests/approval-question-lifecycle.test.js`：在实际 Electron+Go 演示中验证路径显示与信任顺序。
- `prototype/scripts/components/interruption-panel.js`、`prototype/scenes/permission-request.html`、`prototype/scenes/ask-user-question.html`：只补齐本任务需要的交互状态和示例数据，不改全局窗口骨架、侧栏、色板、字体、面板圆角或布局。

T039 在 `desktop/renderer/styles/composites/permission-question.css` 中实现选项的默认无边框、hover 边框、两类选项的等距留白、Permission Request 粗体内容，以及圆形取消与返回控件的尺寸、边界和焦点样式。在 `desktop/renderer/scripts/components/interruption-panel.tsx` 中使用方框 Y/T/N/H 快捷键、Lucide `X` 与 `ArrowLeft`，保留 `aria-label="Cancel"` 和 `aria-label="Back"`。在 `desktop/tests/approval-question-lifecycle.test.js` 中覆盖两个图标操作的无障碍名称和返回行为。以上文件均在 TASK-T039 的已声明 scope 内。开始实现前，须按 T006 要求核对 T039 acceptance、共享文件顺序与批准范围。
