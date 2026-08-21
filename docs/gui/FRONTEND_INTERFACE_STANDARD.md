# Rózsa 前端界面规范

本文规定 Rózsa 原型与真实 GUI 共同遵守的前端组织方式。它描述长期有效的职责、依赖、状态、数据接入和验证规则，不记录文件迁移历史。

原型与真实 GUI 使用同一套概念，但各自维护实现，不能互相 import：

- 原型用于设计、演示和重放界面状态，最终真源位于项目根目录 `prototype/`；在真源切换完成前，候选内容仍位于 `docs/gui/new-version/`。
- 真实 GUI 位于 `crates/rozsa-gui/frontend/`，由 Tauri main/sidebar 两个 WebView 加载，并继续使用现有产品状态、handler、event 和 IPC。
- 双方分别与自己的重构前 baseline 比较。原型不是 runtime 的视觉覆盖层，runtime 也不能反向改变原型设计。

## 1. 术语

| 名称 | 本项目含义 |
| --- | --- |
| component | 具有明确 DOM、样式、状态和交互边界的可复用界面单元。 |
| template | component 稳定的 HTML 结构，不包含数据或事件处理。 |
| renderer | 根据数据创建或更新 DOM 的代码；一种稳定结构只能有一个 template 或 renderer 真源。 |
| adapter | 把 prototype mock 或 Tauri 数据转换成 component 输入，不决定结构、视觉或流程。 |
| application shell | 窗口长期存在的骨架与 slot，例如 sidebar、header、main content 和 Composer 区域。 |
| UI state | focus、展开、选中、draft、selection、scroll 等界面状态，不等于产品数据。 |
| scene | 使用 application shell 展示真实应用某个明确时刻的原型配置。 |
| gallery | 集中展示视觉参考或组件变体的原型配置，不代表 runtime 页面。 |
| baseline | 修改前保存、修改后可在相同条件下重放的截图、DOM、computed style 和交互材料。 |
| fixture | 为重复验证准备的固定输入；runtime fixture 仍通过产品契约进入，不是 prototype mock。 |

`scene`、`gallery`、`UI_COVERAGE` 和 `DESIGN_GAPS` 是 Rózsa 的项目约定，不应表述为行业唯一命名。

## 2. 静态层级与依赖方向

前端按下列方向组合：

```text
foundations
  → primitives
    → composites
      → features
        → layouts
          → scenes（prototype only）
            → entries
```

上层可以组合下层；下层不能 import、查询或操纵上层。scene 和 entry 只做装配，不能成为公共组件的第二实现位置。

### 2.1 `foundations`

存放全局基础标准：颜色、字体、间距、圆角、阴影、z-index 约定、motion、断点、reset 和 base element rules。

适合的内容：

- light/dark token；
- 字体和尺寸变量；
- reset、body 基线与 reduced-motion；
- 对所有控件成立的 focus 可见性原则。

不应放入 Button、Composer、PermissionRequest 或某个 scene 的专用 selector。新增 token 前要确认至少有明确语义和使用方；只有一个局部使用方的值留在负责该视觉的组件中。

### 2.2 `primitives`

存放单一职责的基础控件，例如 Button、Input、Choice、Icon、Surface。primitive 不认识 session、permission、queue 或 settings 等产品含义。

只有 DOM、状态含义和交互规则一致时才能共享同一 primitive。视觉相似但键盘或选择语义不同的控件必须保持独立，或只共享更低层 token。

### 2.3 `composites`

存放由多个 primitive 组成、可以独立复用的完整结构，例如 Composer、Popover、Notification、ToolCall 和替换 Composer 的通用交互面板。

Composite 可以管理自己的 focus、展开、选中和 draft 显示，但不能调用 Tauri、读取 mock、切换整个页面或拥有产品数据。

### 2.4 `features`

存放带有明确界面用途的组件组合，例如 Conversation、Queue、Steering、PermissionRequest、AskUserQuestion、Settings、Sidebar 和 Dev Flow。Feature 组合 component 并把意图交给 flow；它不能复制下层稳定 DOM 或 CSS。

### 2.5 `layouts` 与 `scenes`

`layouts` 只定义窗口和页面区域的几何关系，例如 application shell、sidebar shell、main content slot 和 Composer slot。它不决定 Button 的颜色，也不实现 PermissionRequest 的提交。

`scenes` 只存在于 prototype，用配置选择：

- layout；
- component/feature 组合；
- 初始 UI state；
- mock ID；
- 为展示目标状态要执行的操作。

Scene 不复制 application shell、公共 HTML、CSS 或交互脚本。

### 2.6 `entries`

Entry 负责以确定顺序加载样式和启动脚本。它不保存组件实现或产品逻辑。

