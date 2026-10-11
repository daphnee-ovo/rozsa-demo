---
source: test
nums: 1
---

- [x] ISSUE-I005：macOS native split GUI ready 返回 main window unavailable
  - severity: P2
  - location：crates/rozsa-gui/src/commands.rs:250
  - description：原生双 WebView GUI 启动后 gui_webview_ready 在已有错误分支返回 main GUI window is unavailable；T021 将真实错误呈现为共享通知。该后端窗口查找逻辑在 T021 前已存在且本任务未修改。
  - reproduce：macOS ./run.sh --prepare-only 后启动 staged app；展开错误列表看到 gui_webview_ready failed: main GUI window is unavailable。
  - fix：ready 流程改为按 Window 身份获取 main，兼容 native split 多 WebView；真实 Tauri mock 双 WebView 回归修复前失败、修复后通过，12 项相关测试通过，staged macOS app 重放不再出现 readiness 错误。
  - files_modify: [crates/rozsa-gui/src/commands.rs, crates/rozsa-gui/src/native_split_view.rs, crates/rozsa-gui/tests/gui_scene_routing_test.rs, crates/rozsa-gui/src/lib.rs]
  - files_create: [crates/rozsa-gui/src/window_visibility.rs, crates/rozsa-gui/tests/gui_scene_routing_test.rs]
