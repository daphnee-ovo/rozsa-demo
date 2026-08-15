# 新版 GUI 迁移覆盖清单

本文档是新版 GUI 迁移的人工审阅入口。`docs/gui/new-version/` 中的 HTML、CSS、JavaScript 是视觉与交互实现原版；现有 Tauri GUI 继续拥有业务状态和业务行为。隐藏节点、空容器、只有 CSS 的规则或 no-op 演示 handler，均不能证明某个可见状态已经覆盖。

机器可验证的场景、状态、runtime 入口、原型证据和阻断证据位于 [`NEW_VERSION_MIGRATION_COVERAGE.json`](./NEW_VERSION_MIGRATION_COVERAGE.json)。测试直接读取该清单指向的真实源码，不再把 Markdown 中出现某个名字视为覆盖证据。

## 状态约定

| 状态 | 含义 | 迁移规则 |
| --- | --- | --- |
| `covered` | 原型已经实际呈现该表面及所需交互状态。 | 可进入清单指定的迁移任务。 |
| `partial` | 组件已出现，但一个或多个 runtime 可见变体没有实际呈现或操作。 | 默认阻断整个共用组件族；只有用户批准且可独立验收的子项可以迁移。 |
| `missing` | 没有可执行的具体原型表面。 | 不实现，向用户反馈并等待补充原型。 |
| `non-visual` | 仅需保留 runtime ownership、路由或平台行为，不产生新的视觉决策。 | 原样保留并测试，不自行补画。 |

本迭代尚无已批准的 `partial` 例外。任何例外都必须记录独立 DOM/CSS 边界、未改变未覆盖状态的证据、不混用新旧视觉的证据、独立验收材料以及用户批准引用。

## 权威输入与实质性校验

覆盖完整性来自以下输入的并集：

1. `frontend/index.html`、`frontend/sidebar.html` 中的 scene root、template、panel、dialog、popover 和全局 layer。
2. `frontend/app.js`、`frontend/sidebar.js`、`frontend/gui_shared.js` 中的可见 renderer、状态转换和双 WebView ownership。
3. Tauri 可见事件、命令结果、错误路径，以及 native split、fallback sidebar、scene router 和平台分支。
4. `docs/gui/new-version/scenes/` 的全部 HTML 场景、共用 `rozsa-gui.js`，以及 `styles/main.css`、`styles/sidebar.css`、它们直接导入的组件 CSS、`styles/scenes/` 场景覆盖和 `styles/features/visual-demo.css`；原单体 `rozsa-gui.css` 已删除，不存在兼容入口。

`prototype_coverage_inventory_test.rs` 会直接执行以下检查：

- 磁盘上的全部原型 HTML 必须与 JSON 场景注册表双向相等；新增或删除场景会立即失败。
- 每个场景的 `data-rozsa-scene` 必须与文件名一致，且必须直接加载 `styles/main.css` 和原版 JS，并保留两个稳定 scene root。
- `styles/` 下全部 CSS 必须与机器清单双向相等；新增、删除或改名的组件不能静默绕过分类。
- `styles/main.css` 与 `styles/sidebar.css` 的实际 import 必须分别与清单完全相等；`main.css` 的顺序还必须与 `styles/source-order.json` 中原单体 CSS 的连续块顺序完全相等。
- 场景覆盖 CSS 只能由清单声明的场景直接加载，清单、HTML 引用和 `source-order.json` 的 inline-style 抽取记录必须三向一致；visual demo 的独立样式同理。
- 根原型必须直接加载 `styles/main.css`，任何 HTML 不得引用已删除的 `rozsa-gui.css`，磁盘也不得重新出现该兼容入口。
- 每条 runtime、原型和阻断证据指向的文件必须真实存在，声明的 token 必须直接存在于源码。
- 所有 `covered` 表面必须同时具有 runtime 与原型源码证据；所有 `missing` 表面必须具有来自原型源码的具体阻断证据。
- `app.js` 与 `sidebar.js` 中实际注册的可见事件集合必须与清单完全相等；新增事件不能静默绕过 ownership 审查。
- Markdown 仅用于中文说明和审阅，不能再作为源码事实的替代品。

