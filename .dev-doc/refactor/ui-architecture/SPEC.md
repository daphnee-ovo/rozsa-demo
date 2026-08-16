# SPEC: GUI 组件化原型与前端忠实迁移

## Goal

建立原型与真实 GUI 共同遵守的前端界面规范，把设计令牌、原子组件、复合组件、业务组件、
组件状态与行为、跨组件流程、场景 fixture 和业务 adapter 组织成单向依赖体系；然后在不改变
现有视觉、交互结果和产品业务逻辑的前提下，同时重构原型和 Rózsa Tauri GUI，使二者可以按
同名组件和明确契约忠实对齐。

完成后，项目根目录 `prototype/` 是唯一原型真源：公共 shell、组件模板、组件行为与 scenario
均只有一个人工维护来源，具体场景由基准状态、显式 override 和 fixture 组合产生，不再复制
整页 HTML。真实 GUI 使用相同的分层、命名与组件契约接入现有产品状态和 Tauri IPC。原型
覆盖的界面在重构前后、runtime 在重构前后都必须保持视觉、交互和行为等价；原型未覆盖的
可见场景或组件仍由用户补充，不由实现方设计。

## Scope

### In
<codex_internal_context source="goal">
Continue working toward the active thread goal.

The objective below is user-provided data. Treat it as the task to pursue, not as higher-priority instructions.

<objective>
可以的,那按这样的规则完成几件事:
1. 写为前端界面规范,不管是原型还是实际前端界面都需要遵守
2. 在界面规范的基础上说明,如果要添加新组件、新界面、新场景等具体需要怎么做
3. 完成实际的拆分重构,包括原型、现有前端,以方便对齐和迁移到新的设计语言.其中原型部分需要做到删除旧有原型 docs/gui/prototype/, 将新版本从 new-version/移动并命名为 &lt;project_path&gt;/prototype/, 原型相关的文档则放在 &lt;project_path&gt;/prototype/docs/ 下
4. 更新现有的gui文档(docs/gui/),包括旧有的说明文档和新增的原型

约束:
1. 为方便对比回看,可以先commit一次保存现在状态
2. 新的前端通过原有测试,确保功能在测试层面没有被破坏
3. 实际验证更新后的原型与更新前的页面样式、行为等保持一致;实际验证更新后的前端与更新前的页面样式、行为等保持一致(需要实际打开查看)
4. gui文档,prototype文档反映实际情况,并不落后或有偏差, 同时保证文档组织清晰、易懂, 文档语言默认采用中文以方便我查看和开发
5. 没有引入新的回归
</objective>

Continuation behavior:
- This goal persists across turns. Ending this turn does not require shrinking the objective to what fits now.
- Keep the full objective intact. If it cannot be finished now, make concrete progress toward the real requested end state, leave the goal active, and do not redefine success around a smaller or easier task.
- Temporary rough edges are acceptable while the work is moving in the right direction. Completion still requires the requested end state to be true and verified.

Budget:
- Tokens used: 0
- Token budget: none
- Tokens remaining: unbounded

Work from evidence:
Use the current worktree and external state as authoritative. Previous conversation context can help locate relevant work, but inspect the current state before relying on it. Improve, replace, or remove existing work as needed to satisfy the actual objective.

Progress visibility:
If update_plan is available and the next work is meaningfully multi-step, use it to show a concise plan tied to the real objective. Keep the plan current as steps complete or the next best action changes. Skip planning overhead for trivial one-step progress, and do not treat a plan update as a substitute for doing the work.

Fidelity:
- Optimize each turn for movement toward the requested end state, not for the smallest stable-looking subset or easiest passing change.
- Do not substitute a narrower, safer, smaller, merely compatible, or easier-to-test solution because it is more likely to pass current tests.
- Treat alignment as movement toward the requested end state. An edit is aligned only if it makes the requested final state more true; useful-looking behavior that preserves a different end state is misaligned.

Completion audit:
Before deciding that the goal is achieved, treat completion as unproven and verify it against the actual current state:
- Derive concrete requirements from the objective and any referenced files, plans, specifications, issues, or user instructions.
- Preserve the original scope; do not redefine success around the work that already exists.
- For every explicit requirement, numbered item, named artifact, command, test, gate, invariant, and deliverable, identify the authoritative evidence that would prove it, then inspect the relevant current-state sources: files, command output, test results, PR state, rendered artifacts, runtime behavior, or other authoritative evidence.
- For each item, determine whether the evidence proves completion, contradicts completion, shows incomplete work, is too weak or indirect to verify completion, or is missing.
- Match the verification scope to the requirement's scope; do not use a narrow check to support a broad claim.
- Treat tests, manifests, verifiers, green checks, and search results as evidence only after confirming they cover the relevant requirement.
- Treat uncertain or indirect evidence as not achieved; gather stronger evidence or continue the work.
- The audit must prove completion, not merely fail to find obvious remaining work.

