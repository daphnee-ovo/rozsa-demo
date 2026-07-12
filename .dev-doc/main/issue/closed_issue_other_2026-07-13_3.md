---
source: other
nums: 1
---

- [x] ISSUE-I007：修复全屏顶部原生白色遮挡
  - severity: P0
  - location：crates/rozsa-gui/src/native_titlebar.rs
  - description：进入 macOS 原生全屏后，WebView 顶部仍被约 32px 的白色原生 titlebar accessory 区域遮挡；DOM 已确认 native-fullscreen=true 且 app-body padding=0。
  - reproduce：启动真实 .app，启用 translucent sidebar，点击原生全屏按钮；顶部出现贯穿 sidebar 和正文的白色条带。
  - fix：根因是原生标题栏 accessory 和 macOS 全屏菜单栏 reveal 覆盖 WebView 顶部。将 sidebar 按钮改为不参与标题栏布局的 AppKit overlay；全屏隐藏 overlay 与菜单栏，退出后恢复；真实 .app 在普通、全屏、退出、resize 后再全屏及顶边 hover 均验证通过。
  - files_modify: [crates/rozsa-gui/src/native_titlebar.rs]
  - files_create: []
