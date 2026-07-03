---
source: audit
nums: 1
---

- [x] ISSUE-I038：permission: 复合命令拆分 — pipe/&&/||各段独立检查
  - severity: P0
  - location：crates/rozsa-app/src/permissions/mod.rs
  - description：TS版splitShellSegments将pipe/&&/||拆为独立段各段独立检查黑名单。Rust版只对完整command做正则,env|grep key这类绕过无法检测。
  - reproduce：
  - fix：
