---
source: audit
nums: 1
---

- [x] ISSUE-I057：App: 缺少 LSP 集成 — 无法提供代码智能（定义跳转/引用/诊断）
  - severity: P1
  - location：crates/rozsa-app/src/lib.rs
  - description：TS 有完整 LSP client (3 文件 500+ 行)：LSPManager 管理 workspace 连接，支持 9 种语言服务器 (TS/Rust/Python/Go/Java/C++/Ruby/PHP/Dart)，提供 diagnostics/definition/references/hover/rename/code-actions 功能。Rust 完全没有 LSP 模块。实现参考：legacy-ts/packages/coding-agent/src/core/lsp/。方案：新建 rozsa-app/src/lsp/ 模块，使用 lsp-types + tower-lsp crate 实现 LSP client；优先支持 rust-analyzer 和 typescript-language-server。
  - reproduce：在 agent 对话中使用需要类型信息的操作，无法获取 LSP 辅助
  - fix：转入 docs/TODO.md — 长线规划，不作为当前迭代 issue 跟踪