Do not rely on intent, partial progress, memory of earlier work, or a plausible final answer as proof of completion. Marking the goal complete is a claim that the full objective has been finished and can withstand requirement-by-requirement scrutiny. Only mark the goal achieved when current evidence proves every requirement has been satisfied and no required work remains. If the evidence is incomplete, weak, indirect, merely consistent with completion, or leaves any requirement missing, incomplete, or unverified, keep working instead of marking the goal complete. If the objective is achieved, call update_goal with status "complete" so usage accounting is preserved. If the achieved goal has a token budget, report the final consumed token budget to the user after update_goal succeeds.

Blocked audit:
- Do not call update_goal with status "blocked" the first time a blocker appears.
- Only use status "blocked" when the same blocking condition has repeated for at least three consecutive goal turns, counting the original/user-triggered turn and any automatic goal continuations.
- If the user resumes a goal that was previously marked "blocked", treat the resumed run as a fresh blocked audit. If the same blocking condition then repeats for at least three consecutive resumed goal turns, call update_goal with status "blocked" again.
- Use status "blocked" only when you are truly at an impasse and cannot make meaningful progress without user input or an external-state change.
- Once the blocked threshold is satisfied, do not keep reporting that you are still blocked while leaving the goal active; call update_goal with status "blocked".
- Never use status "blocked" merely because the work is hard, slow, uncertain, incomplete, or would benefit from clarification.

Do not call update_goal unless the goal is complete or the strict blocked audit above is satisfied. Do not mark a goal complete merely because the budget is nearly exhausted or because you are stopping work.

</codex_internal_context>
- 编写中文 `docs/gui/FRONTEND_INTERFACE_STANDARD.md`，作为原型和真实前端共同遵守的
  规范；静态架构只引用 §3 唯一依赖图，动态架构采用
  §3 唯一动态依赖图。
- 编写中文、可执行的 `docs/gui/FRONTEND_CHANGE_GUIDE.md`，说明新增 token、原子组件、复合
  组件、业务组件、状态、交互、flow、fixture、scenario 和页面时应修改哪些文件、满足哪些
  边界并完成哪些验证。
- 删除旧 `docs/gui/prototype/`，把当前 `docs/gui/new-version/` 重构并迁移到项目根目录
  `prototype/`；原型专属说明统一放入 `prototype/docs/`，不存在兼容入口或并行原型真源。
- 原型使用单一 shell 和原生 HTML/CSS/JavaScript 组件系统，不新增前端框架或构建依赖；
  独立场景通过 registry、`prototype/mock/` 数据和显式状态组合加载，必要的独立 HTML 只能是
  薄入口或生成物。
- 原型与 runtime 都按 §3 的唯一依赖图重组 CSS；保持最终级联、声明值、selector 语义、
  状态表现和响应式结果不变。
- 将重复的原型 DOM 抽成唯一 template/renderer；将单体交互 JavaScript 按 core、components、
  features、flows、adapters 拆分，只搬移和显式化既有行为；mock 与 scenario 是声明式 JSON，
  其加载和校验逻辑属于 core/prototype adapter，不形成脚本实现层。
- 真实 GUI 采用同名组件边界和单向依赖，保留现有产品状态所有权；原型 mock adapter 与
  runtime Tauri adapter 共享语义事件契约，但不得复制产品业务状态或模拟生产逻辑。
- 原型为呈现真实数据形状而需要的全部 mock 数据、响应和错误结果只存放在 `prototype/mock/`；
  runtime frontend、Tauri 代码、production manifest 与打包产物不得导入、复制或 fallback 到
  这些 mock。
- 保留现有 session continuity、IME、scroll、selection、expanded state、permission progress、
  native scene identity 和双 WebView 生命周期行为。
- 建立“运行时可见表面 → 原型场景/组件 → 目标文件 → 测试”的覆盖矩阵。
- 对原型缺失或只部分覆盖的场景、组件和关键状态生成可操作的缺口报告。
- 同步受影响的 GUI 文档、样式架构说明、设计真源说明与链接。

预期前端范围：

- `prototype/**`
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
- 不以代码整洁、语义化、组件复用或 DESIGN 文档解释为理由改变原型或 runtime 的实际效果；
  本轮允许为建立已批准的组件边界而移动规则、抽取模板、增加组合 class、拆分函数和建立
  语义事件，但重构前后渲染结果与可观察交互必须等价。
- 不在本迁移中重构业务状态、Tauri IPC、session 模型、权限策略、Dev Flow 数据模型或
  native window 架构。
- 不把原型演示 fixture、硬编码结果或无真实行为的 stub 当作生产数据逻辑。
- 不修改测试来掩盖实现回归；只有现有测试与本规格明确冲突时，先请求用户确认。
- SPEC 阶段不进行任务拆解、代码实现或完整 TEST 阶段；实施提交按 task 边界进行。

## Requirements Trace

