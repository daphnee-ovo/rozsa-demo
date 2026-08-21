# SPEC：GUI 原型与真实前端组件化重构

## 1. 目标

本次工作是一场前端架构重构，不是界面改版。

需要同时整理两套前端：

1. 用来演示和设计界面的原型；
2. Tauri 应用实际运行的 GUI 前端。

整理后，两套前端都应具备清楚的组件层级、唯一的公共实现、可复用的页面组合方式和明确的数据接入位置。新增组件或场景时，应当能够复用已有 CSS、HTML 结构和交互代码，而不是再次复制整页后重新拆分。

本次重构不得改变任一侧已有的视觉、文案、交互结果或产品行为。原型与真实 GUI 分别和自己的重构前版本比较；不得借此把真实 GUI 改成新版原型的设计语言，也不得把新版原型改得更像当前 GUI。

## 2. 本规范涉及的实际对象

“旧原型”“新版原型”和“真实 GUI”在本规范中固定指以下内容：

| 名称 | 当前路径 | 当前作用 | 本次处理结果 |
| --- | --- | --- | --- |
| 旧原型 | `docs/gui/prototype/` | 较早的原型，只有 `index.html`、`prototype.css`、`prototype.js` | 删除，不作为本次组件化的实现来源 |
| 新版原型 | `docs/gui/new-version/` | 当前需要保留的新版界面原型，包含主原型、11 个场景页、视觉参考页、脚本和样式 | 先在原位置完成组件化，再整体迁至项目根目录 `prototype/` |
| 真实 GUI | `crates/rozsa-gui/frontend/` | Tauri 应用实际加载的 main/sidebar HTML、JavaScript 和 CSS | 保持现有功能与外观，完成同类责任的组件化整理 |

新版原型的当前权威输入包括：

- `docs/gui/new-version/rozsa-gui.html` 与 `rozsa-gui.js`；
- `docs/gui/new-version/scenes/` 下 11 个场景页；
- `docs/gui/new-version/rozsa-visual-demo.html` 独立视觉参考页；
- `docs/gui/new-version/styles/` 下现有 CSS 及加载顺序记录。

最终只有项目根目录 `prototype/` 是可编辑原型。旧原型目录、新版原型的旧路径、重定向文件、兼容入口和第二份可编辑副本都不保留。

## 3. 范围与不可越过的边界

### 3.1 本次要做

- 定义原型和真实 GUI 共同遵守的前端组织规范。
- 按规范拆分 CSS、公共 HTML 结构、组件交互、跨组件流程和数据接入代码。
- 让原型场景通过公共页面骨架、组件组合和 mock 数据生成，不再复制完整页面。
- 将原型展示数据集中到最终的 `prototype/mock/`。
- 将新版原型迁到根目录 `prototype/`，删除旧原型和旧路径。
- 更新 GUI 与 prototype 相关中文文档，使其准确说明组件、场景、目录和新增内容的方法。
- 通过真实页面、真实应用操作和现有专项测试证明重构前后没有退化。

### 3.2 本次不做

- 不把新版设计语言迁移到真实 GUI。
- 不改变颜色、字体、间距、尺寸、圆角、阴影、模糊、层级、动效、文案、图标和响应式结果。
- 不改变点击、键盘、focus、输入、提交、取消、错误、乱序、恢复等可观察交互。
- 不替用户设计原型中缺少的页面、组件、状态或视觉效果。
- 不重构 session、权限、设置、Dev Flow 等产品数据模型。
- 不改变 Tauri IPC、native window 架构或 Rust production 代码，除非现有接口确实无法提供原型已经定义且本次必须使用的数据，并由用户另行批准扩展范围。
- 不引入新的前端框架、构建系统或生产依赖。
- 不通过修改现有测试来掩盖回归。

## 4. 组件化以后应该是什么样

### 4.1 本规范使用的通用术语与项目约定

下表先区分哪些是常见工程概念，哪些只是本项目为了组织原型而采用的名称：

