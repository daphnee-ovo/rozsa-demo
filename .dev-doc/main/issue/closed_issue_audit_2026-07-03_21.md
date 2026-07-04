---
source: audit
nums: 1
---

- [x] ISSUE-I061：TUI: Session 管理命令 (/export /import /share /gc /search) 无实现
  - severity: P1
  - location：crates/rozsa-tui/src/command/builtin.rs
  - description：TS native-session-commands.ts 实现 8 个 session 命令：/export (HTML/JSONL)、/import (JSONL)、/share (GitHub gist)、/copy (clipboard)、/resume (picker)、/gc (垃圾回收)、/search (regex 搜索 50 条上限)、/lsp。Rust builtin.rs 列出命令名但全部 delegate 到 backend 且无实际实现。实现参考：legacy-ts/packages/coding-agent/src/modes/native/native-session-commands.ts。方案：逐个实现，优先 /resume (已有 session_tree)、/search (使用 regex crate)、/gc (按时间删除旧 JSONL)。
  - reproduce：执行 /search pattern 期望搜索会话历史，无结果
  - fix：验证确认已实现：native.rs 全部 session 命令 (compact/resume/lsp/gc/search/export/copy/import/fork/clone/share) 均有实现。误报。
