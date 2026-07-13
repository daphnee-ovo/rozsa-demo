---
source: other
nums: 1
---

- [x] ISSUE-I005：修复 macOS 全屏切换后的标题栏与内容布局
  - severity: P0
  - location：crates/rozsa-gui/src/native_titlebar.rs:1
  - description：原生窗口进入全屏后顶部残留空白，退出全屏后标题栏与正文出现双重纵向偏移；sidebar 背景、正文起点和 settings 布局不连续。需要基于真实 NSWindow、contentLayoutRect、contentView 和 WebView parent frame 证据修复原生与 CSS 坐标契约。
  - reproduce：启动 macOS GUI；观察普通窗口；进入全屏；退出全屏；resize 后再次进入并退出全屏。对比标题栏、正文起点、sidebar 背景延伸和 settings 布局。
  - fix：在 Tauri 窗口创建阶段设置 hiddenTitle + titleBarStyle Overlay，使 WebView parent 覆盖完整 NSWindow；保留 CSS 单一 42px traffic-light 避让并由 native-fullscreen class 在全屏清零；增加 AppKit frame 与 JS fullscreen class 校准日志。
  - files_modify: [crates/rozsa-gui/src/native_titlebar.rs, crates/rozsa-gui/src/lib.rs, crates/rozsa-gui/frontend/app.js, crates/rozsa-gui/frontend/index.html]
  - files_create: []
