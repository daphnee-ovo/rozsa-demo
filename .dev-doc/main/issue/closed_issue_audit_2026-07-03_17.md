---
source: audit
nums: 1
---

- [x] ISSUE-I063：Provider: Bedrock/OpenAI 缺少 thinking/reasoning 控制 — 高级推理模型无法配置思维深度
  - severity: P1
  - location：crates/rozsa-model/src/providers/bedrock/payload.rs
  - description：TS Bedrock 支持 reasoning (ThinkingLevel)、thinkingBudgets、interleavedThinking、thinkingDisplay (summarized/omitted)。TS OpenAI-completions 支持 reasoningEffort (minimal~xhigh)。Rust 两者都缺少这些控制。实现参考：legacy-ts/packages/ai/src/providers/amazon-bedrock.ts (ThinkingControls 部分)、openai-completions.ts (reasoning_effort)。方案：在 payload 构建时读取 config 中的 thinking_level/reasoning_effort，映射为对应 API 字段。
  - reproduce：设置 thinking level 为 high 后使用 Bedrock Claude 模型，API 请求中无 reasoning 参数
  - fix：验证确认已实现：Bedrock payload.rs 有 ThinkingLevel mapping + budget；OpenAI payload.rs 有 reasoning_effort + tool_choice。误报。
