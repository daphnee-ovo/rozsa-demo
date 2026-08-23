# GUI 界面对照表

本文档是 [`UI_COVERAGE.json`](./UI_COVERAGE.json) 的中文审阅入口。JSON 是逐项事实的唯一结构化真源；本文只说明字段、状态和区域结论，不复制一套可独立维护的详细清单。

## 使用方式

每个 `items` 项目都有不会随文件移动改变的稳定 ID，并记录 runtime 入口与触发方式、prototype 入口、覆盖状态、重构后的 CSS/markup/behavior 责任位置、全部使用方和重构前后验证 ID。`prototype_inventory` 覆盖 `index.html` 主入口、兼容入口、11 个 application scene、gallery HTML/脚本与 shell/renderer/legacy behavior 脚本；`runtime_inventory` 覆盖 main/sidebar HTML、三个运行脚本和所有已注册可见事件。

prototype 的统一入口是 `prototype/index.html`：`?scene=<id>` 装配 application scene，`?gallery=visual-direction` 装配 `gallery/visual-direction.html`。T015 的独立 visual demo 基线材料只用于重构前后成对比较，不能替代 gallery 入口。

后续移动文件时更新责任路径，不更换项目 ID。公共实现的每个 `used_by` 都必须进入同一项目的验证范围。`verification` 在 TASK-T002 建立重构前材料后成为前后成对检查的稳定键。

状态含义固定如下：

| 状态 | 含义 |
| --- | --- |
| `covered` | 新版原型已展示对应界面、状态和需要验证的交互。 |
| `partial` | 有对应组件，但缺少 runtime 已有的一个或多个状态。 |
| `missing` | runtime 有可见内容，新版原型没有。 |
| `non-visual` | 只有数据接线、路由或 native 行为，没有独立视觉表现。 |

`partial` 与 `missing` 的具体阻断范围只在 [`DESIGN_GAPS.md`](./DESIGN_GAPS.md) 维护。它们可以整理 runtime 现有实现，但不得由实施者推导或补画新视觉。

## 响应式布局验证（TASK-T024）

sidebar 的自动收起由主内容可用宽度驱动：约 220px sidebar 加 560px 主内容舒适宽度得到 780px 收起边界；恢复展开使用 820px 滞回边界。产品与 prototype 的最低窗口约束统一为 640×480，正常 PC、820px 分屏和最低尺寸的几何、Composer 与 sidebar 状态记录在 [`TASK-T024 verification`](../../tmp/ui-architecture-refactor/TASK-T024/verification.md) 与 [`source-order.json`](../styles/source-order.json) 中。响应式规则归属实际 owner CSS，不再以断点生成独立 CSS 文件。

## 区域汇总

### Application shell 与 sidebar

- 项目：`UI-SHELL-001`、`UI-SHELL-002`、`UI-SIDEBAR-001`、`UI-SIDEBAR-002`、`UI-SIDEBAR-003`。
- 结论：应用骨架、scene continuity、session/sidebar 基础状态已分类；quota tooltip 与限额语义变体为 `partial`。

### Conversation、Composer、Queue 与 Steering

- 项目：`UI-CONV-001`、`UI-CONV-002`、`UI-CONV-003`、`UI-COMPOSER-001`、`UI-COMPOSER-002`、`UI-COMPOSER-003`、`UI-COMPOSER-004`、`UI-COMPOSER-005`、`UI-FLOW-001`。
- 结论：空会话、thinking、Composer 与 thinking effort 已覆盖；完整消息矩阵、queue/steering/fork/subagent 为 `partial`；autocomplete、附件/拖放和 model picker 为 `missing`。

### Tool call、evidence、diff 与 Markdown

- 项目：`UI-TOOL-001`、`UI-TOOL-002`、`UI-EVIDENCE-001`、`UI-MARKDOWN-001`。
- 结论：code/diff/file changes/turn activity 已覆盖；tool lifecycle 与 rich Markdown 完整状态为 `partial`。

### PermissionRequest 与 AskUserQuestion

- 项目：`UI-PERMISSION-001`、`UI-PERMISSION-002`、`UI-QUESTION-001`。
- 结论：AskUserQuestion 已覆盖；permission main/hint 为 `partial`，trust、排队、错误与取消为 `missing`。