原型样式已按与 runtime 对齐的 `styles/` 分层保存。当前 CSS 实现真源是 `styles/**`：`main.css` 是完整场景入口，`sidebar.css` 是 sidebar 子集入口，公共规则按 token、reset、base、layout、components、features、utilities 分层；只有无法放入公共组件而又必须保持原始位置语义的规则位于 `styles/scenes/`。`styles/source-order.json` 只记录拆分来源、连续块顺序与摘要，用于证明“仅搬移、未改样式”，不是另一份可加载样式。后续迁移和新增场景必须直接复用组件库，不再重复拆分，也不得恢复 `rozsa-gui.css` 兼容入口。

## 本迭代基线

`TASK-T001` 于 2026-08-15 开始时：

- `git status --short -- crates/rozsa-gui/src` 无输出。
- `git diff --stat -- crates/rozsa-gui/src` 无输出。
- 因此任务开始时 `crates/rozsa-gui/src/` 没有可见的已跟踪或未跟踪工作树变化。
- 该目录之外已有其他工作树变化，不归因于本次迁移。
- 本迭代没有计划修改 Rust production 文件；后续如需修改，必须满足 `SPEC-AC-013` 的证据、批准和 task scope 更新要求。

## 任务文件边界

| 任务 | 负责范围 | 验收产物 |
| --- | --- | --- |
| `TASK-T001` | 覆盖清单、缺口报告 | 可执行 inventory contract test |
| `TASK-T002` | 共用 token/reset/base/layout、stylesheet entry、`gui_shared.js` | stylesheet fidelity test |
| `TASK-T003` | sidebar HTML/JS 与 sidebar feature CSS | sidebar fidelity evidence |
| `TASK-T004` | session/conversation HTML/JS 与 conversation/feedback CSS | session fidelity evidence |
| `TASK-T005` | tool HTML/JS 与 `tools.css` | tool fidelity evidence |
| `TASK-T006` | permission/question HTML/JS 与 actions/forms/overlays CSS | interruption fidelity evidence |
| `TASK-T007` | settings HTML/JS 与 settings/appearance/actions/forms CSS | settings fidelity evidence |
| `TASK-T008` | notification HTML/JS 与 feedback/overlays CSS | notification fidelity evidence |
| `TASK-T009` | Dev Flow 主视图/sidebar HTML/JS 与相关 feature CSS | Dev Flow fidelity evidence |
| `TASK-T010` | 不修改 production 文件 | runtime/platform preservation evidence |
| `TASK-T011` | 最终 stylesheet entry 组装和中央验收文档 | 跨表面 fidelity manifest |
| `TASK-T012` | GUI 架构、术语、主题、贡献与维护文档 | 同步后的中文文档 |
| `TASK-T013` | 将新版原型 CSS 原样拆为 `styles/` 组件库并让 HTML 直接接入 | 字节级来源清单、浏览器等价证据与组件库结构测试 |

临时 fidelity 产物只允许位于各任务声明的 `tmp/gui-new-version/TASK-T003` 至 `TASK-T011` 路径。

## 覆盖矩阵

详细源码证据以 JSON 为准；下表用于人工判断迁移范围。