| Requirement | Source | Design response |
| --- | --- | --- |
| 原型 CSS/HTML/JS 是实现母版 | 用户明确要求 | 原型来源映射、受限差异清单、同视口对照验收 |
| 只做必要抽象与位置搬移 | 用户明确要求 | 保持声明与结构，抽取仅解决 runtime 文件边界和复用 |
| 不改变实际业务逻辑 | 用户明确要求 | 使用薄适配层，保留 handler、状态与 IPC 契约 |
| 不改变已实现的视觉语言 | 用户明确要求 | 禁止自行优化；视觉差异必须先获得批准 |
| 缺失设计由用户补充原型 | 用户明确要求 | 缺口阻断协议，不允许实现方自行推导 |
| CSS 必须外置并可复用 | 已确认项目约束 | 原型与 runtime CSS 按 §3 唯一依赖图归属并由 entry 统一装配 |
| 静态组件必须层层组合 | 用户补充要求 | 使用 §3 唯一静态依赖图，组件语义层与装配层分离 |
| 动态交互也必须复用 | 用户补充要求 | 使用 §3 唯一动态依赖图，禁止 inline handler 契约 |
| 基准场景变化不复制整页 HTML | 用户补充要求 | 单一 shell、场景 registry、mock composition 与一层显式 base |
| 原型 mock 与真实前端严格隔离 | 用户补充要求 | 全部 mock 数据进入 `prototype/mock/`，runtime 与构建产物零引用 |
| 原型迁至项目根目录 | 用户明确要求 | 删除旧原型目录，唯一真源为 `prototype/`，说明进入 `prototype/docs/` |
| 原型与真实前端遵守同一规范 | 用户明确要求 | 新增统一前端界面规范、同名组件边界和新增界面工作流 |
| 重构前后实际等价 | 用户明确要求 | 原型与 runtime 分别建立浏览器/应用基线并执行实机对照 |
| 功能迁移不得退化 | 用户要求与现有测试 | 行为契约矩阵、最小专项测试、最终 TEST 阶段 |

## Design

### 1. Source-of-truth contract

具体实现的优先级如下：

1. 用户对当前迁移的明确要求。
2. 重构前为 `docs/gui/new-version/` 中实际存在的 CSS、HTML 和 JavaScript；目录迁移完成后
   唯一实现真源切换为项目根目录 `prototype/`。
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
- 重构前 `docs/gui/new-version/scenes/`、重构后 `prototype/scenarios/`、`prototype/mock/`
  和组件 gallery 中的原型场景、状态与 variant。

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

### 3. Unified frontend architecture

原型与 runtime 共同采用以下唯一静态依赖图，禁止反向依赖和跨层覆盖：

```text
foundations → primitives → composites → features ─┐
      └────────────────→ layouts ─────────────────┼→ scenes → entries
                                                   ┘
```

- `foundations`：颜色、字体、间距、尺寸、圆角、阴影、motion 和 breakpoint；不包含组件。
- `primitives`：Surface、Button、Input、Choice、Icon、Status 等单一职责控件，只依赖
  foundations。
- `composites`：Composer、ComposerExtension、ReplacementPanel、SelectionPanel、Popover、
  Notification、ToolCall、SettingsField 等结构组合，只依赖 primitives 与 foundations。
- `features`：Queue、Steering、PermissionRequest、AskUserQuestion、Conversation、Settings、
  Sidebar、Dev Flow 等业务表面，只通过公共 slot 和 variant 扩展 composite。
- `layouts`：页面与窗口几何、稳定 slot 和 responsive arrangement，只依赖 foundations；不得
  定义控件视觉或进入 feature 内部。
- `scenes`：只组合 features、layouts、fixture 和页面级状态；不得定义可复用组件或修改组件
  内部 selector。runtime 即使没有独立 scene CSS 文件，也必须在 registry 中声明 scene composition。
- `entries`：纯装配层，只按 registry 的稳定顺序 import；不得包含 selector、declaration 或
  业务逻辑，也不能被其他层依赖。

CSS 的概念依赖记录在 component registry，由 entry 按稳定顺序统一 import；组件 CSS 不层层
`@import` 依赖，避免重复加载和隐式 cascade。每个视觉规则必须具有唯一逻辑 owner。一个 owner
可以拆为基础文件与同目录的 `*.responsive.css` source fragment；responsive fragment 仍属于该
owner，不构成 utilities 或 late-state 层。foundations 级全局 reduced-motion 规则进入
`foundations/motion.css`。

迁移期间以 rule block 为单位维护 source-order ledger。相同 selector、相同 specificity override、
custom property、animation/keyframe 及选择器可能相交的 block 均为 order-sensitive，entry 必须
保持其原始相对顺序。只有经机械检查确认 selector 不相交、无共享 custom property/keyframe 且
不参与同一 cascade 的 block 才可按 owner 聚合；这类重排仍需后续 DOM、computed style 与像素
证据证明等价。不能证明可交换时保持原顺序，不以目录整洁为理由重排。

最终结构不保留笼统 `utilities` 或 `late-state` 语义层：现有此类规则必须按实际 owner 归入
foundations、primitive、composite、feature、layout 或 scene。迁移期间为了维持 source order
可以保留临时 late import，但对应 task 关闭前必须完成 owner 归类；responsive、motion、state
rule 跟随其组件 owner，而不是形成跨组件覆盖文件。

动态依赖方向如下：

```text
component state → component behavior → semantic event → flow coordinator → adapter
```

