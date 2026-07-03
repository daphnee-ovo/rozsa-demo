---
source: audit
nums: 1
---

- [x] ISSUE-I022：agent loop: PendingMessageQueue 死代码
  - severity: P2
  - location：crates/rozsa-core/src/queue.rs
  - description：queue.rs 定义了 PendingMessageQueue（含 OneAtATime/All 模式），但 agent_loop.rs 直接用 Vec 管理 pending messages。要么整合使用，要么删除。
  - reproduce：
  - fix：
