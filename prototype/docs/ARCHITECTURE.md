# 原型架构

## 真源与入口

项目根目录 `prototype/` 是唯一可编辑的 Rózsa 原型真源。统一入口是 `prototype/index.html`：

- `?scene=<id>` 通过 `scenarios/registry.js` 校验 scene，并由 `scripts/core/shell.js` 装配 application shell。
- `?gallery=visual-direction` 校验 gallery，加载 `gallery/visual-direction.html`、`gallery/visual-direction.css` 和 `gallery/visual-direction.js`。
- 无参数等同于 `?scene=empty-session`。
- 未知 scene 或 gallery 在装配前显示明确错误，不进入部分页面，也不使用未定义 fallback。

gallery 是视觉研究与组件变体的集中展示，不是 runtime application scene。它可以拥有独立 topbar、说明和展示布局；公共 token 与已确认相同的控件仍从本原型公共 CSS/脚本边界复用。

## 目录职责

| 目录 | 职责 |
| --- | --- |
| `styles/foundations` | tokens、reset、base、motion 和全局视觉标准 |
| `styles/primitives` | forms、actions 等低层控件 |
| `styles/composites` | overlays、feedback 等复合表面 |
| `styles/features` | conversation、settings、sidebar、permissions、Dev Flow 和 gallery 视觉规则 |
| `styles/layouts` | application shell 与 sidebar 的几何关系 |
| `styles/scenes` | 仅由特定 scene 使用的晚期 CSS override |
| `styles/entries` | 保持既有 cascade 顺序的 import 清单 |
| `templates` | 公共稳定 HTML 结构，不包含 mock 和事件处理 |
| `scripts/core` | template 加载、DOM 挂载、scene 启动和 shell 入口 |
| `scripts/components` | Composer、interruption、popover、notification、tool-call 行为 |
| `scripts/features` | Conversation、Settings、Sidebar、Dev Flow 的功能装配 |
| `scripts/flows` | 跨组件提交、取消、focus restore 和 scene 时序 |
| `scripts/adapters` | mock 读取与输入转换 |
| `scenarios` | scene/gallery ID、初始状态、mock 和操作配置 |
| `mock` | 原型专用演示数据，runtime 不得依赖 |
| `gallery` | visual direction 的 HTML、CSS、行为和说明组合 |
| `docs` | 原型组织、组件、场景、流程、mock、验证和对照资料 |

## 加载边界

`styles/main.css` 和 `styles/sidebar.css` 只是稳定 shim，真正的 import 顺序记录在 `styles/source-order.json` 和 `styles/entries/`。application scene 的稳定 shell 来自 `templates/`；scene 文件只提供页面配置和场景专属入口，不复制公共 shell。

`scripts/core/shell.js` 先加载模板，再以固定顺序挂载 root，最后加载 `rozsa-gui.js` 的行为入口。mock 只由 `scripts/adapters/mock-adapter.js` 读取。真实 GUI 位于 `crates/rozsa-gui/frontend/`，与本原型保持实现和数据边界，不直接 import 本目录文件。

## 新增内容

新增 application scene 时，应先在 `scenarios/registry.js` 登记 ID、布局、组件、初始状态、mock 和操作，再复用已有 template、CSS 和 script，最后更新 `docs/UI_COVERAGE.*`、`docs/SCENARIOS.md` 和 baseline。新增 gallery view 只放展示特有结构，公共实现必须沿现有责任边界复用，并登记独立 verification ID。
