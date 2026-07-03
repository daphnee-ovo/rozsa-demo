---
source: audit
nums: 1
---

- [x] ISSUE-I042：permission: 深度命令分析 — subcommand提取+变量间接
  - severity: P2
  - location：crates/rozsa-app/src/permissions/mod.rs
  - description：TS版checkCommandDeep:反引号subcommand递归检查/变量赋值间接/敏感env泄露/网络外泄。Rust版完全缺失。
  - reproduce：
  - fix：