| 名称 | 性质 | 在本项目中的准确含义 |
| --- | --- | --- |
| `component` | 常见 UI 工程概念 | 具有明确 DOM、样式、状态和交互边界的可复用界面单元 |
| `template` | 常见前端概念 | 组件稳定的 HTML 结构；数据填充和事件处理不属于 template |
| `renderer` | 常见前端概念 | 根据数据创建或更新 DOM 的代码；同一种稳定结构只能有一个 renderer/template 来源 |
| `adapter` | 常见 Adapter Pattern | 把 mock 或 Tauri 数据转换成组件需要的输入，不负责界面结构和样式 |
| `application shell` | 常见应用前端概念 | 应用窗口长期存在的页面骨架和内容 slot，例如 sidebar、header、main content 的位置关系 |
| `UI state` | 常见状态管理概念 | focus、展开、选中、draft、scroll 等界面状态；它与后端产品数据分开管理 |
| `baseline` | 常见测试概念 | 修改前保存、修改后可按同样条件重放的对照材料；本项目要求的具体内容见第 9 节 |
| `fixture` | 常见测试概念 | 为重复验证准备的固定输入；runtime fixture 仍通过现有产品契约进入，不等于 prototype mock |
| `scene` | 本项目约定 | 使用应用 shell 展示一个真实应用时刻的原型配置，例如 empty-session 或 permission-request |
| `gallery` | 本项目约定 | 用来集中查看视觉参考或组件变体的原型配置，不代表真实产品页面 |
| `UI_COVERAGE` | 本项目文档 | 逐项对应新版原型和真实 GUI 的界面对照表，不是通用框架或测试覆盖率指标 |
| `DESIGN_GAPS` | 本项目文档 | 记录真实 GUI 已有、但新版原型尚未完整定义的界面和状态 |

`foundations → primitives → composites → features → layouts` 是设计系统中常见的分层思路，但不同项目的命名并不统一。本规范把它固定为 Rózsa 的文件组织约定；执行者不能把这套命名宣称为唯一行业标准，也不能为了满足层级名称制造没有实际复用价值的文件。

### 4.2 先按“职责”拆分，不按页面名称复制

同一个输入框、按钮、悬浮面板或工具调用卡片只能有一份公共实现。页面和场景只能组合这些公共实现，不能各自复制一份再单独维护。

判断两处内容是否应当共用组件，需要同时满足：

1. HTML 结构相同；
2. 显示状态的含义相同；
3. 键盘、focus、选择、提交等交互规则相同。

只有外观看起来相似，不足以合并。若三项中有一项不同，应保留独立组件，或者只抽取真正相同的下层部分。

### 4.3 CSS 和组件的层级

原型与真实 GUI 使用相同的概念层级，但各自维护自己的代码，不互相 import：

```text
基础标准 → 基础控件 → 复合组件 → 功能模块 → 页面布局 → 场景组合 → 加载入口
```

| 层级 | 存放内容 | 本项目中的例子 | 不应放入 |
| --- | --- | --- | --- |
| `foundations` | 全局颜色、字体、间距、圆角、阴影、动效规则、断点 | tokens、reset、base、reduced-motion | 某个按钮或页面的专用样式 |
| `primitives` | 单一职责的基础控件 | Button、Input、Choice、Icon、Surface | session、permission 等业务含义 |
| `composites` | 由多个基础控件组成、可单独复用的完整结构 | Composer、Popover、Notification、ToolCall、替换输入区的面板 | 后端调用和页面切换 |
| `features` | 带有明确界面功能的组件组合 | Queue、Steering、PermissionRequest、AskUserQuestion、Settings、Sidebar、Dev Flow | 整个窗口骨架 |
| `layouts` | 窗口和页面区域的几何排列 | app shell、sidebar shell、主内容区 slot | 组件内部颜色和按钮状态 |
| `scenes` | 选择某个布局、组件、初始状态和演示数据 | empty-session、complete-session、permission-request | 公共组件的重复 CSS 或 HTML |
| `entries` | 按确定顺序加载 CSS 和启动脚本 | main、sidebar、prototype index | 组件实现和业务逻辑 |

CSS 拆分以现有完整 rule block 为单位。只允许改变文件位置和加载位置，不允许改变 selector、declaration、specificity、media query、keyframe 或规则先后关系。`utilities.css` 中的规则必须回到实际负责的组件或基础层，不能换个名字继续作为兜底文件。

