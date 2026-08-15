# SPEC: GUI 新设计语言忠实迁移

## Goal

将 `docs/gui/new-version/` 中已经实现的 CSS、HTML、JavaScript 忠实迁移到
Rózsa Tauri GUI。原型代码是具体视觉与交互实现的母版；迁移只做运行时集成所必需的
抽取、拆分、位置搬移和数据接线，不重新解释、优化或近似复刻原型，也不改变现有产品
业务逻辑。

迁移完成后，原型覆盖的界面在真实运行时中应保持相同的 DOM 表达、视觉语言、交互反馈
和响应式行为，同时继续满足现有 session、streaming、tool、permission、question、settings、
native split、Dev Flow 等运行时契约。原型未覆盖的可见场景或组件不由实现方自行设计，
必须形成缺口报告并等待原型补充。

## Scope

### In

- 以 `docs/gui/new-version/rozsa-gui.css`、`docs/gui/new-version/rozsa-gui.js` 和
  `docs/gui/new-version/scenes/*.html` 为迁移母版。
- 将原型公共 CSS 按现有 runtime CSS 分层规则搬入 `frontend/styles/`，保持最终级联、
  声明值、selector 语义、状态表现和响应式结果不变。
- 将原型场景 HTML 搬入 runtime 静态 DOM、template 或现有 renderer；只允许增加运行时
  绑定所必需且不改变视觉的 ID、`data-*`、ARIA 和事件连接。
- 将原型交互 JavaScript 接入现有运行时状态、事件和 Tauri IPC；使用薄适配层完成数据
  形状转换，不在适配层产生视觉决策。
- 保留现有 session continuity、IME、scroll、selection、expanded state、permission progress、
  native scene identity 和双 WebView 生命周期行为。
- 建立“运行时可见表面 → 原型场景/组件 → 目标文件 → 测试”的覆盖矩阵。
- 对原型缺失或只部分覆盖的场景、组件和关键状态生成可操作的缺口报告。
- 同步受影响的 GUI 文档、样式架构说明、设计真源说明与链接。

预期前端范围：

- `crates/rozsa-gui/frontend/index.html`
- `crates/rozsa-gui/frontend/sidebar.html`
- `crates/rozsa-gui/frontend/app.js`
- `crates/rozsa-gui/frontend/sidebar.js`
- `crates/rozsa-gui/frontend/gui_shared.js`
- `crates/rozsa-gui/frontend/styles/**`
- 对应的 `crates/rozsa-gui/tests/*`
- `docs/gui/**` 中受影响的规范和架构文档
- 项目级 GUI 真源与维护约定

Rust 文件默认不在范围内。若原型已明确要求的界面数据无法通过现有 IPC 获得，必须先
提交缺失字段、现有 payload/command/event 无法满足的证据、最小契约变化、文件范围、
错误路径和跨平台影响，并获得用户明确批准。批准后先修订 SPEC 与 Dev Flow task 文件
范围，才可修改 `crates/rozsa-gui/src/`；该修改只补充数据契约，不改变产品行为，并需要
独立的 Rust 数据契约与错误路径测试。

### Out

- 不重新设计、补画、推导或自行实现原型未覆盖的可见组件与状态。
- 不调整原型已经确定的颜色、字体、间距、圆角、阴影、模糊、层级、动效、文案、图标、
  DOM 构图或响应式行为。
- 不以代码整洁、语义化、组件复用或 DESIGN 文档解释为理由改变原型实际效果。
- 不在本迁移中重构业务状态、Tauri IPC、session 模型、权限策略、Dev Flow 数据模型或
  native window 架构。
- 不把原型演示 fixture、硬编码结果或无真实行为的 stub 当作生产数据逻辑。
- 不修改测试来掩盖实现回归；只有现有测试与本规格明确冲突时，先请求用户确认。
- 本 SPEC 不进行任务拆解、代码实现、完整 TEST 阶段或提交。

## Requirements Trace

| Requirement | Source | Design response |
| --- | --- | --- |
| 原型 CSS/HTML/JS 是实现母版 | 用户明确要求 | 原型来源映射、受限差异清单、同视口对照验收 |
| 只做必要抽象与位置搬移 | 用户明确要求 | 保持声明与结构，抽取仅解决 runtime 文件边界和复用 |
| 不改变实际业务逻辑 | 用户明确要求 | 使用薄适配层，保留 handler、状态与 IPC 契约 |
| 不改变已实现的视觉语言 | 用户明确要求 | 禁止自行优化；视觉差异必须先获得批准 |
| 缺失设计由用户补充原型 | 用户明确要求 | 缺口阻断协议，不允许实现方自行推导 |
| CSS 必须外置并可复用 | 已确认项目约束 | 原型 CSS 原样拆入现有 foundations/layout/components/features 层 |
| 功能迁移不得退化 | 用户要求与现有测试 | 行为契约矩阵、最小专项测试、最终 TEST 阶段 |

