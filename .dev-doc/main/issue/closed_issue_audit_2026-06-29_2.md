---
source: audit
nums: 1
---

- [x] ISSUE-I037：permission: 黑名单补全 — 对齐TS版10+patterns
  - severity: P0
  - location：crates/rozsa-app/src/permissions/mod.rs
  - description：Rust版仅5条,TS版还有rm通配符/rm当前目录/git clean -fd/dd/diskutil erase/rm -rf多种路径变体。
  - reproduce：
  - fix：