- component state 是显式有限状态/variant，不以任意 class、ID 或 DOM 内容充当隐藏产品状态。
- component behavior 只处理自身 DOM、ARIA、键盘、focus 和视觉瞬态，并发出语义事件。
- flow coordinator 处理 Permission 替换 Composer、关闭后恢复焦点等跨组件状态转换。
- prototype adapter 只从 `prototype/mock/` 读取确定性 mock 结果；runtime adapter 连接现有
  handler、event 和 IPC，不得包含 prototype import、mock branch 或 mock fallback。
- adapter 不生成 DOM、不选择视觉 variant、不吞掉失败，也不拥有第二份产品状态。

共享契约只有一个机器真源：`docs/gui/FRONTEND_COMPONENTS.json`，并由
`docs/gui/frontend-component-contract.schema.json` 严格校验。每个组件必须声明 schema version、
ID、layer、state/variant、slot、semantic event、dependencies 和 consumers；同一记录下分别声明
prototype 与 runtime 的 CSS、template/renderer、behavior、adapter、mock/gallery ID 和来源映射。
prototype 与 runtime 不再各自维护完整 component map。未知字段、未知组件、缺失依赖、循环、
反向依赖、consumer 不对称、schema version 不匹配均 fail fast。简单 primitive 可以共用同层
文件；拥有独立状态或行为的 composite/feature 必须具有独立身份。

共享 registry、component contract、gallery、scenario registry 和 fidelity manifest 只能保存
mock ID、schema/version、源路径、digest 和捕获配置，不得内嵌 mock payload、响应或展开后的
组件数据。

### 4. Single shell and scenario composition

原型只有一个人工维护的应用 shell。场景是状态数据而不是页面副本：

```text
scenario = base + mock fixtures + override + flow
```

- 每个 scenario 必须具有 `base`、`fixtures`、`override`、`flow` 四个字段：`base` 为一个 scene ID
  或 `null`，`fixtures` 为指向 `prototype/mock/` 记录的有序 ID 数组，`override` 为对象，`flow`
  为 flow ID 或 `null`。
- 一个 scenario 最多引用一个 base，且被引用的 base 自身不得再引用 base；循环、多 base、缺失
  base/fixture/flow、未知字段均 fail fast。
- 确定性合并顺序为 base snapshot → 按声明顺序应用 fixtures → override。对象递归合并；数组、
  scalar 与 `null` 整体替换；同名 fixture leaf 后者覆盖前者。删除字段必须通过 scenario schema
  定义的 `$unset` path 数组显式完成，不把 `null` 隐式解释为删除。
- `override` 只允许 scenario schema 明确列举的展示状态、component variant、viewport/theme/
  motion capture 参数和 flow 参数；message 内容、tool result、backend payload、data、response、
  error body 等领域值必须通过 mock ID 引入。override 出现未声明路径或 backend-shaped 字段时
  fail fast。
- base、mock fixtures 与 override 合并完成、flow 尚未执行的状态称为 `composed initial state`。
  其 canonical JSON 只能在运行时生成或写入项目 `tmp/`；正式 scenario registry 只保存 initial
  state digest。相同输入在不同运行中必须产生字节一致的 canonical JSON 与 digest，避免隐式
  继承或 merge 实现差异。
- flow 的每个可验收 checkpoint 必须独立声明 state ID、触发 semantic event、等待条件、mock
  response ID 和 expected state digest；成功、失败、取消等分支不能共用一个 final-state digest。
- session、messages、tool results、permission request 等通过 `prototype/mock/` 中的小型 fixture
  组合，不复制 DOM；mock payload 应保持真实 backend payload 的字段形状、长内容、空值和错误
  结果，但不得被描述为真实后端行为。
- scene registry 负责 scene ID、初始 state、fixture、viewport/theme/motion capture config 和 flow。
- renderer 从唯一 template/renderer 产生 DOM；禁止为同一组件维护 scene HTML 与 JS string 两份 markup。
- `prototype/index.html?scene=<id>` 是标准运行入口。若工具必须使用独立场景 HTML，只允许薄入口
  或确定性生成物，且生成来源必须唯一、不可手工修改。
- gallery 逐组件展示 default、hover、focus、active、selected、expanded、disabled、loading、
  empty、error、long-content、narrow、Light/Dark 和 reduced-motion 中原型实际定义的状态。
- 静态 snapshot 场景验证初始状态；interaction flow 验证事件、状态转换、错误、取消和 focus restore。

mock 目录按数据职责组织：

```text
prototype/mock/
├── manifest.json
├── mock.schema.json
├── mock-manifest.schema.json
├── sessions/
├── messages/
├── tools/
├── permissions/
├── settings/
├── notifications/
├── dev-flow/
└── responses/
```