## Design

### 1. Source-of-truth contract

具体实现的优先级如下：

1. 用户对当前迁移的明确要求。
2. `docs/gui/new-version/` 中实际存在的 CSS、HTML 和 JavaScript。
3. `DESIGN.md`，用于解释和一致性检查，但不能覆盖原型已经实现的具体细节。
4. 现有 runtime 业务行为、状态与 IPC 契约。
5. 旧 GUI 文档和旧 prototype；迁移后应同步更新，不得反向覆盖新原型。

若原型与 runtime 在视觉结构上冲突，原型决定视觉，runtime 决定业务行为。若两者无法在
不改变任一契约的情况下连接，停止该表面的实现并提交冲突报告，不使用隐式 fallback。

### 2. Coverage inventory and migration gate

实施前盘点所有可见 runtime 表面及其关键状态。inventory 的权威输入是以下集合的并集：

- `index.html`、`sidebar.html` 中的 scene root、template、panel、dialog 和 popover。
- `app.js`、`sidebar.js` 中所有创建、替换、显示、隐藏或切换可见 DOM 的 renderer/handler。
- Tauri 前端事件、IPC 结果与错误分支可触发的可见状态。
- native split、sidebar fallback、scene router 和跨 WebView 状态。
- `crates/rozsa-gui/tests/` 已编码的 GUI 场景与状态契约。
- `docs/gui/new-version/scenes/` 中的原型场景和 variant。

矩阵必须双向闭合：每个 runtime renderer/state branch 都映射到一个 coverage item；每个原型
场景/variant 都映射到 runtime 目标或明确标记为 demo-only。存在未映射 renderer、可见状态
分支、原型 variant 或测试场景时，inventory 不完整，不能通过验收。

每个 coverage item 以“可见 surface + state variant”为最小身份，并处于以下状态之一：

- `covered`：原型明确展示 DOM、视觉和必要交互状态，可以迁移。
- `partial`：原型有组件但缺少会影响实现的关键状态，默认阻断整个共享组件族迁移。
- `missing`：原型没有该场景或组件，阻断该组件迁移。
- `non-visual`：只涉及既有业务或数据接线，不需要新增视觉决定。

覆盖矩阵至少记录：surface、variant、runtime 入口、触发条件、状态集合、原型来源文件和
定位、目标文件、适配要求、现有测试和验收证据。只有 `covered` 与 `non-visual` 项可以进入
实现。仅当 variant 使用独立 DOM/CSS、不会改变未覆盖状态、不会产生新旧样式混合、能够
独立验收且获得用户明确同意时，才允许把 `partial` 组件拆成独立 coverage item 分阶段迁移。

### 3. Fidelity-preserving extraction

原型迁入 runtime 时遵守以下规则：

- CSS declaration、custom property、selector intent、media query、animation keyframe 和
  cascade precedence 必须保持。拆分前为原型连续 rule block 记录原始 source order。
- 场景内稳定 `<style>` 规则搬入外部 CSS；运行时动态值仍可通过 style property 写入。
- 按现有 `tokens → reset/base → layout → components → features → utilities/late state`
  责任拆分；只在不改变跨块覆盖关系的连续边界拆文件。`main.css`、`sidebar.css` 的 import
  组装必须重现原始相对顺序，拆分不能改变 specificity、source order 或 computed style。
- 迁移前后检查重复 selector、相同 specificity 的覆盖和跨场景 selector 碰撞。若隔离规则
  必须新增 scope selector、改变 specificity 或重新排序，停止该表面的迁移并提交冲突报告。
- 原型公共 HTML 可以抽成 template 或 renderer，但渲染结果必须与原型结构等价。
- 原型 JavaScript 可以按职责搬移；事件时序、visible state、focus、pin、expand、hover、
  reduced-motion 和 narrow-layout 行为必须保持。
- 禁止通过重新实现一个“相似组件”替代原型代码。

每个目标模块维护原型来源映射。允许差异仅限：文件路径、模块边界、去除 demo fixture、
真实数据绑定、Tauri 环境接线，以及不改变视觉的稳定标识和可访问性绑定。其他差异都需要
用户批准。

### 4. Runtime adapter boundary

运行时连接采用单向薄适配边界：

