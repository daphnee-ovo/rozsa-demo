---
source: other
nums: 1
---

- [x] ISSUE-I004：修正 main pane 居中与 sidebar 图标视觉尺寸
  - severity: P1
  - location：crates/rozsa-gui/frontend/index.html:2403
  - description：Main pane 居中已修正；补充修复 titlebar sidebar icon 实际渲染尺寸、native split 折叠态左缘悬浮 sidebar 回归、折叠态标题栏避让。
  - reproduce：font size 14 下居中已恢复；折叠 sidebar 后将鼠标移到窗口左缘无悬浮 sidebar；标题 Untitled 与 traffic lights/titlebar 重叠；SF Symbol setSize 8px 后屏幕视觉尺寸无变化。
  - fix：修正 native main body 宽度并由 main-panel 居中 chat/input；sidebar titlebar button 使用 SF Symbol configuration + ScaleNone 校准视觉尺寸；折叠态标题仅增加 12px 顶部避让；左缘触发时将现有 sidebar WKWebView 重挂载到 AppKit 顶层 overlay，继承折叠前宽度，移回 main panel 后隐藏。真实 GUI 验证显示/隐藏和布局通过。
  - files_modify: [crates/rozsa-gui/frontend/index.html, crates/rozsa-gui/src/native_titlebar.rs, crates/rozsa-gui/tests/gui_layout_polish_test.rs, crates/rozsa-gui/tests/appearance_settings_test.rs, crates/rozsa-gui/frontend/app.js, crates/rozsa-gui/src/native_split_view.rs, crates/rozsa-gui/src/lib.rs, crates/rozsa-gui/frontend/sidebar.js, crates/rozsa-gui/tests/native_split_view_test.rs, crates/rozsa-gui/tests/frontend_platform_fallback.rs]
  - files_create: []
