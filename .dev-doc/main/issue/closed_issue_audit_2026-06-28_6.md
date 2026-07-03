---
source: audit
nums: 1
---

- [x] ISSUE-I019：agent loop: StopReason::Length 未处理 — 截断 tool call 传入残缺 JSON
  - severity: P1
  - location：crates/rozsa-core/src/agent_loop.rs:128
  - description：stop_reason 只匹配 Error/Aborted。当 model 因 max_tokens 返回 Length 时，若 content 含未完成 tool call（arguments JSON 被截断），会传入残缺 JSON 给 tool 或 parse 失败。应对 Length 做特殊处理：丢弃截断的 tool call 或触发 continuation。
  - reproduce：
  - fix：
