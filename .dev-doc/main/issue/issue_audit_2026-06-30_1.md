---
source: audit
nums: 1
---

- [ ] ISSUE-I046：TUI: keybinding action 名字不匹配 — 多个快捷键无效
  - severity: P0
  - location：crates/rozsa-tui/src/backend/native.rs
  - description：default_keybindings()注册的名字和handle_key()中matches_action使用的不一致:thinking.cycle vs thinking.toggle, subagent.prev vs subagent.previous, compact vs tools.expand。导致Ctrl+T/Alt+[/Ctrl+O等快捷键全部失效。
  - reproduce：
  - fix：
