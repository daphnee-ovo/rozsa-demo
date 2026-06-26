---
source: other
nums: 1
---

- [x] ISSUE-I006：Graph: 搜索交互问题 — 缺 hints + 误触进入 + backspace 无效
  - severity: P2
  - location：crates/rozsa-tui/src/components/graph.rs
  - description：1) 底部无操作提示，用户不知道可搜索，随意按键即进入搜索模式无法退出。需增加底部 hints bar（/ to search, ↑↓/jk navigate, Enter expand, Esc close）。2) 搜索应改为 / 触发而非任意字符。3) backspace 删除搜索字符无效（handle_list_key 的 deleteCharBackward 匹配可能未命中实际按键）。
  - reproduce：
  - fix：