- Prototype 最终由 `prototype/index.html` 装配 scene/gallery。
- Runtime 保留 main/sidebar 两个入口，并分别使用 `scripts/entries/main.js` 与 `scripts/entries/sidebar.js`；两个入口继续共享同一产品状态、scene identity、revision 和事件来源。

CSS entry 必须保留已验证的 cascade 顺序，不能形成循环 import。

## 3. CSS 规则

原型和 runtime 的 CSS 都使用 `foundations/primitives/composites/features/layouts/entries` 责任层级，但不直接共享文件。

重构现有 CSS 时：

1. 以完整 rule block 为最小搬移单位；
2. 不改变 selector、declaration、specificity、media query、keyframe 或相对 source order；
3. 不把跨组件规则留在 `utilities.css` 或换名后的兜底文件；
4. 稳定 CSS 必须外置，scene 只能保存确实无法归入公共责任且只属于该 scene 的覆盖；
5. 修改公共 CSS 后，沿 `UI_COVERAGE.used_by` 重放全部使用方，不能只看当前页面。

新增 CSS 的判断顺序：

```text
是否是全局语义 token？
  是 → foundations
  否 → 是否是单一基础控件？
         是 → primitives
         否 → 是否属于可独立复用的组合结构？
                是 → composites
                否 → 是否属于明确界面功能？
                       是 → features
                       否 → 是否只负责页面几何？
                              是 → layouts
                              否 → 重新检查责任是否过宽或缺少设计定义
```

## 4. Template、renderer 与 script

### 4.1 Prototype template

`prototype/templates/` 保存可跨 scene 装配的稳定 HTML。Template 可以包含：

- 语义化标签和稳定 DOM 层级；
- ARIA role、label、expanded/selected 等属性位置；
- 数据填充 slot；
- 固定存在的按钮、输入和容器。

Template 不能包含 mock/产品数据、inline handler、整页 scene 副本或 CSS。

### 4.2 Script

`scripts/` 负责数据填充、事件绑定、UI state 和流程协调：

| 目录 | 责任 |
| --- | --- |
| `core/` | template 加载、mount、scene 启动和所有入口需要的最小机制。 |
| `components/` | component 内部键盘、focus、选择、展开等交互。 |
| `features/` | Conversation、Settings、Sidebar、Dev Flow 等功能装配。 |
| `flows/` | 跨 component 的提交、取消、替换 Composer、错误和 focus 恢复顺序。 |
| `adapters/` | 读取 mock 或调用现有 Tauri 接口，并转换成 component 输入。 |
| `entries/` | 启动 main/sidebar/prototype，不包含稳定实现。 |

Script 不得保存一份与 template 重复的稳定 skeleton。由数据数量决定的重复 row 可以由 renderer 生成，但该 renderer 必须是唯一结构真源。

### 4.3 Runtime renderer

Runtime 不新增异步 template fetch。main/sidebar 的稳定 application shell 保留在对应 HTML；数据决定的重复结构由 `scripts/components/` 下唯一 renderer 生成。这一差异是为了保持 Tauri 启动与渲染时序，不代表 runtime 可以复制组件。

## 5. 状态归属

### 5.1 产品数据

Session、message、permission request、settings、notification 和 Dev Flow payload 属于产品数据。

- Runtime 继续通过现有 product state、handler、event 和 IPC 获取。
- Prototype 通过 mock adapter 获取。
- Component 和 flow 不能建立第二套产品状态。

### 5.2 Component 当前显示状态

Popover open、button focus、choice highlighted、tool card expanded 等由对应 component script 管理。它们不写回产品数据，除非现有产品契约明确要求持久化。

### 5.3 Session-specific UI state

输入 draft、caret/selection、scroll、某个 tool 是否展开等需要随 session 恢复的状态继续使用 runtime 现有 session-specific UI state。它们不能变成全局变量，也不能交给 prototype mock 决定生命周期。

任何拆分都不得改变状态何时创建、保存、恢复或清除。

## 6. Component、flow 与 adapter 边界

一次动态交互按以下方向传递：

```text
用户操作
  → component 产生明确意图
    → flow 协调跨组件顺序
      → adapter 调用 prototype mock 或现有 Tauri 接口
        → 结果返回 component 显示
```

- Component 处理 click、keyboard、focus、ARIA 和自身 UI state；不直接调用 mock/Tauri。
- Flow 处理 disable、waiting、success close、error、cancel、乱序和 focus restore；不创建稳定 HTML。
- Adapter 转换数据并透明返回错误；不决定 DOM、CSS 或流程，不得吞错或返回空对象伪装成功。
- Runtime adapter 只能调用现有 handler/event/IPC。发现契约不足时停止并说明事实、影响和最小方案，未经批准不能修改 Rust production。

