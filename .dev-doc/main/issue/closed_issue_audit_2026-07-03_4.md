---
source: audit
nums: 1
---

- [x] ISSUE-I050：TUI: word movement 缺 matches_action 检查 — 重绑定无效
  - severity: P1
  - location：crates/rozsa-tui/src/input/keys.rs:1000
  - description：Alt+F/B 只硬编码无 matches_action(tui.editor.cursorWordRight/Left)，用户重绑定后新键无效旧键仍生效
  - reproduce：重绑定 cursorWordRight 到 Alt+W → Alt+W 无反应 Alt+F 仍移动
  - fix：在 match 前添加 matches_action 检查 tui.editor.cursorWordRight/Left + 注册到 default_keybindings