| ID | 表面与状态族 | 迁移归属 | 状态 | 缺口 |
| --- | --- | --- | --- | --- |
| `SURF-001` | 应用 shell、顶栏、阅读列、浮动 composer；明暗主题、窄屏、reduced motion | T002/T004 | `covered` | — |
| `SURF-002` | main/settings 稳定 scene root、可见性、inert、revision continuity | T002 | `non-visual` | — |
| `SURF-003` | sidebar session、基本选择、active/idle/empty/new session | T003 | `covered` | — |
| `SURF-004` | sidebar Git 状态、tool chip、quota bar | T003 | `covered` | — |
| `SURF-005` | quota tooltip、near-limit/error/hidden 组合 | 阻断 | `partial` | `GAP-001` |
| `SURF-006` | 空白会话 poster | T004 | `covered` | — |
| `SURF-007` | 用户/assistant 消息、continuation、streaming cursor、error block | 阻断未覆盖状态 | `partial` | `GAP-002` |
| `SURF-008` | thinking block 的展开、收起和时长 | T004 | `covered` | — |
| `SURF-009` | Read/Edit/Bash tool evidence 的生命周期与展开状态 | 阻断未覆盖状态 | `partial` | `GAP-003` |
| `SURF-010` | Subagent、AskUserQuestion、generic tool evidence | 阻断 | `partial` | `GAP-003` |
| `SURF-011` | code、diff、file changes、verification/turn activity | T005 | `covered` | — |
| `SURF-012` | rich Markdown 全状态矩阵与 overflow | 阻断 | `partial` | `GAP-002` |
| `SURF-013` | composer、rich input、send/abort、context ring | T004 | `covered` | — |
| `SURF-014` | slash autocomplete 列表及键盘/空白/错误状态 | 阻断 | `missing` | `GAP-004` |
| `SURF-015` | 文件/目录附件、引用和拖放反馈 | 阻断 | `missing` | `GAP-005` |
| `SURF-016` | model picker、空列表和切换错误 | 阻断 | `missing` | `GAP-006` |
| `SURF-017` | thinking effort picker | T004 | `covered` | — |
| `SURF-018` | queue、steering、fork、subagent 的操作/空白/错误状态 | 阻断 | `partial` | `GAP-007` |
| `SURF-019` | permission 主页面与 deny hint | 阻断组件族 | `partial` | `GAP-008` |
| `SURF-020` | permission trust、排队、错误和取消状态 | 阻断 | `missing` | `GAP-008` |
| `SURF-021` | ask-user-question 选项、Other、校验、提交/取消 | T006 | `covered` | — |
| `SURF-022` | settings 导航与 Appearance 控件 | T007 | `covered` | — |
| `SURF-023` | General settings 的填充、加载和错误状态 | 阻断 | `missing` | `GAP-009` |
| `SURF-024` | Skills/Tools capability 设置 | 阻断 | `missing` | `GAP-010` |
| `SURF-025` | keyboard shortcut 列表、编辑、冲突和错误 | 阻断 | `missing` | `GAP-011` |
| `SURF-026` | permission settings、规则列表与编辑器 | 阻断 | `missing` | `GAP-012` |
| `SURF-027` | Extensions 保留空状态 | T007 | `covered` | — |
| `SURF-028` | notification stack、关闭、error tray/list | T008 | `covered` | — |
| `SURF-029` | notification lifecycle 与 model-config reconciliation | T008 | `non-visual` | — |
| `SURF-030` | Dev Flow settings 的 unavailable/connected/dashboard | T009 | `covered` | — |
| `SURF-031` | Dev Flow sidebar 的 claimed work、more、hover/pin | T009 | `covered` | — |
| `SURF-032` | Dev Flow runtime detail、focus、expand、refs、file tree、pin/narrow | T009 | `covered` | — |
| `SURF-033` | Dev Flow Bash tool presentation | T009 | `covered` | — |
| `SURF-034` | native sidebar/fullscreen、双 WebView host、非 macOS fallback | T010 | `non-visual` | — |
| `SURF-035` | 其他跨组件 loading/error/empty/disabled/long-content 状态 | 阻断 | `partial` | `GAP-013` |

## 平台验收策略

- macOS：本迭代完成完整视觉和行为验收。
- Linux/Windows：本迭代完成共享 DOM/CSS/renderer、keyboard/focus、IPC 和 fallback contract 测试；真实视觉结果保持 `pending platform visual verification`，直到用户提供相应环境。

## 当前门禁结论

- `covered` 只能进入声明的任务，不代表已完成迁移。
- `non-visual` 只允许保留和验证既有行为，不允许新增视觉设计。
- `partial` 与 `missing` 继续阻断，具体需求见 [`NEW_VERSION_PROTOTYPE_GAPS.md`](./NEW_VERSION_PROTOTYPE_GAPS.md)。
- 没有任何已批准的 partial-component 例外。
- `TASK-T013` 已完成 CSS 原样拆分；这只改变样式来源定位，不改变 35 个 surface 的覆盖分类和缺口判断。
- 当前 covered 原型未证明需要扩展 Rust/IPC；最新 Dev Flow detail payload 已包含 refs、dependency、done-when 与 create/modify/test 文件数组。

## 相关文档

- [`DESIGN.md`](../../DESIGN.md)
- [`新版 GUI 原型`](./new-version/)
- [`SPEC.md`](../../.dev-doc/main/SPEC.md)
- [`新版原型缺口`](./NEW_VERSION_PROTOTYPE_GAPS.md)
- [`GUI 架构`](./ARCHITECTURE.md)
