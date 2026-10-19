---
source: other
nums: 1
---

- [x] ISSUE-I026：调整 Translucent 层级与深浅色材质效果
  - severity: P1
  - location：desktop/renderer/styles/features/sidebar-window.css
  - description：用户明确 Sidebar 应位于 Main Panel 下层；增强原生模糊透视；深色不使用反转阴影，去除侧栏边界阴影并提高透明度。保留现有主题色和完整材质覆盖。
  - reproduce：分别开启深浅色 Translucent，检查主区边界阴影、侧栏原生材质及透明度。
  - fix：Translucent sidebar now sits below the main pane; light mode uses the main pane shadow, dark mode removes boundary shadows and uses a lower tint opacity. Native material appearance follows application light/dark/system settings, preserving the existing palette and full coverage. Bridge regression checks 14/14, packaged native layout checks, typecheck and package passed; inspected the current native development window.
  - files_modify: [desktop/renderer/styles/features/sidebar-window.css, desktop/main/native-appearance.ts, desktop/main/session-bridge.ts, desktop/main/entry.ts, desktop/tests/native-appearance.test.ts, desktop/tests/native-layout.test.ts, desktop/tests/bridge.test.ts, CONTRIBUTING.md, desktop/tests/native-forge.test.ts]
  - files_create: []