以 AskUserQuestion 提交为例：component 收集当前 choice/Other 并发出 submit intent；flow 按现有顺序禁用控件、等待、处理成功/失败并恢复 focus；prototype adapter 返回 mock 结果，runtime adapter 调用现有 question handler。两侧 component 的视觉和操作语义各自保持重构前结果。

## 7. 实际组件关系

### 7.1 Composer、Queue 与 Steering

```text
Input + Button + Popover
  └─ Composer
       ├─ Queue：复用 Composer 的位置与意图，增加排队呈现
       └─ Steering：复用 Composer 的位置与意图，增加 steering 呈现
```

Composer 拥有输入区、toolbar、action area、draft/selection 和自身键盘规则。Queue/Steering 不能复制 Composer；它们通过 feature/flow 增加展示和发送模式。若现有 DOM、状态或发送顺序不同，只共享确认相同的下层部分。

### 7.2 PermissionRequest 与 AskUserQuestion

```text
Input + Button + Choice
  └─ 替换式交互面板
       ├─ PermissionRequest：permission 选项、hint、trust 与响应含义
       └─ AskUserQuestion：问题、choice、Other 与回答含义
```

只有稳定 DOM、状态语义和交互规则一致的部分可以进入公共替换式面板。Permission 的 queue/trust/cancel 与 Question 的多题/Other/validation 各自保留在 feature/flow，不能因为外观相似而强制合并。

### 7.3 ToolCall 与 evidence

公共 ToolCall 负责 title、status、expand/collapse 和 output slot；Read/Edit/Bash/Subagent/Question/generic evidence 只实现各自内容。生命周期与展开逻辑若一致应只有一份，否则只抽取明确相同的 status/surface primitive。

## 8. Application scene、gallery 与 mock

### 8.1 Application scene

Application scene 展示真实应用在一个明确时刻的完整状态，例如 empty-session、complete-session、permission-request、settings-appearance 或 dev-flow-runtime。它必须复用 application shell、component、feature 和 mock adapter。

### 8.2 Gallery

Gallery 集中展示视觉参考或 component variant。`visual-direction` 的 Independent demo topbar、说明和 Empty/Active/Appearance 展示布局属于 gallery；它们不是 runtime 页面。

Gallery 可以有自己的展示布局，但 Button、Input、token 或与 application scene 相同的 component 必须引用同一 prototype 公共实现，不能形成第二套组件库。

最终入口：

- `prototype/index.html?scene=empty-session`；
- `prototype/index.html?gallery=visual-direction`；
- 无参数时加载 `empty-session`；
- 未知 scene/gallery ID 必须在 mount 前明确失败，不能装配部分页面或 fallback 到未定义内容。

### 8.3 Mock

Prototype 演示数据集中在 `prototype/mock/`，包括 session/message、tool、permission、question、settings、notification、Dev Flow 和 success/error/cancel 响应。

- Scene 只引用 mock ID 或文件，不复制大段数据。
- Mock 字段形状尽量接近 runtime 输入，但只由 prototype adapter 读取。
- Runtime source、Rust、Tauri config 和 app bundle 对 `prototype/mock/` 必须零依赖。
- Runtime 数据失败时继续走真实错误路径，禁止 fallback 到 mock 或默认空结果。

## 9. UI_COVERAGE、DESIGN_GAPS 与 baseline

[`UI_COVERAGE.json`](./UI_COVERAGE.json) 是逐项结构化真源，记录稳定 ID、runtime/prototype 入口、状态、CSS、markup、behavior、全部使用方和验证 ID。[`UI_COVERAGE.md`](./UI_COVERAGE.md) 只解释组织并按区域汇总，不复制详细事实。

[`DESIGN_GAPS.md`](./DESIGN_GAPS.md) 只收录 `partial`/`missing` 项。缺口关闭前：

- 可以整理 runtime 现有代码，但不能推导新视觉；
- 不能把隐藏 DOM、单独 CSS rule 或 no-op handler 当作覆盖；
- 不能用相邻 component 的设计替用户补齐缺失状态。

修改公共实现前，必须用 `verification` ID 找到重构前 baseline。后续使用相同 URL/fixture、viewport、DPR、theme、font、motion、scroll、focus、展开/选中状态和操作步骤，分别比较：

1. 完整截图；
2. 关键 DOM/class/attribute；
3. computed style；
4. keyboard、focus、hover、选择、展开、提交、取消、错误、乱序和恢复结果。

文件数量、selector 数量、页面能加载或自动化测试通过不能代替实际视觉与交互验证。macOS 完成实际 WebView/native split 验证；Linux/Windows 没有真实环境时只验证共享契约，并明确保留 `pending platform visual verification`。

## 10. 如何新增或拆分内容

### 10.1 新增 token

1. 明确语义和 light/dark 值；
2. 确认不是单个 component 的局部值；
3. 放入双方各自的 `foundations`；
4. 登记所有使用方并检查 contrast、focus、reduced-motion 和响应式结果；
5. 更新相关文档与 baseline。

