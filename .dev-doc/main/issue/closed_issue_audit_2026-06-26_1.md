---
source: audit
nums: 1
---

- [x] ISSUE-I001：Slash 命令覆盖不全 — /fork /clone /permissions 等无实现
  - severity: P2
  - location：crates/rozsa-tui/src/backend/native.rs
  - description：约 10 个 slash 命令（/fork /clone /tree /graph /permissions /gc /lsp /login /logout /search）在执行时返回 not yet 提示，功能未实现。不影响核心对话但降低 UX 完整度。
  - reproduce：
  - fix：
