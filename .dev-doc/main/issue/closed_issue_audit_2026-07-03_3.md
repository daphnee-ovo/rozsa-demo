---
source: audit
nums: 1
---

- [x] ISSUE-I049：TUI: kill_ring accumulate 时 ring 为空 — Ctrl+K 连续操作可 panic
  - severity: P1
  - location：crates/rozsa-tui/src/input/kill_ring.rs:39
  - description：Ctrl+K 行尾空删设置 last_action=Kill 但不 push ring → 下次 Ctrl+K accumulate=true ring 空 → 当前守卫可能被绕过
  - reproduce：行尾 Ctrl+K → 下一行 Ctrl+K → 若 caller 不检查 empty 则 panic
  - fix：False positive: push() 中 !self.ring.is_empty() 守卫已确保 unwrap 安全，accumulate=true 且 ring 空时走 else 分支 push
