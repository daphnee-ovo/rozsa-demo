---
source: other
nums: 1
---

- [x] ISSUE-I008：修复全屏标题栏唤出与退出渲染竞争
  - severity: P0
  - location：crates/rozsa-gui/src/native_titlebar.rs
  - description：全屏时有概率无法通过屏幕顶边唤出原生标题栏，切换应用后恢复；随后退出全屏会出现标题栏背景重复分层和渲染异常。
  - reproduce：进入 macOS 原生全屏，反复将指针移到顶边并直接退出；部分循环无法唤出标题栏，切换应用恢复；退出后顶部出现额外白色标题栏层。
  - fix：移除全局 NSMenu 菜单栏显隐控制；改用 AppKit Will/Did fullscreen 生命周期同步前端 transition 状态，阻止 isFullscreen 在过渡期覆盖 CSS；补充生命周期回归测试并完成三轮真实窗口验证。
  - files_modify: [crates/rozsa-gui/src/native_titlebar.rs, crates/rozsa-gui/tests/appearance_settings_test.rs, crates/rozsa-gui/Cargo.toml, crates/rozsa-gui/frontend/app.js, crates/rozsa-gui/src/lib.rs]
  - files_create: []
