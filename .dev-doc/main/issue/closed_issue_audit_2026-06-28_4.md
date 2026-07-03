---
source: audit
nums: 1
---

- [x] ISSUE-I017：agent loop: 无最大轮次保护（无限循环风险）
  - severity: P1
  - location：crates/rozsa-core/src/agent_loop.rs:84
  - description：若 model 持续返回 tool call 且 terminate=false、should_stop_after_turn 不触发，while 循环永远不退出。缺少 circuit breaker。应在 AgentLoopConfig 加 max_turns 或 max_tool_calls 安全阀。
  - reproduce：
  - fix：
