# 新版 GUI 原型样式组件库

本目录是 `docs/gui/new-version/` 的 CSS 实现真源。它由原来的单体 `rozsa-gui.css` 以及场景内稳定 `<style>` 机械拆分而来，只改变文件位置和 import 组装，不改变任何视觉或交互结果。

## 分层

- `tokens.css`：主题变量、字体、radius、shadow 与尺寸 token。
- `reset.css`、`base.css`：全局尺寸约束、box sizing 和基础元素规则。
- `layout/`：应用与 sidebar 的稳定几何骨架。
- `components/`：表单、action、permission、overlay 与 feedback 等可复用控件。
- `features/`：conversation、tools、settings、appearance、sidebar 与 Dev Flow 等业务表面。
- `scenes/`：原来位于具体 HTML `<style>` 中、必须晚于公共入口加载的场景 override。
- `utilities.css`：原生窗口状态、响应式重排和 reduced-motion 等晚期规则。
- `main.css`：完整原型的权威有序入口。
- `sidebar.css`：供独立 sidebar WebView 直接迁移使用的有序子集入口。

全部原型 HTML 直接加载 `styles/main.css`；独立 sidebar WebView 直接加载 `styles/sidebar.css`。原单体 `rozsa-gui.css` 不保留兼容层。

## 不可改变的约束

1. `main.css` 的 import 顺序必须与 `source-order.json` 一致。
2. 不得在拆分过程中改写 selector、declaration、custom property、media query、keyframe、specificity 或 rule 内声明顺序。
3. 场景 override 必须在 `styles/main.css` 之后加载，不能提前并入组件文件。
4. 新增原型场景应直接复用本组件库；只有场景独有且确实需要晚期覆盖的规则才进入 `styles/scenes/`。
5. runtime 迁移优先使用同路径文件直接搬移；若目标 runtime 尚无对应文件，应保留本目录中的组件边界，不重新混入大文件。

`source-order.json` 保存拆分前单体 CSS 的 SHA-256、每个连续块的原始行号和目标摘要，并保存四个 inline-style HTML 的可重建摘要。`prototype_stylesheet_extraction_test.rs` 会按该清单重组并验证字节级等价。
