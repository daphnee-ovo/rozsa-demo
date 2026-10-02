# 新版 GUI 原型样式组件库

本目录装配 `prototype/` 的 CSS。已提取的 foundations、primitives 和 layouts 规则只有一份实现，位于 [`frontend/ui/styles/`](../../crates/rozsa-gui/frontend/ui/styles/README.md)；本目录中的对应路径只转发，不保存第二份规则。其余组件规则由各区域任务继续接入共享目录，scene/gallery 专属内容留在原型宿主。

## 分层

- `foundations/`：tokens、reset、base 与 reduced-motion 基础规则。
- `primitives/`：forms、actions 等可复用的低层交互控件。
- `composites/`：overlays、feedback、notification 等组合控件。
- `features/`：conversation、tools、settings、appearance、sidebar、permissions 与 Dev Flow 等业务表面。
- `layouts/`：应用与 sidebar 的稳定几何骨架，以及随 owner 一起维护的响应式规则。
- `scenes/`：迁移遗留边界，仅保留非运行时标记；application scene 不加载这里的 CSS，公共状态规则归组件、feature 或 layout owner。
- `entries/`：完整 application 入口与独立 sidebar 入口的有序 import 清单。
- `features/visual-demo.css`：独立 visual direction gallery 的稳定视觉规则；由 `gallery/visual-direction.css` 通过 import 加载，不属于 application shell。

全部 application scene HTML 继续加载 `styles/main.css`，该文件是稳定的入口 shim 并转入 `styles/entries/main.css`；visual direction gallery 通过 `gallery/visual-direction.css` 加载共享视觉规则；独立 sidebar WebView 继续加载 `styles/sidebar.css`，该文件转入 `styles/entries/sidebar.css`。原单体 `rozsa-gui.css` 与通用 `utilities.css` 不保留。

响应式规则回到实际 owner CSS 内，保留原始 media-query、selector、declaration 和有效 source order；不再按 560/760/900/1100 生成独立文件。应用骨架的 sidebar 收起边界由 220px sidebar 与 560px 主内容舒适宽度测得为 780px，820px 为展开滞回边界；`foundations/motion.css` 只承载 reduced-motion 规则。

## 不可改变的约束

1. `entries/main.css` 与 `entries/sidebar.css` 的 import 顺序必须与 `source-order.json` 一致；顶层 `main.css`、`sidebar.css` 只承担稳定路径转发。
2. 不得在拆分过程中改写 selector、declaration、custom property、media query、keyframe、specificity 或 rule 内声明顺序。
3. application scene 只在 registry 中声明配置，由 shell 装配稳定 template；不得声明或动态加载 scene stylesheet。visual demo gallery 保持自己的独立加载入口。
4. 新增原型场景应直接复用本组件库；属于组件状态、feature 状态或响应式布局的规则必须进入对应 owner，不得用 scene ID 选择器承载。
5. 原型与 runtime 引用同一共享 owner；不得把共享规则复制回本目录。缺失组件设计仍按已有缺口记录留待后续迭代，不重新混入大文件。

`source-order.json` 保存拆分前单体 CSS 的 SHA-256、每个连续静态块的原始行号和目标摘要、响应式源区间及当前 owner 文件映射；已提取条目的 `target`/`owner_file` 指向共享实现，`prototype_source` 保留其原型来源。它还保存 780/820 sidebar 边界、640×480 最低窗口和实测几何；`extracted_inline_styles` 为空表示应用场景不再承载运行时内联样式。`prototype_stylesheet_extraction_test.rs` 通过完整 import 图验证实际 owner 顺序，scene ownership 测试验证没有场景 CSS 加载边界。