### Settings、Appearance 与 permission settings

- 项目：`UI-SETTINGS-001`、`UI-SETTINGS-002`、`UI-SETTINGS-003`、`UI-SETTINGS-004`、`UI-SETTINGS-005`、`UI-SETTINGS-006`。
- 结论：Appearance 与 Extensions 空状态已覆盖；General、Skills/Tools、keyboard shortcut 和填充后的 permission rules 为 `missing`。

### Notification 与错误反馈

- 项目：`UI-NOTIFICATION-001`、`UI-NOTIFICATION-002`、`UI-FEEDBACK-001`。
- 结论：notification 可见表面和非可见生命周期已分类；跨组件异常状态仍为 `partial`。

### Dev Flow

- 项目：`UI-DEVFLOW-001`、`UI-DEVFLOW-002`、`UI-DEVFLOW-003`、`UI-DEVFLOW-004`、`UI-DEVFLOW-005`。
- 结论：settings、sidebar、runtime detail、Bash presentation 与双 WebView/native ownership 均已分类；native host 项为 `non-visual`。

### Gallery 视觉参考

- 项目：`UI-GALLERY-001`、`UI-GALLERY-002`、`UI-GALLERY-003`、`UI-GALLERY-004`。
- 入口：`prototype/index.html?gallery=visual-direction`；对照实现：`gallery/visual-direction.html`、`gallery/visual-direction.js`，共享 `styles/features/visual-demo.css`。
- 结论：Independent demo topbar 与 Empty、Active、Appearance 三个视图均作为 gallery 专属内容登记；它们不是 runtime application scene。独立 visual demo 基线材料只用于与 baseline 成对比较。

## 维护约束

- 新增或删除 scene、gallery view、runtime entry、可见事件或界面状态时，必须同步更新 JSON inventory 和对应 item；gallery 的 HTML、脚本入口和 shared CSS owner 必须同时可追溯。
- 一个项目只能使用四种固定状态；`partial`/`missing` 必须在缺口表有且只有一个可追溯条目。
- `css`、`markup`、`behavior` 分别描述最终唯一责任位置，不能把第二份公共实现藏在 scene 或 gallery 中。
- prototype CSS 路径必须对应 `foundations`、`primitives`、`composites`、`features`、`layouts` 或 `scenes` 的唯一 owner；响应式规则直接归属于该 owner，入口只负责保持 import source order。
- application shell 的稳定 HTML 只能来自 `templates/`，入口只提供 mount；`scripts/core/` 只负责加载、装配和 renderer 边界，scene 文件仍是只读迁移输入。
- scene/gallery 的唯一路由登记入口是 `scenarios/registry.js`；scene 读取路径经过 `scripts/core/scene-loader.js` 和 `scripts/adapters/mock-adapter.js`，gallery 通过统一 entry route 装配；mock 文件及每个 scene 的全部使用方必须登记在 JSON `scenario_inventory`，未知 ID 必须明确报错且不能 fallback 到未定义页面。
- prototype component behavior 的唯一归属是 `scripts/components/` 或 `scripts/features/`；Composer、interruption、popover、notification、tool-call、conversation、settings、sidebar 与 Dev Flow 的使用方必须在 JSON 的 `behavior` 和 `used_by` 中闭合记录。
- 跨组件 flow 的唯一归属是 `scripts/flows/`：`composer-interruption.js` 只协调 interruption 与 composer 的 focus restore，`scenario-flow.js` 只协调 scene-ready 与 application shell 的装配时序；两者的 component、feature、scene 使用方登记在 JSON `flow_inventory`，交由 T015 统一验证。
- 本清单与自动化测试只证明分类和引用闭合，不证明视觉或交互等价；最终结论必须来自 TASK-T002 建立的成对材料与后续实际重放。

## Related Docs

- [`SPEC：GUI 原型与真实前端组件化重构`](../../.dev-doc/refactor/ui-architecture/SPEC.md)
- [`设计缺口表`](./DESIGN_GAPS.md)
- [`GUI 架构`](./ARCHITECTURE.md)
- [`界面使用指南`](./UI_USAGE_GUIDELINES.md)
