---
source: other
nums: 1
---

- [x] ISSUE-I027：统一关闭 Translucent 时的侧栏层级与阴影
  - severity: P1
  - location：desktop/renderer/styles/features/sidebar-window.css
  - description：上次层级及阴影调整只作用于 Translucent 开启状态。用户要求关闭状态一并调整：Main Panel 在 Sidebar 上层，浅色主区投影，深色无边界阴影；关闭时侧栏保持不透明且主题不变。
  - reproduce：关闭 Translucent，分别切换深浅色，检查侧栏与主区层级和边界阴影。
  - fix：将 Sidebar/main panel 的层级及浅色阴影规则统一应用于 Translucent 开关两种状态；深色均无边界阴影，关闭时保持完全不透明，不修改主题色。新增关闭状态原生回归断言：旧包失败，新包四种组合通过；Forge HMR 原生检查通过。
  - files_modify: [desktop/renderer/styles/features/sidebar-window.css, desktop/tests/native-layout.test.ts, CONTRIBUTING.md, desktop/tests/native-forge.test.ts]
  - files_create: []
