---
source: test
nums: 1
---

- [x] ISSUE-I001：sidebar WKWebView 遮挡 native theme backing
  - severity: P0
  - location：crates/rozsa-gui/src/native_split_view.rs:install
  - description：前台切换 Dark theme 时 main WebView 更新但 sidebar 仍为白色；CSS transparent 未使 WKWebView surface 透明，native opaque/material backing 被遮挡。
  - reproduce：启动 GUI，打开 Settings，切换 Dark；观察 sidebar pane 保持白色。
  - fix：sidebar child WebView 创建时启用 WebviewBuilder.transparent(true)，使 CSS transparent 与 WKWebView surface 一致并露出 NativeSplitHost backing；theme_surface 增加回归断言。
  - files_modify: [crates/rozsa-gui/src/native_split_view.rs, crates/rozsa-gui/tests/theme_surface.rs]
  - files_create: []
