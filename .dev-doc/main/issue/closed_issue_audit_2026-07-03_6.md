---
source: audit
nums: 1
---

- [x] ISSUE-I052：Provider: 缺少 OpenAI Responses / Google / Vertex / Mistral / Azure / Cloudflare 共 7 个 provider
  - severity: P0
  - location：crates/rozsa-model/src/providers/mod.rs
  - description：TS 有 10+ provider 实现，Rust 仅有 anthropic、bedrock、openai_completions 三个。缺少：OpenAI Responses (WebSocket)、OpenAI Codex Responses、Google Gemini、Google Vertex AI、Mistral、Azure OpenAI Responses、Cloudflare。用户无法使用非 Anthropic/OpenAI-completions 模型。实现参考：legacy-ts/packages/ai/src/providers/
  - reproduce：在 model selector 选择 Google/Mistral/Azure 模型后发送消息，收到 Provider not yet implemented 错误
  - fix：转入 docs/TODO.md — 长线规划，不作为当前迭代 issue 跟踪
