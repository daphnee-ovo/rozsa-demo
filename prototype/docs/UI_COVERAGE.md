# GUI 界面对照表

本文档是 [`UI_COVERAGE.json`](./UI_COVERAGE.json) 的中文审阅入口。JSON 是逐项事实的唯一结构化真源；本文只说明字段、状态和区域结论，不复制一套可独立维护的详细清单。

## 使用方式

每个 `items` 项目都有不会随文件移动改变的稳定 ID，并记录 runtime 入口与触发方式、prototype 入口、覆盖状态、重构后的 CSS/markup/behavior 责任位置、全部使用方和重构前后验证 ID。`prototype_inventory` 覆盖 `index.html` 主入口、11 个 application scene、gallery HTML/脚本与 shell/renderer/legacy behavior 脚本；`runtime_inventory` 覆盖 main/sidebar HTML、三个运行脚本和所有已注册可见事件。

prototype 的统一入口是 `prototype/index.html`：`?scene=<id>` 装配 application scene，`?gallery=visual-direction` 装配 `gallery/visual-direction.html`。独立 visual demo 基线材料只用于重构前后成对比较，不能替代 gallery 入口。

后续移动文件时更新责任路径，不更换项目 ID。公共实现的每个 `used_by` 都必须进入同一项目的验证范围。`verification` 在 TASK-T002 建立重构前材料后成为前后成对检查的稳定键。

状态含义固定如下：

| 状态 | 含义 |
| --- | --- |
| `covered` | 新版原型已展示对应界面、状态和需要验证的交互。 |
| `partial` | 有对应组件，但缺少 runtime 已有的一个或多个状态。 |
| `missing` | runtime 有可见内容，新版原型没有。 |
| `non-visual` | 只有数据接线、路由或 native 行为，没有独立视觉表现。 |

`partial` 的已覆盖状态接入共享 UI；未覆盖状态与 `missing` 项保留空容器并暂停相关操作。设计缺口沿用 [`DESIGN_GAPS.md`](./DESIGN_GAPS.md) 的原有 ID，缺失设计与功能恢复在后续迭代逐项处理，不由实施者补画。

每个 `partial`/`missing` 项新增 `shared_migration`，记录本次迁移处置：

| 字段 | 含义 |
| --- | --- |
| `gap_id` | 原有设计缺口 ID；不新增覆盖状态。 |
| `owner_task` | 负责接入或暂停检查的开发任务。 |
| `empty_container` | 未覆盖状态保留的区域或 slot；不授权新设计。 |
| `paused_entries` | 需要暂停的可见入口及隐藏操作路径。 |
| `user_impact` | 暂缺操作的用户影响及已覆盖部分的保留边界。 |
| `safety_gate` | 切换前须满足的安全门槛；无法安全暂停则阻断对应区域迁移。 |
| `safety_evidence` | 已核对的当前源码路径与入口；不代替实际操作验证。 |

这些字段记录处置边界；实际实施状态结合 `implementation_status` 与专项证据核对，覆盖状态本身不等于迁移完成。后台事件、跨 WebView 路由和在途请求必须由对应开发任务验证；不自动回应请求、不丢弃真实错误、不生成假成功。共享切换与 T036 清理都以对应安全门槛及专项证据为前提。清理已验收区域时保留受阻区域的必要实现；这些区域不计为已迁移，设计补齐留待后续迭代。

Permission request 队列、reply 失败、被动 message/tool 状态与在途恢复保留真实出口；只迁移原型覆盖的正常状态，未覆盖部分仍标为受阻。主动 Trust 与缺失操作已暂停，不以隐藏 DOM 作为被动事件安全结论。

## 响应式布局与平台

浏览器 shell 的 sidebar 自动收起边界为 780px，恢复展开为 820px；macOS native split 的收起由 AppKit 管理。共享 viewport 以全窗口宽度提供响应式 token，避免分栏宽度触发整窗断点。最低窗口约束为 640×480，规则归属实际 owner CSS，入口顺序见 [source-order.json](../styles/source-order.json)。macOS 已实际重放共享 UI；Linux/Windows 真实视觉保持 pending platform visual verification。

## 区域汇总

### Application shell 与 sidebar

- 项目：`UI-SHELL-001`、`UI-SHELL-002`、`UI-SIDEBAR-001`、`UI-SIDEBAR-002`、`UI-SIDEBAR-003`。
- 结论：应用骨架、scene continuity、session/sidebar 基础状态已分类；quota tooltip 与限额语义变体为 `partial`。