```text
existing product state / Tauri events / IPC
                    ↓
         non-visual data adapter
                    ↓
     prototype-derived renderer and behavior
                    ↓
          prototype-derived styles
```

适配层只允许：

- 字段重命名和数据形状转换。
- 把 demo fixture 替换为真实 payload。
- 把原型动作连接到现有 handler 或 IPC。
- 把现有 runtime state 映射到原型已经定义的 variant。

适配层禁止选择新颜色、class、布局、动效、文案或未在原型中定义的 fallback variant。
原型没有对应 variant 时进入缺口协议。

状态所有权分为三类：

- 产品状态：session、streaming、permission result、queue、Dev Flow payload 等继续由现有
  runtime state/IPC 唯一拥有。原型 action 只调用现有 handler/IPC，不把 DOM 当作最终状态。
- 视觉瞬态：hover、popover 展开、临时 focus、原型已定义的纯视觉 pin/animation 可以由
  prototype-derived behavior 直接维护，不写回产品状态。
- 可恢复 UI 状态：scroll、selection、tool/thinking 展开和 permission UI progress 继续由
  现有 session-specific UI state 管理。

产品 action 的生命周期固定为：原型交互 → 现有 handler/IPC → runtime 更新产品状态 →
event/snapshot 回传 → prototype-derived renderer 更新 DOM。仅保留现有产品已经具备的
optimistic update，不因迁移新增 optimistic 业务状态。异步成功、失败、取消和乱序回传继续
使用现有 session identity/revision 规则处理；不得注册重复 handler 或建立第二份产品状态。

### 5. Runtime behavior preservation

首轮迁移尽量保留现有 DOM ID、`data-od-id`、handler 名称、event 名称、IPC payload 和
state key。若原型结构要求调整 renderer，业务状态获取与更新逻辑仍保持独立，不与视觉
markup 重写混合。

以下行为属于不可退化契约：

- main/sidebar 两个持久 WebView 及 native scene root identity。
- session 切换时 draft、caret/selection、scroll、tool/thinking 展开状态和权限进度恢复。
- IME composition 期间的 scene 延迟切换。
- send、stream、queue、steer、abort、fork、subagent 和附件行为。
- tool evidence、turn diff、Markdown、代码复制与横向滚动。
- permission、trust、deny hint 和 ask-user-question 的键盘及错误路径。
- settings、theme、model、capabilities、permissions、keyboard shortcuts 和 Dev Flow 行为。
- notification、quota、sidebar collapse/reveal 和跨平台 fallback。

### 6. Missing-prototype protocol

遇到 `partial` 或 `missing` 项时，不实现视觉。`partial` 默认阻断整个共享组件族；只有满足
§2 的独立性条件并获得用户明确同意时，才允许迁移其中已覆盖的独立 variant。缺口报告至少
包含：

- 场景或组件名称。
- runtime 文件、函数和 DOM 入口。
- 真实触发条件与 payload 形状。
- 已知 normal、empty、loading、running、disabled、error、long-content、narrow 等状态。
- 现有行为和相关测试。
- 可复现步骤；需要时提供存放于项目 `tmp/` 的当前运行时截图。
- 需要用户在 `docs/gui/new-version/` 补充的场景、组件或 variant 清单。
- 被阻断的迁移表面以及不受影响、仍可继续的部分。

用户补齐原型后重新核对，只有状态变为 `covered` 才继续。实现方不提交建议视觉稿，除非
用户另行明确要求。

### 7. Deterministic fidelity evidence

每个 `covered` 场景使用固定 fixture、初始状态、viewport、theme、DPR、font availability、
reduced-motion 设置和捕获时点。动画在原型明确的稳定帧捕获，或在原型支持的 reduced-motion
状态比较。原型与 runtime 必须在同一机器和等价 WebView 环境中对照。

验收证据分为三层：

- DOM：关键节点层级、class、顺序、属性和 visible state。
- Computed style：字体、颜色、尺寸、间距、radius、shadow、blur、position、z-index 及该
  场景的关键交互属性。
- Screenshot：相同内容区域的原型图、runtime 图和差异图。

系统窗口阴影、native titlebar、字体抗锯齿等宿主差异必须单独列入差异清单，不得用于豁免
产品内容区域的差异。所有非允许差异都需要修复或取得用户批准；不使用单一全局像素阈值
替代 DOM、computed style 和截图三层判断。

### 8. Platform verification policy

本轮采用阶段性平台策略 B：

- macOS 是本轮完整视觉验收平台，覆盖原型/runtime 同环境对照和 native split。
- Linux/Windows 本轮必须通过共享 HTML/CSS/JS、renderer、keyboard/focus、IPC、无横向溢出
  和平台 fallback 契约验证。
