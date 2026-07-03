---
source: audit
nums: 1
---

- [x] ISSUE-I023：agent loop: parallel task panic 静默丢弃 — model 收到不完整 tool results 导致 API 400
  - severity: P0
  - location：crates/rozsa-core/src/agent_loop.rs:495
  - description：JoinHandle panic 时 continue 静默跳过。ToolExecutionStart 已 emit 但无对应 End。Model 发 3 个 tool call 只收到 2 个 result → API 400。应对 JoinError 生成 error ToolResultMessage + emit ToolExecutionEnd。
  - reproduce：
  - fix：
