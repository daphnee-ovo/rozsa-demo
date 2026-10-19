---
source: other
nums: 1
---

- [x] ISSUE-I028：Settings 场景替换主界面并恢复窗口拖动
  - severity: P1
  - location：desktop/renderer/styles/features/settings.css
  - description：Translucent 开启时设置侧栏透出主界面侧栏；设置页顶部无窗口拖动区域。设置打开时应停止渲染和交互主场景，返回时恢复；设置页保留原主题及原生透明效果并提供顶部拖动区域。
  - reproduce：开启 Translucent 并打开 Settings，观察 Sessions/Status 穿透设置导航；尝试拖动设置页顶部。
  - fix：设置打开时以 content-visibility:hidden 停止整个主场景的绘制并设为 inert，保留布局防止 Pele 零尺寸损坏；关闭设置时恢复交互及原会话。设置页顶部增加 macOS 原生拖动区域，控件显式 no-drag。旧包场景替换断言失败，新包四种深浅色/透明组合及往返图表回归通过，打包和类型检查通过；用户确认生效。
  - files_modify: [desktop/renderer/styles/layouts/app-shell.css, desktop/renderer/styles/features/settings.css, desktop/renderer/runtime/app/scenes-settings.ts, desktop/tests/native-layout.test.ts, CONTRIBUTING.md]
  - files_create: []