### 4.4 原型的 `templates/` 与 `scripts/` 分别负责什么

`templates/` 保存可复用的 HTML 结构，例如 Composer 的输入区、工具栏和操作区，或 PermissionRequest/AskUserQuestion 共用的替换式面板结构。

模板可以包含：

- 语义化标签和稳定的 DOM 层级；
- ARIA 属性和需要填充的明确位置；
- 组件固定存在的按钮、输入区和容器。

模板不能包含：

- mock 或真实产品数据；
- inline event handler；
- 只为某一个场景复制的整页结构；
- 稳定视觉规则。

`scripts/` 负责把模板变成可操作的界面：填入数据、绑定事件、切换显示状态、协调多个组件，以及连接 mock 或 Tauri。脚本不得再保存一份与模板重复的稳定组件骨架。由数据数量决定的重复行可以由脚本生成，但生成它的代码仍必须只有一个实现位置。

两者的关系可以概括为：

```text
template 决定“这个组件由哪些 DOM 部分组成”
script 决定“这些部分何时显示、如何响应操作、数据从哪里进入”
```

这里的外部 `templates/` 目录用于原型，因为原型需要把同一组件装进多个独立 scene。真实 GUI 不新增异步 template 加载：main/sidebar 的稳定 application shell 继续位于各自 HTML 中，数据决定的可复用结构由 `scripts/components/` 下唯一的 renderer 生成。两种方式都必须保证同一组件只有一份稳定 DOM 实现；区别只在于原型需要跨 scene 装配，而真实 GUI 需要保持现有 Tauri 启动与渲染时序。

### 4.5 三类状态分别由谁管理

本规范不新增产品状态模型，只要求把现有状态放回正确位置：

1. **产品数据**：session、message、permission request、settings、notification、Dev Flow payload 等。真实 GUI 继续从现有产品状态、事件和 IPC 获取；原型由 mock 提供。
2. **组件当前显示状态**：popover 是否打开、按钮是否 focus、选项是否高亮、tool card 是否展开等。由对应组件脚本管理，不写回产品数据。
3. **需要随 session 保留的界面状态**：输入草稿、caret/selection、滚动位置、某个 tool 是否展开等。继续使用真实 GUI 现有的 session-specific UI state，不搬成全局变量，也不交给原型 mock 控制。

这里的区分只决定代码放在哪里，不改变这些状态现有的生命周期和恢复行为。

### 4.6 动态交互如何拆分

一次交互按以下责任传递：

```text
组件接收用户操作
→ 发出“提交、取消、选择、关闭”等明确意图
→ 跨组件流程协调先后顺序
→ 数据接入层调用 mock 或现有 Tauri 接口
→ 结果返回组件显示
```

以 AskUserQuestion 提交为例：

1. 组件代码处理点击、Enter、focus 和 ARIA；
2. 它发出“提交当前选择”，但不直接调用 mock 或 Tauri；
3. `flow` 沿用现有顺序处理禁用、等待、成功关闭、失败显示和 focus 恢复；
4. 原型 adapter 返回 mock 结果，runtime adapter 调用现有 handler/Tauri 接口；
5. 组件根据结果显示原有成功或错误状态。

组件代码不复制后端业务；flow 不创建 HTML；adapter 不决定组件长什么样，也不能吞掉错误。

### 4.7 Composer、Queue、Steering 和替换式面板的关系

```text
Input、Button 等基础控件
  └─ Composer：输入区、工具栏、操作区和自身交互
       ├─ Queue：使用 Composer 已有位置和事件增加排队呈现
       └─ Steering：使用 Composer 已有位置和事件增加 steering 呈现

Input、Button、Choice 等基础控件
  └─ 替换式交互面板：临时占用 Composer 区域的公共结构
       ├─ PermissionRequest：加入 permission 选项和提交含义
       └─ AskUserQuestion：加入问题选项和回答含义
```

上图表达的是优先检查的复用关系，不代表可以不看现有实现就强制合并。若实际 DOM、状态或交互不同，只抽取已经确认相同的部分，其余保持独立。

## 5. 原型中的场景、视觉参考和 mock

### 5.1 application scene 是什么

