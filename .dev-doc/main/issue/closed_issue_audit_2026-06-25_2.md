---
source: audit
nums: 1
---

- [x] ISSUE-I002：stats 字段恒 None — Token/Cost 面板无数据
  - severity: P1
  - location：crates/rozsa-tui/src/backend/native.rs
  - description：push_state 中 stats 字段写死为 None。AssistantMessage.usage 数据已存在，需累加输出到 stats 字段，解锁侧边栏 TOKENS 面板。
  - reproduce：
  - fix：
