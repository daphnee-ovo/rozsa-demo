---
source: audit
nums: 1
---

- [x] ISSUE-I003：session_name 恒 None — 标题栏无会话名
  - severity: P1
  - location：crates/rozsa-tui/src/backend/native.rs
  - description：push_state 中 session_name 写死为 None。SessionManager::current_name() 已实现，接线成本极低（一行）。
  - reproduce：
  - fix：