- 在相应真实环境建立前，Linux/Windows 视觉验收明确标记为
  `pending platform visual verification`，不得描述为已经完成或通过。
- 用户提供 Linux/Windows 环境后，补做字体 fallback、WebView blur、sidebar fallback、
  file picker 周边界面和关键场景截图验证，再关闭对应 pending 项。

### 9. Likely file placement

- Design values：`styles/tokens.css`
- Shared normalization：`styles/reset.css`、`styles/base.css`
- Window geometry：`styles/layout/`
- Reusable controls/surfaces：`styles/components/`
- Conversation、tools、settings、sidebar、appearance、Dev Flow：`styles/features/`
- Main/sidebar stylesheet entry：`styles/main.css`、`styles/sidebar.css`
- Static scene roots：`index.html`、`sidebar.html`
- Main runtime render/behavior：`app.js`
- Sidebar runtime render/behavior：`sidebar.js`
- Cross-WebView scene/theme mechanics：`gui_shared.js`

Dev Flow runtime 若需独立 feature stylesheet，可以新增文件，但其内容必须来自最新的
`docs/gui/new-version/scenes/dev-flow-runtime.html` 和公共原型 CSS，不得重新设计。

### 10. Documentation synchronization

迁移实现必须同步：

- GUI 设计真源指向 `DESIGN.md` 与 `docs/gui/new-version/`。
- `docs/gui/UI_USAGE_GUIDELINES.md`
- `docs/gui/ARCHITECTURE.md`
- `docs/gui/TERMINOLOGY.md` 和前端术语文档（若结构术语变化）
- `docs/gui/DEV_FLOW_INTEGRATION.md`
- `docs/gui/themes.md`
- `frontend/styles/README.md`
- 项目级维护约定和 Related Docs/backlinks

文档只描述实际完成的迁移，不提前宣称缺失表面已符合新设计。

## Acceptance

- SPEC-AC-001: 覆盖矩阵与 §2 定义的全部权威输入双向闭合；所有 runtime renderer、可见
  state branch、原型 variant 和相关测试场景均已映射，且每个 surface/variant 标记为
  `covered`、`partial`、`missing` 或 `non-visual`，没有未分类项。
- SPEC-AC-002: 只有 `covered` 和 `non-visual` 项被实现；每个 `partial`/`missing` 项都有符合
  本规格的缺口报告。`partial` 默认阻断共享组件族，任何 variant 级迁移均满足独立性条件并
  有用户批准，不存在实现方自行补画、推导或混合新旧样式的生产视觉。
- SPEC-AC-003: 每个迁移后的 HTML/CSS/JS 模块均能追溯到具体原型来源；除允许差异清单外，
  没有未经批准的结构、视觉或交互变化。
- SPEC-AC-004: runtime HTML 继续只加载外部 stylesheet entry，不包含稳定 `<style>` 或静态
  `style` 视觉规则；CSS import 存在且无循环；原型连续 rule block 的相对 source order、
  specificity 和 computed style 保持，重复 selector 与跨场景碰撞均有检查结果。
- SPEC-AC-005: 每个 `covered` 场景具有固定 fixture、viewport、theme、DPR、font、motion 和
  捕获时点；原型与真实 runtime 通过 DOM、关键 computed style、截图/差异图三层对照，且
  任何宿主差异或非允许差异均有明确记录与用户批准。
- SPEC-AC-006: 原型定义的 hover、focus、active、selected、expanded、pinned、disabled、
  reduced-motion 和 narrow-layout 行为在 runtime 中保持。
- SPEC-AC-007: 产品状态、视觉瞬态和可恢复 UI 状态分别由 §4 指定的唯一 owner 管理；迁移
  未新增重复 handler、第二份产品状态或 optimistic 业务状态，且未改变现有 handler、事件、
  IPC、异步失败/取消/乱序处理的可观察语义。
- SPEC-AC-008: main/sidebar WebView identity、scene continuity、IME、focus、selection、scroll
  和 session-specific UI state 继续通过既有契约测试。
- SPEC-AC-009: Light/Dark 主题和共享跨平台路径保持相同组件结构、状态语义与现有功能；
  macOS 完成完整视觉与 native split 验收；Linux/Windows 完成共享契约和 fallback 验证，
  并在真实环境验证前明确保持 `pending platform visual verification`，不得宣称视觉已通过。
- SPEC-AC-010: GUI 文档、样式架构说明、设计真源、术语及 backlinks 与实际迁移结果同步，
  且未把未迁移表面描述为已完成。
