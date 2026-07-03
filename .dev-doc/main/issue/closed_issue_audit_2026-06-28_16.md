---
source: audit
nums: 1
---

- [x] ISSUE-I029：agent loop: agentLoopContinue 无效状态静默返回空 vs 应报错
  - severity: P1
  - location：crates/rozsa-core/src/agent_loop.rs:30
  - description：TS 对空 context 或最后消息是 assistant 时 throw Error。Rust 静默返回 AgentStart+AgentEnd{[]}，与正常无工作不可区分。调用者无法检测使用错误，bug 静默传播。应返回 Result 或 emit error event。
  - reproduce：
  - fix：
