---
source: other
nums: 1
---

- [x] ISSUE-I006：修复 Overlay 全屏仍残留顶部空白
  - severity: P0
  - location：crates/rozsa-gui/src/native_titlebar.rs:1
  - description：titleBarStyle Overlay 修复窗口模式后，全屏 Settings 与主界面顶部仍残留约 46px 空白。此前 ISSUE-I005 人工验收误判。必须采集全屏 NSWindow/contentLayoutRect/contentView/WebView parent 与 DOM bounding rect 后修复。
  - reproduce：启动 GUI，打开 Settings，进入全屏；顶部出现约 46px 空白。主界面全屏也需复查。退出全屏后再 resize 并重复。
  - fix：根因是 NSTitlebarAccessoryViewController 的 32px TitlebarDragView 在 fullscreen 中仍保留布局槽并覆盖 WebView。进入全屏时隐藏 view 并将高度设为 0；退出时恢复 32px 后显示。Overlay 保证 WebView 覆盖完整窗口，native-fullscreen class 清除 CSS titlebar offset。
  - files_modify: [crates/rozsa-gui/src/native_titlebar.rs, crates/rozsa-gui/src/lib.rs, crates/rozsa-gui/frontend/app.js, crates/rozsa-gui/frontend/index.html, crates/rozsa-gui/tauri.conf.json]
  - files_create: []