manifest 登记的 mock fixture/response record 统一为 UTF-8 JSON，并符合
`prototype/mock/mock.schema.json`，具有全局唯一、按领域命名的 ID（例如 `sessions/complete`）、
schema version、adapter contract ID 与 `payload`。`prototype/mock/manifest.json` 单独符合
`prototype/mock/mock-manifest.schema.json`，只保存 ID → path、schema、SHA-256 映射，不内嵌
payload；`mock.schema.json` 与 `mock-manifest.schema.json` 是不参与 record 登记的元文件。mock
root 除这三个元文件外的每个 JSON 都必须登记，manifest 也不得指向不存在或越出 mock root 的
文件。重复 ID、未知 ID、schema/version 不匹配或 digest 不一致均 fail fast。共享 adapter
envelope 由 `docs/gui/frontend-adapter-contract.schema.json` 定义；领域 payload 必须由对应
component state/adapter contract 测试验证与真实 backend shape 一致。

所有语义 JSON digest 统一使用 `sha256-jcs-v1`：先按 RFC 8785 JSON Canonicalization Scheme
生成 UTF-8 bytes，再计算 SHA-256。mock record digest 的输入是解析后的完整 record；composed
initial state 与 flow checkpoint digest 的输入是对应完整 state；manifest/schema version 同时
绑定 `canonicalization: "RFC8785"`。不支持该版本、非 JCS 可表示数字或 digest 不一致时 fail
fast。production-copy 检查只对静态 bundled JSON 或可完整提取的静态对象计算同样的完整对象
JCS hash；不以字段名、类型结构或合法 runtime backend response 相同作为复制证据。

mock 文件只能包含确定性数据或声明式响应，不包含组件 DOM、CSS 或产品业务实现。prototype
adapter 把 mock 数据映射到共享 adapter contract；runtime 缺少真实数据时必须透明报错或使用
原型已经覆盖的真实 error variant，不得读取 mock、返回空对象或静默隐藏组件。

原型标准启动协议为本地 HTTP，不支持把 `file://` 作为验收入口：

```text
uv run python -m http.server 4173 --directory prototype
http://127.0.0.1:4173/?scene=<id>
```

端口占用或 mock fetch 失败时直接失败并报告，不自动换端口或返回空数据。文档、截图脚本和
交互验证必须使用同一启动协议。

### 5. Fidelity-preserving extraction

原型迁入 runtime 时遵守以下规则：

- CSS declaration、custom property、selector intent、media query、animation keyframe 和
  cascade precedence 必须保持。拆分前为原型连续 rule block 记录原始 source locator、内容摘要、
  owner、目标 fragment、原始序号和 order-sensitive predecessors。
- 场景内稳定 `<style>` 规则搬入外部 CSS；运行时动态值仍可通过 style property 写入。
- 按 §3 唯一依赖图归类；优先在连续边界拆文件。非连续规则归入同一 owner 时，仅允许跨越经
  ledger 证明可交换的 block。entry 必须保持全部 order-sensitive 关系；responsive fragment 在
  owner 目录中由 entry 显式装配，不建立公共 late import。拆分不能改变 specificity、实际
  cascade precedence 或 computed style。
- 迁移前后检查重复 selector、相同 specificity 的覆盖和跨场景 selector 碰撞。若隔离规则
  必须新增 scope selector、改变 specificity 或重新排序，停止该表面的迁移并提交冲突报告。
- 重复原型 HTML 必须抽成唯一 template 或 renderer，旧场景的最终 DOM 结构、顺序、class、ARIA、
  visible state 与交互结果必须等价；不得为了保留重复文件而建立兼容入口。
- inline event handler 搬入组件 behavior，并用 semantic event 连接 flow；事件时序、visible state、
  focus、pin、expand、hover、reduced-motion 和 narrow-layout 行为必须保持。
- 静态 inline visual style 搬入对应层 CSS；真实动态值使用受组件契约约束的 custom property 或
  state attribute，不允许任意 JS 写入视觉声明。
- 禁止通过重新实现一个“相似组件”替代原型代码。

新增或修改前端界面统一遵循以下落盘流程，prototype 与 runtime 不得各自发明一套结构：

1. 先在 registry 声明组件 ID、owner layer、state、variant、slot、semantic event、依赖与 consumer；
   原型未覆盖的状态先进入 gap，不写实现。
2. 新 primitive/composite 建立唯一 template、CSS owner 和 component behavior；feature 只能通过
   slot/variant 组合，不能穿透覆盖组件内部 selector。
3. 新界面先组合既有 component/feature；只有确有新复用边界时才新增组件。新 scene 只登记
   base、mock fixture ID、override、flow 和 capture config，不复制整页 HTML。
4. 原型需要 backend-shaped 数据时新增 `prototype/mock/` record 与 manifest JCS digest；runtime
   只能接真实 adapter，禁止引用、复制或 fallback 到 mock。
5. 变更同时更新 component registry、scenario registry、coverage/gap 和中文文档，依次运行
   contract、component behavior、flow、surface fidelity 与 production isolation 测试。

每个目标模块维护原型来源映射。允许差异仅限：文件路径、模块边界、去除 demo fixture、
真实数据绑定、Tauri 环境接线，以及不改变视觉的稳定标识和可访问性绑定。其他差异都需要
用户批准。

### 6. Runtime adapter boundary

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

### 7. Runtime behavior preservation

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

### 8. Missing-prototype protocol

