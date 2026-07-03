---
source: audit
nums: 1
---

- [x] ISSUE-I015：agent loop: 取消时 TurnStart/TurnEnd 事件不配对
  - severity: P1
  - location：crates/rozsa-core/src/agent_loop.rs:88
  - description：cancellation 检查在 TurnStart emit 之后。取消时直接 emit AgentEnd 但缺失对应 TurnEnd。依赖事件配对的下游消费者（如 TUI 状态机）会出现状态泄漏。应在 return 前 emit TurnEnd 或使用 RAII guard。
  - reproduce：
  - fix：
