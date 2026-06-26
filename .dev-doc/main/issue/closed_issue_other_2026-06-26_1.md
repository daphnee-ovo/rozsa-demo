---
source: other
nums: 1
---

- [x] ISSUE-I004：Autocomplete 快速输入时追加而非替换前缀
  - severity: P2
  - location：crates/rozsa-tui/src/backend/native.rs
  - description：快速连续输入 slash 命令时（如快速键入 /login），autocomplete 可能在前缀基础上追加完整补全项，导致输入变为 /l/login 而非 /login。正常速度逐字输入时不复现，但 tmux send-keys 一次性粘贴时稳定复现。怀疑 autocomplete accept 逻辑未正确替换已有前缀文本。
  - reproduce：
  - fix：
