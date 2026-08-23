# 新版 GUI 原型样式组件库

本目录是 `prototype/` 的 CSS 实现真源。它由原来的单体 `rozsa-gui.css` 以及场景内稳定 `<style>` 机械拆分而来，只改变文件位置和 import 组装，不改变任何视觉或交互结果。

## 分层

- `foundations/`：tokens、reset、base 与 reduced-motion 基础规则。
- `primitives/`：forms、actions 等可复用的低层交互控件。
- `composites/`：overlays、feedback、notification 等组合控件。
- `features/`：conversation、tools、settings、appearance、sidebar、permissions 与 Dev Flow 等业务表面。
- `layouts/`：应用与 sidebar 的稳定几何骨架，以及随 owner 一起维护的响应式规则。
- `scenes/`：原来位于具体 HTML `<style>` 中、必须晚于公共入口加载的场景 override。
- `entries/`：完整 application 入口与独立 sidebar 入口的有序 import 清单。
- `features/visual-demo.css`：独立 visual direction gallery 的稳定视觉规则；由 `gallery/visual-direction.css` 通过 import 加载，不属于 application shell。

全部 application scene HTML 继续加载 `styles/main.css`，该文件是稳定的入口 shim 并转入 `styles/entries/main.css`；visual direction gallery 通过 `gallery/visual-direction.css` 加载共享视觉规则；独立 sidebar WebView 继续加载 `styles/sidebar.css`，该文件转入 `styles/entries/sidebar.css`。原单体 `rozsa-gui.css` 与通用 `utilities.css` 不保留。

响应式规则回到实际 owner CSS 内，保留原始 media-query、selector、declaration 和有效 source order；不再按 560/760/900/1100 生成独立文件。应用骨架的 sidebar 收起边界由 220px sidebar 与 560px 主内容舒适宽度测得为 780px，820px 为展开滞回边界；`foundations/motion.css` 只承载 reduced-motion 规则。

## 不可改变的约束

1. `entries/main.css` 与 `entries/sidebar.css` 的 import 顺序必须与 `source-order.json` 一致；顶层 `main.css`、`sidebar.css` 只承担稳定路径转发。
2. 不得在拆分过程中改写 selector、declaration、custom property、media query、keyframe、specificity 或 rule 内声明顺序。
3. application scene 的场景 override 必须在 application entry 之后加载，不能提前并入组件文件；visual demo gallery 保持自己的独立加载入口。
4. 新增原型场景应直接复用本组件库；只有场景独有且确实需要晚期覆盖的规则才进入 `styles/scenes/`。
5. runtime 迁移优先使用同路径文件直接搬移；若目标 runtime 尚无对应文件，应保留本目录中的组件边界，不重新混入大文件。

`source-order.json` 保存拆分前单体 CSS 的 SHA-256、每个连续静态块的原始行号和目标摘要、响应式源区间及当前 owner 文件映射，并保存 780/820 sidebar 边界、640×480 最低窗口和实测几何。`prototype_stylesheet_extraction_test.rs` 会验证静态块的字节级等价、owner 映射、入口顺序、最低尺寸与运行时入口形状。
