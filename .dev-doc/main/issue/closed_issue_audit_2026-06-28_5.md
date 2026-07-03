---
source: audit
nums: 1
---

- [x] ISSUE-I018：agent loop: ShouldStopContext 每轮全量 clone — O(n²) 开销
  - severity: P1
  - location：crates/rozsa-core/src/agent_loop.rs:169
  - description：每轮构造 ShouldStopContext 时 clone 整个 context（全历史消息）和 new_messages。随对话增长总开销二次。should_stop 和 prepare_next_turn 实际只需当前轮 message + usage。改为传引用或只传必要字段。
  - reproduce：
  - fix：