application scene 用来展示“真实应用在某个明确时刻是什么样”。它使用应用公共页面骨架，并组合真实 GUI 中会出现的组件、状态和演示数据。

当前 `docs/gui/new-version/scenes/` 下的 empty-session、complete-session、tool-calls、permission-request、ask-user-question、settings-appearance、permissions、notifications 和 Dev Flow 相关页面都属于 application scene。

组件化后，一个场景只描述：

- 使用哪个页面布局；
- 组合哪些组件；
- 组件从什么状态开始；
- 使用哪份 mock 数据；
- 为展示该状态需要执行哪些操作。

它不再包含一份完整的应用公共页面骨架，也不重新实现公共组件。

### 5.2 gallery 是什么

gallery 用来集中查看视觉参考或组件变体，不代表真实产品中的一张页面。

当前 `docs/gui/new-version/rozsa-visual-demo.html` 是 gallery 内容：它有自己的 “Independent demo” topbar、Empty/Active/Appearance 三个展示视图和视觉研究说明。这些内容需要保留，但它们不是 runtime application scene，也不应被强行套成真实应用页面。

gallery 可以保留自己的展示布局；其中与应用场景相同的 Button、Input、颜色、字体或交互必须引用同一公共实现。只在该视觉参考中存在的 topbar、说明和展示布局可以留在 `gallery/`，但不能再复制一份公共组件。

两类内容通过同一个 `prototype/index.html` 打开：

- `prototype/index.html?scene=empty-session`：打开一个应用场景；
- `prototype/index.html?gallery=visual-direction`：打开视觉参考组合。

使用同一入口的目的是让页面骨架、组件加载和数据装配只有一套机制，不是让 scene 与 gallery 变成同一种页面。无参数时加载 `empty-session`；未知 scene/gallery ID 时在装配页面前明确报错，不加载部分页面，也不增加未定义的 fallback 页面。

### 5.3 mock 是什么，放在哪里

原型没有真实后端，但需要展示接近真实的数据，因此将演示数据集中放在 `prototype/mock/`，例如：

- session 和 message；
- tool call/result；
- permission request；
- AskUserQuestion；
- settings 和 notification；
- Dev Flow 数据；
- success、error、cancel 等响应结果。

场景只引用 mock ID 或文件，不能把同一份大段数据复制进 HTML 或 JavaScript。mock 的字段形状应接近真实 GUI 接收到的数据，但 mock 只能被原型的数据接入层读取。

真实 GUI、Rust、Tauri 配置和应用包不得 import、fetch、复制或 fallback 到 `prototype/mock/`。真实数据失败时继续使用当前错误路径，不能显示 mock 或空对象伪装成功。

## 6. 目标目录及每个目录的职责

### 6.1 原型

```text
prototype/
├── index.html
├── styles/
│   ├── foundations/
│   ├── primitives/
│   ├── composites/
│   ├── features/
│   ├── layouts/
│   ├── scenes/
│   └── entries/
├── templates/
│   ├── primitives/
│   ├── composites/
│   └── features/
├── scripts/
│   ├── core/
│   ├── components/
│   ├── features/
│   ├── flows/
│   └── adapters/
├── scenarios/
├── mock/
├── gallery/
└── docs/
```

| 目录 | 具体内容 |
| --- | --- |
| `styles/` | 各层 CSS；`entries/` 只按原有 cascade 顺序装配其他样式 |
| `templates/` | 公共组件的稳定 HTML 结构；不含数据、事件处理和整页副本 |
| `scripts/core/` | template 加载、组件挂载、场景启动等所有场景都需要的最小机制 |
| `scripts/components/` | Composer、Popover、Notification、ToolCall 等组件内部交互 |
| `scripts/features/` | Conversation、Settings、Sidebar、Dev Flow 等界面功能的装配 |
| `scripts/flows/` | 提交、取消、替换 Composer、失败恢复等跨组件先后顺序 |
| `scripts/adapters/` | 读取 mock 并转换成组件需要的数据；不生成稳定 HTML |
| `scenarios/` | scene/gallery ID 到布局、组件、初始状态、mock 和操作步骤的配置 |
| `mock/` | 原型专用演示数据 |
| `gallery/` | 视觉参考特有的 HTML、CSS、JavaScript、说明和组合配置；公共组件仍从 `styles/`、`templates/`、`scripts/` 引用 |
| `docs/` | 原型架构、组件目录、场景目录、mock 说明和验证方法 |