### 10.2 新增 component

1. 检查已有结构，只有 DOM、状态语义、交互三者都一致才共享；
2. 选择 primitive/composite/feature 层级；
3. 定义稳定 markup、输入、意图和 UI state；
4. Prototype 建唯一 template/renderer，runtime 建唯一 renderer；
5. 把数据调用留给 adapter，把跨组件顺序留给 flow；
6. 在 `UI_COVERAGE` 登记 CSS/markup/behavior/used_by/verification；
7. 为每个使用 scene/runtime surface 建立并重放 baseline。

例如新增 Composer 内的按钮：Button 视觉属于 primitive；按钮在 Composer 中的位置和 focus 属于 composite；点击后改变 Queue/Steering 的顺序属于 flow；Tauri invoke 属于 runtime adapter。

### 10.3 新增交互或 flow

先写清 intent、参与 component、成功/失败/cancel/乱序和 focus restore，再实现 flow。不得把 `invoke()` 塞进 click handler，也不得在 flow 拼 stable HTML。

例如 PermissionRequest 的 Allow/Deny：component 只产生 choice intent；flow 处理 responding、下一条 queued request、session switch 与错误恢复；adapter 发送现有 permission response。

### 10.4 新增 application scene

1. 确认它代表真实应用时刻，而非 gallery；
2. 选择 layout 和公共 component/feature；
3. 添加集中 mock 与 scene 配置；
4. 只记录初始 state 和演示操作，不复制 shell；
5. 更新 `UI_COVERAGE`、scene 文档和 baseline；
6. 从统一入口打开并验证 desktop/narrow、theme、motion 和交互。

### 10.5 新增 gallery view

Gallery view 只保存展示特有布局/说明；公共 token、control 和 behavior 必须引用 prototype 公共实现。它需要独立稳定 ID、used_by 和 baseline，不能伪装成 application scene。

### 10.6 拆分现有页面

1. 先从 `UI_COVERAGE` 找到所有使用方和 baseline；
2. 按完整 CSS rule block、稳定 DOM、component behavior、flow、adapter 顺序识别责任；
3. 只搬移，不顺手改变视觉、文案、交互或业务；
4. 每个稳定实现只能保留一个来源；
5. 从最小组织单元向 component、feature/flow、完整 scene/runtime surface 定位差异；
6. 任一差异都停止相关拆分并恢复重构前结果。

## 11. Prototype 真源切换

根目录 `prototype/` 只有在候选原型完整通过 baseline 对照后才能成为真源。切换必须在同一个执行阶段完成：

1. 停止修改 `docs/gui/new-version/`；
2. 搬入已验证的 scene、gallery、template、style、script、mock 和文档；
3. 同步内部路径及源码、测试、规则和维护文档中的有效引用；
4. 从新路径重开全部 scene/gallery 并运行专项测试；
5. 验证通过后删除 `docs/gui/prototype/`、`docs/gui/new-version/` 和旧清单。

最终不保留 redirect、兼容入口、说明存根或第二份可编辑原型。设计概念由 `DESIGN.md` 解释；最终 GUI 设计真源是 `DESIGN.md` 与根目录 `prototype/`，视觉等价仍以双方各自的重构前 baseline 判断。

## 12. 变更完成检查

- [ ] 层级与依赖方向正确，没有下层引用上层。
- [ ] 稳定 CSS、markup 和 behavior 各有且只有一个责任位置。
- [ ] 产品数据、component UI state 与 session-specific UI state 没有混合。
- [ ] Component、flow、prototype adapter 和 runtime adapter 边界清楚，错误透明。
- [ ] Scene/gallery 没有复制 shell 或公共 component。
- [ ] Runtime、Rust、Tauri config 和 bundle 不依赖 prototype mock。
- [ ] `UI_COVERAGE`、`DESIGN_GAPS`、Related Docs 和 backlinks 与实现同步。
- [ ] 所有 `used_by` 均按稳定 verification ID 完成前后对照。
- [ ] macOS 实际视觉/native split 已验证；其他平台未验证项明确为 pending。
- [ ] 验证 server 与测试 app 已关闭。

## Related Docs

- [`SPEC：GUI 原型与真实前端组件化重构`](../../.dev-doc/refactor/ui-architecture/SPEC.md)
- [`GUI 界面对照表`](./UI_COVERAGE.md)
- [`GUI 设计缺口表`](./DESIGN_GAPS.md)
- [`GUI 架构`](./ARCHITECTURE.md)
- [`前端术语`](./FRONTEND_TERMINOLOGY.md)
- [`界面使用指南`](./UI_USAGE_GUIDELINES.md)
- [`Native split 验证`](./NATIVE_SPLIT_VALIDATION.md)