- SPEC-AC-011: 每个实现任务先运行最小相关测试并通过 `dow task done` 验证；全部任务完成后，
  只有在用户明确进入 `/test` 时才运行全项目 TEST 阶段。
- SPEC-AC-012: 验收所需的截图、diff 和临时报告仅存放在项目 `tmp/`，不写入项目外临时目录。
- SPEC-AC-013: 任何 Rust/IPC 扩围均具有现有契约不足的证据、最小变更设计、用户明确批准、
  已更新的 SPEC/task 文件范围以及独立的数据契约、错误路径和跨平台测试；否则没有 Rust
  生产代码变化。

## Risks

- 原型场景复用整页 markup，抽取时可能意外改变 selector specificity 或 source order。
  缓解：记录连续 rule block 的原始顺序，只在安全连续边界拆分，并比较 computed style；
  必须改变 scope/specificity 才能隔离时停止并报告。
- 原型 demo JavaScript 与真实异步状态时序不同。缓解：只用薄适配层替换 fixture，现有
  runtime 状态机继续拥有产品状态，按 command/event/render 生命周期接线，不新增第二份状态。
- 部分 runtime 状态在原型中不可见。缓解：迁移前完成覆盖矩阵，缺失项按协议阻断。
- native split 的两个 WebView 与原型单页结构存在宿主差异。缓解：保持 scene identity 和
  event routing，分别验证 main/sidebar，不复制业务状态。
- 字体渲染和系统 blur 可能造成跨平台截图差异。缓解：固定对照环境并结合 DOM、computed
  style 和截图验证；宿主差异独立记录，不用单一全局像素阈值掩盖产品差异。
- Linux/Windows 真实视觉环境暂未提供。缓解：本轮执行共享契约与 fallback 验证并保持
  `pending platform visual verification`；环境可用后才能关闭对应视觉验收项。
- 原型要求的数据可能超出现有 IPC。缓解：先提交证据与最小契约方案，只有用户批准并更新
  SPEC/task 范围后才能扩展 Rust，禁止使用占位值或静默隐藏组件。
- 为满足旧测试而保留旧视觉结构可能违背原型。缓解：先区分业务契约测试与旧视觉断言；
  若测试确实与新原型冲突，提交证据并请求用户确认后再调整测试。

## Test Plan

- 迁移前：从 §2 的全部权威输入生成双向覆盖矩阵；检查未映射 renderer/state/variant/test；
  运行受影响功能的最小既有测试，记录基线和预先存在的失败。
- 结构验证：检查 runtime 无稳定 inline CSS、stylesheet import 完整无环、scene roots 未重建、
  关键 runtime 标识和事件绑定存在；核对连续 rule block source order、specificity、重复
  selector 和跨场景碰撞。
- 视觉验证：对每个 `covered` 场景固定 fixture、viewport、theme、DPR、font、motion 与捕获
  时点，分别渲染原型与 Tauri runtime；覆盖桌面、`760px` 窄屏、Light/Dark、reduced motion，
  保存 DOM、关键 computed style、原型截图、runtime 截图和差异图。
- 交互验证：逐项验证原型已有的展开、pin、hover、focus、popover、dialog、settings tab、
  permission/question、notification 和 Dev Flow runtime 行为。
- 功能验证：按受影响模块运行 `crates/rozsa-gui/tests/` 中的专项测试；自引入的 bug 添加独立
  regression test，不修改无冲突的现有契约。
- 原生验证：使用 `./run.sh --prepare-only` 检查 bundle；需要人工运行时使用 `./run.sh`，
  验证完成后立即关闭测试应用。
- 平台验证：macOS 完成全量视觉与 native split 验收；Linux/Windows 本轮运行共享 DOM/CSS、
  renderer、keyboard/focus、IPC、overflow 和 fallback 契约验证，并保持
  `pending platform visual verification`，等待用户提供真实环境后补做平台视觉验收。
- 数据契约验证：若经批准扩展 Rust/IPC，先运行新增数据契约、错误/取消路径和适用平台专项
  测试，再运行依赖该数据的 GUI 验收。
- 最终验证：任务全部完成后询问用户是否进入 `/test`；仅经明确授权后运行 `cargo build`、
  `cargo clippy`、`cargo test` 和 `cargo fmt --all -- --check`。

## Self Check

- [x] Goal is clear
- [x] Scope is clear
- [x] Acceptance criteria are testable
- [x] Critical boundaries and failure paths are defined
- [x] Prototype gaps have an explicit stop-and-report path
- [x] Business logic and visual implementation boundaries are separated
- [x] Matches current quick mode
