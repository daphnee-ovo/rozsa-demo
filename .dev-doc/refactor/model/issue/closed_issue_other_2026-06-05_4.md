---
source: other
nums: 1
---

- [x] ISSUE-I001：统一 /model 和 Ctrl+L 的模型选择界面
  - severity: P2
  - location：packages/coding-agent/src/modes/native/native-builtins.ts:192
  - description：/model（无参数）应使用 Rust TUI 侧的 model selector 面板（带搜索框），与 Ctrl+L 行为一致。方案：给 NativeBuiltinContext 加 listModels() 方法，/model 无参时调用它触发 TUI model selector 面板。有参数时保留直接匹配切换逻辑。
  - fix：NativeBuiltinContext 接口增加 listModels() 方法；native-mode.ts 实现发送 models entries 给 TUI；handleModel 无参时调用 ctx.listModels() 触发 Rust model selector 面板，有参数时直接 findModel 切换