遇到 `partial` 或 `missing` 项时，不实现视觉。`partial` 默认阻断整个共享组件族；只有满足
§2 的独立性条件并获得用户明确同意时，才允许迁移其中已覆盖的独立 variant。缺口报告至少
包含：

- 场景或组件名称。
- runtime 文件、函数和 DOM 入口。
- 真实触发条件与 payload 形状。
- 已知 normal、empty、loading、running、disabled、error、long-content、narrow 等状态。
- 现有行为和相关测试。
- 可复现步骤；需要时提供存放于项目 `tmp/` 的当前运行时截图。
- 需要用户在 `prototype/` 补充的场景、组件或 variant 清单。
- 被阻断的迁移表面以及不受影响、仍可继续的部分。

用户补齐原型后重新核对，只有状态变为 `covered` 才继续。实现方不提交建议视觉稿，除非
用户另行明确要求。

### 9. Deterministic fidelity evidence

每个 `covered` 场景使用固定 mock fixture、初始状态、viewport、theme、DPR、font availability、
reduced-motion 设置和捕获时点。动画在原型明确的稳定帧捕获，或在原型支持的 reduced-motion
状态比较。原型与 runtime 必须在同一机器和等价 WebView 环境中对照。

每次基线和复测必须写入 `tmp/gui-component-refactor/<surface>/<phase>/manifest.json`。manifest
逐 case 记录：scene/state ID、mock ID、入口、content crop selector、viewport、DPR、theme、font
清单、motion、等待条件、交互步骤、DOM selector/属性白名单、computed-style selector/property
白名单、prototype/runtime 或 before/after 图片路径、比较算法、阈值、mask 和批准引用。

验收证据分为三层：

- DOM：移除运行时随机 ID/时间戳后，以 canonical JSON 精确比较关键节点层级、class、顺序、
  属性和 visible state；规范化规则必须逐字段写在 manifest，不能临时忽略差异。
- Computed style：字体、颜色、尺寸、间距、radius、shadow、blur、position、z-index 及该
  场景的关键交互属性；统一颜色、浮点精度和字体 fallback 表达后逐属性精确比较。
- Screenshot：按固定 content crop 输出 before、after 和 diff PNG；同一浏览器原型前后默认
  要求像素精确。Tauri WebView 宿主噪声使用逐 case 像素/感知阈值，默认 0；任何非零阈值或
  mask 必须限定区域、说明原因并附用户批准引用，不能复用于未批准 scene。

系统窗口阴影、native titlebar、字体抗锯齿等宿主差异必须单独列入差异清单，不得用于豁免
产品内容区域的差异。所有非允许差异都需要修复或取得用户批准；不使用单一全局像素阈值
替代 DOM、computed style 和截图三层判断。

### 10. Platform verification policy

本轮采用阶段性平台策略 B：

- macOS 是本轮完整视觉验收平台，覆盖原型/runtime 同环境对照和 native split。
- Linux/Windows 本轮必须通过共享 HTML/CSS/JS、renderer、keyboard/focus、IPC、无横向溢出
  和平台 fallback 契约验证。
- 在相应真实环境建立前，Linux/Windows 视觉验收明确标记为
  `pending platform visual verification`，不得描述为已经完成或通过。
- 用户提供 Linux/Windows 环境后，补做字体 fallback、WebView blur、sidebar fallback、
  file picker 周边界面和关键场景截图验证，再关闭对应 pending 项。

### 11. File placement

原型唯一真源：

```text
prototype/
├── index.html
├── styles/{foundations,primitives,composites,features,layouts,scenes,entries}/
├── templates/{primitives,composites,features}/
├── scripts/{core,components,features,flows,adapters}/
├── mock/{manifest.json,mock.schema.json,mock-manifest.schema.json,sessions,messages,tools,permissions,settings,notifications,dev-flow,responses}/
├── scenarios/{registry.json,scenario.schema.json}/
├── gallery/
└── docs/
```

runtime 对应结构位于 `crates/rozsa-gui/frontend/`：

```text
frontend/
├── index.html
├── sidebar.html
├── styles/{foundations,primitives,composites,features,layouts,entries}/
└── scripts/{core,components,features,flows,adapters}/
```

共享 registry 与 schema 位于：

```text
docs/gui/
├── FRONTEND_COMPONENTS.json
├── frontend-component-contract.schema.json
└── frontend-adapter-contract.schema.json
```

允许在迁移期间以现有 `app.js`、`sidebar.js`、`gui_shared.js` 作为兼容中的真实 runtime entry，
但最终组件实现不得继续集中在单体文件；entry 只负责启动、注入 adapter 和连接现有 runtime
state。原型不保留旧目录兼容入口。原型与 runtime 的 component ID、state/variant 名、semantic
event 名和依赖方向必须一致；文件内容不要求共享，因为 prototype 使用 mock adapter，
runtime 使用真实产品状态和 IPC。

现有迁移资产按以下方式处置：

- `docs/gui/new-version/scenes/*.html.artifact.json` 的有效 scene、renderer、export 和 state metadata
  迁入 `prototype/scenarios/registry.json`；旧 artifact 文件不保留。
