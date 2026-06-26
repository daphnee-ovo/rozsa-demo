---
source: audit
nums: 1
---

- [x] ISSUE-I002：Multi-agent 运行时缺失 — switch_agent 不可用
  - severity: P2
  - location：crates/rozsa-tui/src/backend/native.rs
  - description：switch_agent 返回 not available 通知。需要实现子 agent 运行时（spawn/navigate/view switching）。依赖 AgentSession 支持 sub-agent 生命周期管理。
  - reproduce：
  - fix：
