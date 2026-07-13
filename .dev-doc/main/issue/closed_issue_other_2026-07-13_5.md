---
source: other
nums: 1
---

- [x] ISSUE-I009：恢复原生标题栏双击缩放行为
  - severity: P1
  - location：crates/rozsa-gui/src/native_titlebar.rs
  - description：自定义原生标题栏 drag view 拦截双击，窗口无法执行 macOS 标题栏双击缩放/铺满行为。
  - reproduce：窗口模式下双击 traffic lights 右侧标题栏空白区域，窗口尺寸不变化。
  - fix：在 TitlebarDragView mouseDown 中识别双击事件并调用 NSWindow::performZoom，单击仍调用 performWindowDragWithEvent；补充原生标题栏行为回归断言。
  - files_modify: [crates/rozsa-gui/src/native_titlebar.rs, crates/rozsa-gui/tests/appearance_settings_test.rs]
  - files_create: []