目录代表责任，不要求每个概念都单独建立文件。只有在内容具有独立复用方、独立状态或独立交互时才拆成单独文件；小型且始终一起变化的内容可以放在同一文件。

### 6.2 真实 GUI

```text
crates/rozsa-gui/frontend/
├── index.html
├── sidebar.html
├── styles/{foundations,primitives,composites,features,layouts,entries}/
└── scripts/{core,components,features,flows,adapters,entries}/
```

真实 GUI 保留 main/sidebar 两个 WebView 入口，因此需要 `scripts/entries/main.js` 和 `scripts/entries/sidebar.js`。两个入口可以引用公共组件和流程，但继续使用同一个现有产品状态、scene revision 和事件来源，不能各自复制一份状态。

原型和真实 GUI 只共享组织规范，不直接共享文件：原型的数据接入层读取 mock，真实 GUI 的数据接入层连接现有 Tauri handler、event 和 IPC。

## 7. 界面对照表与设计缺口表

### 7.1 `UI_COVERAGE` 记录什么

`UI_COVERAGE` 逐项记录新版原型与真实 GUI 中已经存在的界面、状态和交互如何对应。

内容按以下界面区域分组：

1. application shell 与 sidebar；
2. conversation、Composer、Queue 与 Steering；
3. tool call、evidence、diff 与 Markdown；
4. PermissionRequest 与 AskUserQuestion；
5. settings、appearance 与 permission settings；
6. notification 与错误反馈；
7. Dev Flow；
8. gallery 视觉参考。

每个项目使用稳定 ID，并包含以下字段：

| 字段 | 内容 |
| --- | --- |
| `id` | 不随文件移动改变的项目 ID |
| `area` | 所属界面区域 |
| `runtime_entry` | 真实 GUI 的入口文件、渲染函数或事件处理位置 |
| `runtime_trigger` | 在真实 GUI 中到达该界面或状态的具体步骤/事件 |
| `prototype_entry` | 对应的 scene、gallery、component 和状态 |
| `status` | `covered`、`partial`、`missing` 或 `non-visual` |
| `css` | 重构后负责该项目的 CSS 文件 |
| `markup` | 重构后负责稳定 DOM 结构的 template、HTML 或 renderer |
| `behavior` | 重构后负责组件内部交互与跨组件 flow 的脚本 |
| `used_by` | 引用该实现的全部 scene、gallery 或 runtime 界面 |
| `verification` | 第 9 节中用于前后比较的检查 ID |

四种 `status` 的含义固定为：

- `covered`：新版原型已展示真实 GUI 中对应的界面、状态和需要验证的交互；
- `partial`：新版原型存在对应组件，但缺少真实 GUI 已有的一个或多个状态；
- `missing`：真实 GUI 有该可见内容，新版原型没有；
- `non-visual`：该项目只有数据接线或后台行为，没有独立可见表现。

清单需要双向核对：新版原型的主原型、11 个 scene 和 gallery 中的每一项都能找到记录；真实 GUI 的 `index.html`、`sidebar.html`、`app.js`、`sidebar.js`、可见事件分支和现有 GUI 测试涉及的界面也都能找到记录。不存在未分类项目。

最终提供两种表示：

- `prototype/docs/UI_COVERAGE.json` 保存上述逐项结构化记录，作为专项检查读取的数据；
- `prototype/docs/UI_COVERAGE.md` 说明分组、字段和状态，并按区域汇总项目 ID 与结论，不复制另一套可独立维护的详细记录。

### 7.2 `DESIGN_GAPS` 记录什么

`prototype/docs/DESIGN_GAPS.md` 只收录 `UI_COVERAGE` 中状态为 `partial` 或 `missing` 的项目，并沿用相同的界面区域分组。

每个缺口包含：

