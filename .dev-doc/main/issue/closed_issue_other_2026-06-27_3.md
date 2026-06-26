---
source: other
nums: 1
---

- [x] ISSUE-I007：Graph: 增加 tool call/result 节点 — 默认隐藏，按 o 切换显示
  - severity: P2
  - location：crates/rozsa-tui/src/components/graph.rs, crates/rozsa-tui/src/backend/native.rs
  - description：当前 /graph 只显示 user 和 assistant 节点，tool call/result 被 filter 丢弃。应将每对 tool call + tool result 合并为一个节点（显示 tool name + 结果摘要），默认隐藏，用户按 o 键切换显示/隐藏。GraphState 增加 show_tools: bool 字段，构建时保留所有消息，渲染时按该字段过滤 tool 节点。
  - reproduce：
  - fix：
