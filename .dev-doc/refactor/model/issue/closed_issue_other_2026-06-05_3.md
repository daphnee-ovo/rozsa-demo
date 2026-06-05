---
source: other
nums: 1
---

- [x] ISSUE-I001：/model 命令的模型列表格式需改为 [Provider] model_id
  - severity: P2
  - location：packages/coding-agent/src/modes/native/native-builtins.ts:353
  - description：/model 斜杠命令实际走的是 TS 侧的 selectModel 函数，格式为 provider/id，需改为 [Provider] model_id（使用已有的 BUILT_IN_PROVIDER_DISPLAY_NAMES 映射）
  - fix：引入 BUILT_IN_PROVIDER_DISPLAY_NAMES，新增 providerDisplayName() 和 formatModelLabel() 函数，selectModel 和 findModel 均使用 [Provider] model_id 格式