| 字段 | 内容 |
| --- | --- |
| `gap_id` | 稳定缺口 ID |
| `coverage_id` | 对应的 `UI_COVERAGE` 项目 ID |
| `runtime_trigger` | 在真实 GUI 中看到该内容的步骤或事件 |
| `current_runtime` | 真实 GUI 当前的视觉和交互表现 |
| `prototype_missing` | 新版原型具体缺少的 scene、组件或状态 |
| `required_prototype` | 需要用户在原型中补充的明确内容 |
| `blocked_scope` | 在原型补充前不能进行的界面实现范围 |

`partial` 和 `missing` 项目可以整理真实 GUI 的现有文件位置，但必须保持现有视觉和行为。缺口关闭前不得根据其他组件推导或实现新效果。

## 8. 重构顺序

在已经理解上述架构和文件职责后，按以下顺序执行：

1. 列全新版原型和真实 GUI 的界面对照项、现有触发方式和设计缺口。
2. 在修改实现前，为这些项目建立重构前对照记录。
3. 完成新版原型的 CSS、application shell、templates、component behavior、flow、scene、gallery 和 mock 重构。
4. 原型迁移完成后统一验证：从最小文件组织单元逐层检查到 component、feature/flow 和完整 scene/gallery。
5. 原型统一验证通过后，将其一次性迁到根目录 `prototype/`，同步引用并删除两个旧原型目录。
6. 完成真实 GUI 的 CSS、renderer、component behavior、flow、Tauri adapter 和各功能区域重构。
7. runtime 迁移完成后统一验证：从最小文件组织单元逐层检查到 component、feature/flow 和 main/sidebar 实际界面。
8. 更新组件说明、GUI 架构和 prototype 文档，汇总全部验收结果。

原型和 runtime 各自作为完整迁移阶段处理，不要求每搬移一个文件或组件就执行一轮正式验证。统一验证发现差异后，应从对应的最小组织单元开始定位，再向上重放所有受影响组合，直至完整 scene 或真实 GUI 界面恢复重构前结果。

## 9. 重构前后如何一一验证

### 9.1 什么是“重构前对照记录”

重构前对照记录不是一句“页面看起来正常”，而是每个待检查项目的一套可重复材料。每套材料使用稳定 ID，并包含：

- 检查的是原型还是 runtime；
- 打开页面的 URL，或在真实 GUI 中到达该界面的操作步骤；
- 使用的数据：原型记录 mock 文件和 ID，runtime 记录可重复的 session/fixture 与触发条件；
- 平台、浏览器/WebView 版本、窗口尺寸、viewport、DPR、theme、字体和 reduced-motion 设置；
- 滚动位置、focus 元素、展开项、选中项和其他会影响画面的状态；
- 整体截图和需要重点检查区域的截图；
- 关键 DOM 结构、class/attribute 和关键元素的 computed style；
- hover、键盘、点击、提交、取消、错误和恢复等操作步骤及预期结果。

“未进入重构前对照记录”是指上面任一必要信息缺失，导致重构后无法按同一输入和步骤重现。这样的项目不得开始修改；应先补齐可重复条件。

### 9.2 如何确定需要记录哪些项目

项目集合由第 7 节的界面对照表产生，而不是由实现者凭印象挑选：

1. 新版原型每个 scene、gallery 视图、可切换状态和可操作交互都形成检查项目；
2. runtime 每个可见渲染分支、main/sidebar 场景、事件触发界面和现有测试覆盖的关键流程都形成检查项目；
3. 对照表中的每一行必须引用至少一个检查项目；一个检查项目也必须能反查到对照表中的界面或状态。

若一个公共 CSS、template、renderer 或脚本被多个场景引用，统一验证必须包含所有引用它的检查项目，不能只验证开发者当前打开的页面。

### 9.3 前后如何对应

重构后的检查使用同一个稳定 ID、同一份输入数据、相同环境设置和相同操作步骤。前后材料按 ID 成对保存，不能在看到差异后临时改 viewport、替换数据、裁掉区域或删除检查项。

原型只能与原型重构前材料比较；runtime 只能与 runtime 重构前材料比较。两套界面即使最终长得不同，只要分别保持自身原状，就符合本次重构目标。

截图用于发现整体布局和视觉差异；DOM 与 computed style 用于定位差异；实际操作用于确认交互结果。不能只依靠任意一种证据，也不能用文件数量、selector 数量、页面可加载或测试通过代替真实视觉验证。