- `NEW_VERSION_MIGRATION_COVERAGE.{md,json}` 迁入并重命名为
  `prototype/docs/MIGRATION_COVERAGE.{md,json}`。
- `NEW_VERSION_PROTOTYPE_GAPS.md` 迁入并重命名为 `prototype/docs/PROTOTYPE_GAPS.md`。
- `styles/source-order.json` 迁入 `prototype/docs/fidelity/source-order.json`，继续作为原始抽取证据，
  不作为可加载 stylesheet。
- `rozsa-visual-demo.html` 迁入 `prototype/gallery/visual-direction.html`，标记为 `reference-only`；
  它不参与 runtime 迁移，仍需保持重构前后视觉等价，后续是否并入正式组件由用户原型决定。
- `docs/gui/prototype/` 是被替换的旧原型，删除且不迁移其视觉实现；仍有效的历史说明只可在
  新文档中以文字吸收，不能形成第二套加载入口。
- T013 及本轮 fidelity 证据继续仅位于项目 `tmp/`，文档记录验证结论和 manifest 位置规则，
  不把临时截图提交为新的设计真源。

### 12. Documentation synchronization

迁移实现必须同步：

- GUI 设计真源指向 `DESIGN.md` 与项目根目录 `prototype/`。
- 新增 `docs/gui/FRONTEND_INTERFACE_STANDARD.md`，完整记录静态层、动态层、scene composition、
  component registry、状态/事件/flow/adapter 边界和禁止事项。
- 新增中文 `docs/gui/FRONTEND_CHANGE_GUIDE.md`，逐类说明新增组件、页面、场景、状态和交互
  流程的具体步骤。
- `docs/gui/UI_USAGE_GUIDELINES.md`
- `docs/gui/ARCHITECTURE.md`
- `docs/gui/TERMINOLOGY.md` 和前端术语文档（若结构术语变化）
- `docs/gui/DEV_FLOW_INTEGRATION.md`
- `docs/gui/themes.md`
- `frontend/styles/README.md`
- `prototype/docs/` 中的原型架构、场景编写、组件 catalog、交互 flow 与验证说明。
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
- SPEC-AC-007: 产品状态、视觉瞬态和可恢复 UI 状态分别由 §6 指定的唯一 owner 管理；迁移
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
- SPEC-AC-014: `docs/gui/prototype/` 与 `docs/gui/new-version/` 均不存在；唯一原型真源位于
  项目根目录 `prototype/`，所有原型专属说明位于 `prototype/docs/`，项目内不存在指向旧目录
  的有效链接、加载入口或兼容文件。
- SPEC-AC-015: 唯一共享 `FRONTEND_COMPONENTS.json` 通过严格 schema 校验，并覆盖实际
  foundations、primitives、composites、features、layouts、scenes、entries、state contract、
  semantic event、dependency、consumer 以及 prototype/runtime implementation；磁盘文件与
  registry 双向闭合，共享字段不存在双份来源，依赖图无循环、无反向依赖。
- SPEC-AC-016: 原型只有一个人工维护 shell 和一份组件 markup 来源；每个 scenario 严格按照
  §4 schema，由一个 base 或 `null`、有序 mock fixtures、override、flow 或 `null` 生成，场景
  不复制完整 HTML，也不以 JS string 维护第二份同组件 markup；全部 scene ID 可独立打开，
  composed initial state canonical JSON 与各 flow checkpoint 可确定性复现。
- SPEC-AC-017: Composer/Queue/Steering、ReplacementPanel/PermissionRequest/AskUserQuestion 等
  已覆盖组件具有显式 state、variant、slot、semantic event、behavior、flow 与 adapter 边界；
  原型 interaction flow 实际证明键盘、focus、成功、失败、取消和恢复中已覆盖的路径。
- SPEC-AC-018: `docs/gui/FRONTEND_INTERFACE_STANDARD.md` 与
  `docs/gui/FRONTEND_CHANGE_GUIDE.md` 为中文，并能使开发者
  不依赖隐含知识完成新增 token、primitive、composite、feature、state、interaction、flow、
  fixture、scenario 和页面；GUI/原型文档、Related Docs 和 backlinks 与实际文件完全一致。
- SPEC-AC-019: 在任何重构前实际打开原型与 macOS Tauri GUI，保存固定环境的基线；重构后
  再次实际打开二者，逐场景执行 DOM、computed style、截图差异和关键交互对照。原型前后、
  runtime 前后分别无未经批准差异，验证完成后没有遗留运行实例。
- SPEC-AC-020: 原有 `rozsa-gui` 专项测试及受影响的前端结构、scene continuity、IME、session、
  permission/question、settings、notification、Dev Flow、native split 与 platform fallback 测试
  全部通过；任何自引入缺陷都有先失败后通过的回归测试，没有通过修改契约来掩盖回归。
