---
source: audit
nums: 1
---

- [x] ISSUE-I048：TUI: delete_selection 对 stale selection_anchor 直接索引可 panic
  - severity: P1
  - location：crates/rozsa-tui/src/input/keys.rs:110
  - description：delete_char_backward 合并行时不清除 selection_anchor，后续 delete_selection 通过 selection_range 获取的 row 越界 panic
  - reproduce：3行文本 Shift+Down 全选 → 行首 Backspace 合并行 → 再触发 delete_selection → panic
  - fix：selection_range() 现在 clamp anchor/cursor row/col 到 lines 有效范围内，防止 stale selection_anchor 导致越界