### 9.4 统一验证的层级

迁移完成后的验证从最小组织单元向实际界面逐层进行：

1. **文件组织单元**：核对 CSS rule block 的内容和 source order、template/renderer 的稳定 DOM、script module 的输入输出和事件绑定，确认搬移没有丢失、重复或改写实现。
2. **component**：把该 component 的 CSS、DOM 和 behavior 合起来检查，覆盖其已有状态、键盘、focus、展开、选择等表现。
3. **feature 与 flow**：检查多个 component 组合后的数据传递、操作顺序、成功、失败、取消和恢复路径。
4. **实际界面**：原型检查完整 scene/gallery；runtime 检查 main/sidebar 中能够真实到达的界面和状态。

前三级用于定位重构错误，不能分别作为最终通过结论。只有第四级的全部检查项目与重构前记录一致，才能认定该侧迁移验证通过。

某个下层单元被多个上层组合引用时，必须沿引用关系检查所有相关 component、feature/flow 和实际界面。`UI_COVERAGE` 的 `used_by` 与 `verification` 字段提供这条引用关系。

### 9.5 验证失败时如何处理

任何 DOM、computed style、截图、文案、交互顺序、focus、错误、乱序恢复或业务结果差异都表示该项未通过。应停止相关拆分、定位搬移造成的差异并恢复原结果。

新的视觉或行为要求不属于本规范。若用户提出新要求，应停止当前重构并重新确认范围和对照记录，不能作为本次架构重构中的例外直接混入。

### 9.6 自动化测试的作用

专项测试用于检查人工不容易稳定发现的结构和行为问题，例如：

- CSS 加载顺序和循环引用；
- 公共组件是否出现第二份实现；
- scene 是否仍复制应用公共页面骨架；
- main/sidebar scene identity 和 revision continuity；
- IME、事件路由、permission/question、settings、notification、Dev Flow 行为；
- runtime 和应用包是否错误依赖 prototype/mock；
- 旧原型入口和旧引用是否仍有效。

专项测试不负责证明页面“看起来一样”。视觉和交互必须通过实际打开原型和 `./run.sh` 启动的应用验证。

macOS 完成实际 GUI 与 native split 验证。Linux/Windows 完成共享结构、行为和 fallback 检查；未在对应真实平台完成的视觉验证必须明确写为 pending，不能用 macOS 截图代替。

## 10. 原型真源切换

根目录 `prototype/` 只有在候选原型完整通过第 9 节验证后才能成为真源。切换必须在同一个执行阶段完成：

1. 停止修改 `docs/gui/new-version/`；
2. 将已验证内容迁入根目录 `prototype/`；
3. 同步 scene、gallery、template、style、script、mock 和文档内部路径；
4. 更新当前规则、源码、测试和维护文档中的有效引用；
5. 从新路径重新打开全部 scene/gallery 并运行专项测试；
6. 验证通过后删除 `docs/gui/prototype/`、`docs/gui/new-version/` 和旧界面对照/缺口文件。

完成后不保留 redirect、兼容入口、说明存根或第二份可编辑原型。

## 11. 文档要求

- `docs/gui/FRONTEND_INTERFACE_STANDARD.md`：原型和真实 GUI 共同遵守的组件层级、HTML/脚本职责、状态归属、场景、mock、验证和新增内容规范。
- `docs/gui/FRONTEND_COMPONENTS.md`：实际组件目录、组成关系、公共部分、特殊化部分和使用场景。
- `prototype/docs/ARCHITECTURE.md`：原型目录和加载方式。
- `prototype/docs/COMPONENTS.md`：原型组件、template、script 和 CSS 的对应关系。
- `prototype/docs/SCENARIOS.md`：scene 与 gallery 的列表、入口、初始状态和操作方法。
- `prototype/docs/FLOWS.md`：跨组件操作的参与组件、执行顺序、成功/失败结果和 focus 恢复规则。
- `prototype/docs/MOCK_DATA.md`：mock 文件、数据含义及使用场景。
- `prototype/docs/VALIDATION.md`：如何建立和重放第 9 节的对照记录。
- `prototype/docs/UI_COVERAGE.*` 与 `DESIGN_GAPS.md`：第 7 节定义的界面对照和设计缺口。