### Conversation、Composer、Queue 与 Steering

- 项目：`UI-CONV-001`、`UI-CONV-002`、`UI-CONV-003`、`UI-COMPOSER-001`、`UI-COMPOSER-002`、`UI-COMPOSER-003`、`UI-COMPOSER-004`、`UI-COMPOSER-005`、`UI-FLOW-001`。
- Runtime 边界：autocomplete、附件/拖放、model picker、fork/subagent 操作及运行中 queue/steer 提交暂停，未覆盖容器为空；输入、IME、draft、真实 model badge、已覆盖 populated queue/steering 与 Stop 保留。`UI-CONV-002` 的被动 streaming/continuation/error 状态迁移受阻，现有处理不得在 T036 删除。具体入口与状态见 [DESIGN_GAPS](./DESIGN_GAPS.md)。
- 结论：空会话、thinking、Composer 与 thinking effort 已覆盖；完整消息矩阵、queue/steering/fork/subagent 为 `partial`；autocomplete、附件/拖放和 model picker 为 `missing`。

### Tool call、evidence、diff 与 Markdown

- 项目：`UI-TOOL-001`、`UI-TOOL-002`、`UI-EVIDENCE-001`、`UI-MARKDOWN-001`。
- 结论：code/diff/file changes/turn activity 已覆盖；tool lifecycle 与 rich Markdown 完整状态为 `partial`。
- Runtime 边界：Read/Bash 非空成功输出、Edit 单文件完整 patch 与普通完成消息的纯文本/fenced code 已共享；未覆盖的正常 evidence/Markdown block 留空并暂停操作。真实工具错误、待回答问题和被动 streaming/continuation/error 消息保留现有处理，迁移受阻，T036 不得删除这些出口。
- File changes/verification 的成功且元数据完整状态共用 renderer；失败、不完整 capture 与未覆盖文件元数据状态保留现有处理，标记 `UI-EVIDENCE-001` migration-blocked。

### PermissionRequest 与 AskUserQuestion

- 项目：`UI-PERMISSION-001`、`UI-PERMISSION-002`、`UI-QUESTION-001`。
- 结论：AskUserQuestion 单问题单选已覆盖，完整状态为 `partial`；permission main/hint 为 `partial`，trust、排队、错误与取消为 `missing`。
- Runtime 边界：正常 permission main/hint 与单问题单选共用 `ui/templates/composites/interruption-panel.html`、`ui/styles/composites/permission-question.css`、共享 component/feature/focus flow；真实请求、队列与 IPC 留在 runtime adapter/handler。
- 主面板保留原型四个选项；Trust 按钮禁用，后续空 slot 与选择入口暂停。多问题、多选、长/复合命令、排队及真实响应失败保留现有处理，迁移受阻；T036 不得删除这些出口。缺失设计留待后续迭代。

### Settings、Appearance 与 permission settings

- 项目：`UI-SETTINGS-001`、`UI-SETTINGS-002`、`UI-SETTINGS-003`、`UI-SETTINGS-004`、`UI-SETTINGS-005`、`UI-SETTINGS-006`。
- 结论：Appearance 与 Extensions 空状态已覆盖；General、Skills/Tools、keyboard shortcut 和填充后的 permission rules 为 `missing`。
- 共享边界：两宿主引用 `ui/templates/features/settings.html`、共享 Settings/Appearance CSS 和 `ui/scripts/features/settings.js`；native Settings 导航也从同一模板装配。原型使用 mock state，runtime adapter 保留真实 settings/theme IPC、持久化键与平台主题处理。
- 暂停边界：五个未覆盖 pane 为空容器，main/sidebar 导航与路由、旧 renderer 和修改入口暂停；不预载 capability/permission settings。全局快捷键读取和真实审批流程保留。Dev Flow settings 已共享；受阻区域所需的惰性模板与隔离规则保留，不计入已迁移范围。

### Notification 与错误反馈

- 项目：`UI-NOTIFICATION-001`、`UI-NOTIFICATION-002`、`UI-FEEDBACK-001`。
- 共享边界：两宿主共用 `ui/templates/composites/notification.html`、`ui/styles/composites/feedback.css` 和 notification renderer/事件绑定。即时通知、未解决错误入口和列表统一位于整个窗口右上角。运行时仅 main 挂载全局通知中心，保留身份、计时、悬停暂停和 resolve；sidebar 的 IPC rejection 转交 main，关闭后保留未解决错误。
- 暂停边界：跨组件异常 variant 仍为 `partial`，缺失 retry/cancel/恢复操作沿各区域缺口暂停。被动错误继续透明表达；共享 UI bootstrap 失败时保留旧错误出口，该分支不计为已迁移。sidebar 已接入同一全局通知出口，已覆盖通知视觉只有一份共享 owner。

