# Runtime CSS architecture

共同 foundations、primitives 和 layouts 的实现位于 [`../ui/styles/`](../ui/styles/README.md)。main/sidebar 入口引用同一共享基础前缀，并在后置位置加载 actions 与 motion；原型直接引用这些 owner，生产入口不依赖原型资源。

`native-host.css` 最后加载，只处理独立 WebView 的 viewport 和背景边界：main 的 native split 不再预留内嵌 sidebar 背景，sidebar 由原生容器提供背景且不继承整个原型窗口的最小宽度。它不定义组件外观；单 WebView 的非 macOS 布局保留共享窗口约束。

本目录尚存各区域的旧组件消费方；它们按区域接入共享 UI 并验证后，由 T036 清理。下面的旧层级描述用于定位现有规则，不是新增第二套组件或 `utilities.css` 规则的依据。

The Tauri webviews load two stylesheet entry points:

- `main.css` for the main window.
- `sidebar.css` for the native sidebar webview.

Import order is part of the visual contract. Keep imports ordered from broad,
stable foundations to narrow state overrides:

1. `tokens.css` and `reset.css`
2. `base.css` and `layout/`
3. `components/`
4. `features/`
5. `utilities.css` and late compatibility overrides

## Responsibilities

- `tokens.css`: semantic color, typography, radius, and scale custom properties.
- `reset.css`: shared browser normalization that is safe in every webview.
- `base.css`: main-window document defaults.
- `layout/`: window shells and region geometry; no feature-specific presentation.
- `components/`: reusable controls, overlays, forms, and feedback surfaces.
- `features/`: styles coupled to a product feature or scene.
- `utilities.css`: cross-feature state helpers and rich-content utilities.

Add a rule to the narrowest stable responsibility. Create a new feature file
when a scene grows independently; do not add runtime `<style>` tags or static
`style` attributes to HTML. Dynamic values such as progress width may still be
set by JavaScript when they represent runtime state.

## Related docs

- [`docs/gui/ARCHITECTURE.md`](../../../../docs/gui/ARCHITECTURE.md)
- [`docs/gui/UI_USAGE_GUIDELINES.md`](../../../../docs/gui/UI_USAGE_GUIDELINES.md)
- [`docs/gui/FRONTEND_TERMINOLOGY.md`](../../../../docs/gui/FRONTEND_TERMINOLOGY.md)
- [`docs/gui/themes.md`](../../../../docs/gui/themes.md)
