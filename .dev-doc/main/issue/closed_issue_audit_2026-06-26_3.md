---
source: audit
nums: 1
---

- [x] ISSUE-I003：cycle_edit_mode 为 stub — 编辑模式切换无效
  - severity: P2
  - location：crates/rozsa-tui/src/backend/native.rs
  - description：cycle_edit_mode 当前调用 session.cycle_edit_mode() 并发通知，但实际 edit mode 对 tool 行为的影响（如 auto-apply vs ask-first）未接入 agent loop 的 tool 执行策略。
  - reproduce：
  - fix：