### Dev Flow

- 项目：`UI-DEVFLOW-001`、`UI-DEVFLOW-002`、`UI-DEVFLOW-003`、`UI-DEVFLOW-004`、`UI-DEVFLOW-005`。
- 结论：settings、sidebar、runtime detail、Bash presentation 与双 WebView/native ownership 均已分类；native host 项为 `non-visual`。
- 已覆盖 Sidebar 与 Dev Flow 结构、样式和交互共用生产 `ui/`；原型使用 mock，runtime adapter 连接真实 Tauri 数据。quota tooltip 暂停；native snapshot 未提供的 tool chip/subagent 数据保持空 slot。详情中的 done_when 与空列表提示为 `partial`，沿用 [GAP-014](./DESIGN_GAPS.md) 留待后续设计。

### Gallery 视觉参考

- 项目：`UI-GALLERY-001`、`UI-GALLERY-002`、`UI-GALLERY-003`、`UI-GALLERY-004`。
- 入口：`prototype/index.html?gallery=visual-direction`；对照实现：`gallery/visual-direction.html`、`gallery/visual-direction.js`，共享 `styles/features/visual-demo.css`。
- 结论：Independent demo topbar 与 Empty、Active、Appearance 三个视图均作为 gallery 专属内容登记；它们不是 runtime application scene。独立 visual demo 基线材料只用于与 baseline 成对比较。

## 维护约束

- 新增或删除 scene、gallery view、runtime entry、可见事件或界面状态时，必须同步更新 JSON inventory 和对应 item；gallery 的 HTML、脚本入口和 shared CSS owner 必须同时可追溯。
- 一个项目只能使用四种固定状态；`partial`/`missing` 必须在缺口表有且只有一个可追溯条目。
- `css`、`markup`、`behavior` 分别描述最终唯一责任位置，不能把第二份公共实现藏在 scene 或 gallery 中。
- prototype CSS 路径必须对应 `foundations`、`primitives`、`composites`、`features` 或 `layouts` 的唯一 owner；`styles/scenes` 不是 application runtime owner，响应式规则直接归属于实际 owner，入口只负责保持 import source order。
- application shell 的稳定 HTML 只能来自 `templates/`，入口只提供 mount；`scripts/core/` 只负责加载、装配和 renderer 边界，scene 文件是配置输入且不得带有运行时 stylesheet 加载。
- scene/gallery 的唯一路由登记入口是 `scenarios/registry.js`；scene 读取路径经过 `scripts/core/scene-loader.js` 和 `scripts/adapters/mock-adapter.js`，gallery 通过统一 entry route 装配；mock 文件及每个 scene 的全部使用方必须登记在 JSON `scenario_inventory`，未知 ID 必须明确报错且不能 fallback 到未定义页面。
- 已提升的 Composer、conversation 与 Popover 由 `crates/rozsa-gui/frontend/ui/` 定义模板、样式与组件行为，prototype 同路径文件转发到共享 owner。未覆盖受阻区域保留必要的 runtime owner；各组件使用方必须在 JSON 的 `behavior` 和 `used_by` 中闭合记录。
- 跨组件 flow 的唯一归属是 `scripts/flows/`：`composer-interruption.js` 只协调 interruption 与 composer 的 focus restore，`scenario-flow.js` 只协调 scene-ready 与 application shell 的装配时序；两者的 component、feature、scene 使用方登记在 JSON `flow_inventory`，按全部使用方统一验证。
- 本清单与自动化测试只证明分类和引用闭合，不证明视觉或交互等价；实际验收使用 baseline 与真实宿主重放；gallery 仅浏览器验证，缺口不计为迁移完成。

## Related Docs

- [`SPEC：GUI 原型与真实前端组件化重构`](../../.dev-doc/refactor/ui-architecture/SPEC.md)
- [`设计缺口表`](./DESIGN_GAPS.md)
- [`GUI 架构`](./ARCHITECTURE.md)
- [`界面使用指南`](../../docs/gui/UI_USAGE_GUIDELINES.md)
