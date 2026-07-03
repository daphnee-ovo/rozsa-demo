---
source: audit
nums: 1
---

- [x] ISSUE-I047：agent_session: cancel_token lock 跨 async 持有 — abort 完全无效
  - severity: P0
  - location：crates/rozsa-app/src/agent_session.rs:268
  - description：prompt() 在 insert cancel_token 后持有锁跨越整个 agent_loop 执行。abort() 需要同一把锁导致取消阻塞到 agent 结束
  - reproduce：启动 agent 执行后按 Escape 取消 → agent 继续运行
  - fix：False positive: MutexGuard 是临时变量在语句结束即释放，不跨 async 持有
