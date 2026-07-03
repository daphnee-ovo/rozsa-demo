---
source: audit
nums: 1
---

- [x] ISSUE-I051：subagent: send 与 abort 竞态 — abort 信号被覆盖 subagent 复活
  - severity: P1
  - location：crates/rozsa-app/src/subagent/manager.rs:238
  - description：abort 取消 token 设 Aborted → send 检查通过重置 token 设 Running → abort 丢失
  - reproduce：subagent Running → abort → 立即 send → subagent 被意外复活
  - fix：send() 添加 status==Aborted 检查，拒绝向已 abort 的 subagent 发送消息
