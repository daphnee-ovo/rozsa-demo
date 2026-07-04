---
source: audit
nums: 1
---

- [x] ISSUE-I021：agent loop: cancel 后已 spawn 的并行 tool task 不 abort
  - severity: P2
  - location：crates/rozsa-core/src/agent_loop.rs:482
  - description：cancellation 检查在 spawn 之间，但已 spawn 的 JoinHandle 不会被 abort。signal 传给 tool.execute 但若 tool 不检查 CancellationToken 则任务跑完结果丢弃，浪费资源。应在 cancel 后 abort 未完成 handles。
  - reproduce：
  - fix：cancel 时 abort 所有 pending JoinHandle，新增回归测试