- SPEC-AC-021: 全部原型模拟数据仅位于 `prototype/mock/` 并由 scenario/prototype adapter 按 ID
  引用；`crates/rozsa-gui/frontend/`、Rust production、共享 registry 的 runtime implementation、
  Tauri `frontendDist`/resource allowlist、bundle 输入和最终 app bundle 对 `prototype/**` 零依赖。
  mock manifest/schema/ID/digest 全部有效，正式 registry 不复制 payload；真实数据缺失时 runtime
  不使用 mock、空对象或静默 fallback，并保持既有透明错误语义。

## Risks

- 原型场景复用整页 markup，抽取时可能意外改变 selector specificity 或 source order。
  缓解：记录连续 rule block 的原始顺序，只在安全连续边界拆分，并比较 computed style；
  必须改变 scope/specificity 才能隔离时停止并报告。
- 把隐式并列 selector 抽成显式 primitive/composite class 时，可能错误合并两个只是在当前值上
  相同的业务组件。缓解：先建立共享 component registry 和 consumer 列表，只抽取有共同结构、状态语义
  与交互契约的部分；无法证明共同语义时保持独立 feature。
- 基准场景继承可能形成难以定位的隐藏状态。缓解：每个 scenario 最多一个 base，composed
  initial state 确定性展开后只写入 `tmp/` 独立 artifact；临时 fidelity manifest 保存 artifact
  path 与 digest，正式 registry 只保存 digest；复杂差异使用显式 mock composition，不使用深层继承。
- template loader 或 component renderer 可能改变初始化时序、focus 和持久 root identity。
  缓解：保留稳定 root，只在其内部挂载一次；先捕获基线，再逐组件迁移并执行交互对照。
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

- 重构前：实际打开当前原型全部 scene 和 macOS Tauri GUI；固定 viewport、theme、DPR、font、
  motion、mock fixture 和捕获时点，保存 DOM、computed style、截图、交互步骤、测试结果及运行版本。
- 迁移前：从 §2 的全部权威输入生成双向覆盖矩阵；检查未映射 renderer/state/variant/test；
  运行受影响功能的最小既有测试，记录基线和预先存在的失败。
- 结构验证：检查 runtime 无稳定 inline CSS、stylesheet import 完整无环、scene roots 未重建、
  关键 runtime 标识和事件绑定存在；核对连续 rule block source order、specificity、重复
  selector 和跨场景碰撞；检查 component registry 与磁盘、依赖和 consumer 双向闭合。
- mock 隔离验证：检查 runtime source 的 import/fetch/include 依赖图、Tauri `frontendDist`、resource
  allowlist、bundle manifest 与完成后的 `.app` 均不包含 `prototype/**`；对 mock payload 生成
  `sha256-jcs-v1` 完整对象 hash，仅对静态 bundled JSON 或可完整提取的静态对象检测 production
  复制，不把合法共享 ID、字段结构、文案或 runtime backend response 误判为依赖；实际断开/
  失败 backend 时确认 runtime 进入
  既有错误路径而不是显示原型数据。
- 场景验证：检查只有一个人工维护 shell/markup 来源；全部旧 scene ID 能由 registry 打开；
  每个 scenario 最多一个 base，composed initial state 与 flow checkpoint 可复现，新增场景无需
  复制完整 HTML。
- 视觉验证：对每个 `covered` 场景固定 mock fixture、viewport、theme、DPR、font、motion 与捕获
  时点，分别渲染原型与 Tauri runtime；覆盖桌面、`760px` 窄屏、Light/Dark、reduced motion，
  保存 DOM、关键 computed style、原型截图、runtime 截图和差异图。
- 交互验证：逐项验证原型已有的展开、pin、hover、focus、popover、dialog、settings tab、
  permission/question、notification 和 Dev Flow runtime 行为；按 semantic event → flow → adapter
  路径验证成功、失败、取消、乱序和 focus restore 中原型已覆盖的状态。
- 功能验证：按受影响模块运行 `crates/rozsa-gui/tests/` 中的专项测试；自引入的 bug 添加独立
  regression test，不修改无冲突的现有契约。
- 原生验证：使用 `./run.sh --prepare-only` 检查 bundle；需要人工运行时使用 `./run.sh`，
  实际打开 main/sidebar、操作关键流程并截图，验证完成后立即关闭测试应用。
- 重构后对照：在与基线相同的环境重新打开原型和 runtime，分别比较原型前后、runtime 前后
  的 DOM、computed style、截图和交互记录；不能用原型与 runtime 彼此相似替代各自前后等价。
- 平台验证：macOS 完成全量视觉与 native split 验收；Linux/Windows 本轮运行共享 DOM/CSS、
  renderer、keyboard/focus、IPC、overflow 和 fallback 契约验证，并保持
  `pending platform visual verification`，等待用户提供真实环境后补做平台视觉验收。
- 数据契约验证：若经批准扩展 Rust/IPC，先运行新增数据契约、错误/取消路径和适用平台专项
  测试，再运行依赖该数据的 GUI 验收。
- DEV 最终 GUI 门：不进入全项目 TEST 阶段也必须运行 `cargo test -p rozsa-gui`，据此闭合
  SPEC-AC-020；若存在经证据确认的共享工作树预存失败，必须单独归因并保持可见，不能宣称
  “全部通过”。
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
