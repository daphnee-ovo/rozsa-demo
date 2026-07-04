---
source: audit
nums: 1
---

- [x] ISSUE-I064：Provider: OpenAI-completions 缺少 cache control / tool choice / session affinity
  - severity: P2
  - location：crates/rozsa-model/src/providers/openai_completions/payload.rs
  - description：TS OpenAI-completions 支持：toolChoice (auto/none/required/function-specific)、cache_control on messages/tools (Anthropic-compat proxy)、long cache retention TTL、session affinity headers (多种格式)、GitHub Copilot 专用 headers、Cloudflare AI Gateway routing。Rust 缺少这些。实现参考：legacy-ts/packages/ai/src/providers/openai-completions.ts。方案：payload 构建时增加 tool_choice 字段；headers 增加 session affinity；cache_control 作为可选 message metadata。
  - reproduce：使用 OpenAI 模型时无法指定 tool_choice: required
  - fix：验证确认已实现：openai_completions/payload.rs 已有 CacheControlFormat/session_affinity/tool_choice。误报。
