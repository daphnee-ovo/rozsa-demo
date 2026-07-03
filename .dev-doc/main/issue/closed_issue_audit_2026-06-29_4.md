---
source: audit
nums: 1
---

- [x] ISSUE-I039：permission: trust key多级生成 — 对齐generateTrustLevels
  - severity: P1
  - location：crates/rozsa-app/src/permissions/mod.rs
  - description：TS版生成多级trust key(exact→各级前缀→复合命令各段)。Rust版仅ToolName:prefix40单一key,无法细粒度信任传播。
  - reproduce：
  - fix：
