---
source: other
nums: 1
---

- [x] ISSUE-I003：修正未达标的 GUI 布局视觉细节
  - severity: P1
  - location：crates/rozsa-gui/frontend/index.html:278
  - description：TASK-T017 验收不完整：model hover 命中区过宽且圆角不统一；composer hint 放在 toolbar 且 3.6s 轮替；settings/sidebar 顶部间距和 sidebar icon 视觉尺寸未达要求；需按用户复述的五项标准修正并真实运行验收。
  - reproduce：启动 target/debug/rozsa，检查 sidebar Sessions 顶部、Settings 大标题、composer hint/model hover、窗口 resize 居中以及 sidebar toggle icon。
  - fix：缩小 sidebar 与 Settings 顶部间距；model hover 收紧至文本宽度并统一圆角；composer placeholder 30 秒轮替且点击清空；context ring 左移；main 内容 resize 居中；sidebar 图标按真实折叠状态切换并缩至 11px。定向测试、rozsa-gui 全量测试及真实 macOS GUI 验收通过。
  - files_modify: [crates/rozsa-gui/frontend/sidebar.html, crates/rozsa-gui/frontend/index.html, crates/rozsa-gui/frontend/app.js, crates/rozsa-gui/src/native_titlebar.rs, crates/rozsa-gui/tests/gui_layout_polish_test.rs, crates/rozsa-gui/tests/appearance_settings_test.rs]
  - files_create: []