这些文档是统一规范和组件说明，不是记录“从哪个文件改到哪个文件”的迁移日志。GUI 架构、术语、主题、Dev Flow、native split、Related Docs、backlinks 和项目规则必须与最终实现同步，默认使用中文。

最终 GUI 设计真源指向 `DESIGN.md` 与根目录 `prototype/`。`DESIGN.md` 只解释设计概念；本次重构的视觉判断仍以原型和真实 GUI 各自的重构前表现为准。

## 12. 验收标准

- SPEC-AC-001：中文界面规范和组件说明准确覆盖本规范，并与最终代码结构一致。
- SPEC-AC-002：原型场景不再复制完整页面；应用公共页面骨架、组件 HTML、CSS 和交互分别只有一份实现。
- SPEC-AC-003：scene 与 gallery 的用途、入口和文件归属明确；视觉参考页保持原有布局和交互。
- SPEC-AC-004：原型和真实 GUI 的 CSS 都按责任归类，稳定 CSS 全部外置，无循环加载、跨组件兜底文件或规则顺序变化。
- SPEC-AC-005：原型重构前后、真实 GUI 重构前后的 DOM、computed style、截图、文案和响应式结果分别一致。
- SPEC-AC-006：组件和跨组件流程拆分后，键盘、focus、hover、选择、展开、提交、取消、错误、乱序和恢复结果不变。
- SPEC-AC-007：界面对照表双向覆盖新版原型与真实 GUI 的可见内容；所有设计缺口都有真实触发方式和缺失说明。
- SPEC-AC-008：原型演示数据集中位于 `prototype/mock/`；runtime、Rust、Tauri 配置和应用包对其零依赖。
- SPEC-AC-009：根目录 `prototype/` 是唯一原型真源；旧原型目录、旧入口和当前有效旧引用全部删除。
- SPEC-AC-010：真实 GUI 保留现有产品状态、handler、event、IPC、异步语义及 main/sidebar WebView 契约。
- SPEC-AC-011：文档准确说明最终组件、场景、mock、目录、验证方式和新增内容方法，不保留失效链接或迁移日志式说明。
- SPEC-AC-012：受影响的现有 GUI 专项测试和实际关键流程通过；没有通过修改测试掩盖回归。
- SPEC-AC-013：未经另行批准没有 Rust/IPC production 变化，也没有本规范之外的视觉、交互或业务变化。
- SPEC-AC-014：macOS 完成实际视觉和 native split 验证；Linux/Windows 未完成的真实视觉验证保持 pending。

## 13. 主要风险与对应措施

- **CSS 搬移改变 cascade**：完整搬移 rule block，保留相对顺序，并用 computed style 与截图共同验证。
- **把相似界面错误合并**：只有结构、状态含义和交互规则都一致时才共享组件。
- **template 装配改变初始化顺序**：保持稳定 DOM root、listener 顺序、focus 和 session-specific UI state，并实际重放交互。
- **mock 污染真实应用**：原型和真实 GUI 使用不同的数据接入层，并检查 production source 与应用包。
- **遗漏某个引用页面**：公共文件的每个使用场景都登记在界面对照表中，修改后全部重放。
- **实现者补画缺失原型**：设计缺口表明确记录并停止视觉实现，只搬移真实 GUI 已有代码。
- **main/sidebar 状态分叉**：继续使用现有 scene identity、revision 和事件来源，不复制产品状态。

## 14. 测试计划

- 使用第 9 节定义的成对材料验证原型和 runtime 各自的重构前后结果。
- 实际操作所有登记的 hover、focus、展开、选择、提交、取消、错误、乱序和恢复路径。
- 运行受影响的 `rozsa-gui` 专项测试，覆盖 scene continuity、IME、queue/steering、permission/question、settings、notification、Dev Flow、native split 和 platform fallback。
- 使用结构测试检查 CSS 装配、公共实现唯一性、scene 组合、旧入口删除和 production/mock 隔离。
- 自引入 bug 必须增加能在修复前失败、修复后通过的回归测试。
- 验证结束后关闭本地 HTTP server 和测试应用，不遗留运行实例。
