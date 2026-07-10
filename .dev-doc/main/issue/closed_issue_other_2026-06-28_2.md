---
source: other
nums: 1
---

- [x] ISSUE-I014：TUI 消息区虚拟滚动 — 只渲染可见消息
  - severity: P1
  - location：crates/rozsa-tui/src/render/messages.rs
  - description：当前 render_messages 每帧组装所有消息的 Lines 然后切片，对话越长越卡。改为虚拟滚动：缓存每条消息行高，滚动时只渲染可见范围内的消息。
  - reproduce：
  - fix：
