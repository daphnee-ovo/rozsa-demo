---
source: other
nums: 1
---

- [x] ISSUE-I025：Translucent sidebar 未跟随所选主题背景且泛白
  - severity: P1
  - location：desktop/renderer/styles/foundations/tokens.css:9
  - description：用户截图确认深色 Sidebar 被浅色原生材质洗成亮灰色，且右侧有未覆盖的深色竖带。保留原有暖纸/暖深色主题及其优先级，只修复原生材质的 tint 主导比例与底层渐变裁切，验证完整侧栏覆盖及主题不变。
  - reproduce：选择有明显背景色的自定义主题，开启 Translucent sidebar，对比侧栏计算颜色和所选主题背景。
  - fix：Removed the width-dependent opaque body gradient; the full sidebar element now paints a 92% tint over the native material. Settings shell backgrounds are also transparent. Restored all original theme token priorities after detecting an unintended palette change. Regression failed before the fix with 59 RGB levels of washout; packaged-app checks now pass for light/dark material contrast, unchanged main theme and uncropped shell. The current development window was also inspected with a native screenshot.
  - files_modify: [desktop/renderer/styles/foundations/tokens.css, desktop/renderer/styles/features/sidebar-window.css, desktop/tests/native-layout.test.ts, desktop/tests/native-forge.test.ts, CONTRIBUTING.md]
  - files_create: []
