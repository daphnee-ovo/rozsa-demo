---
source: audit
nums: 1
---

- [x] ISSUE-I036：permission: 工作区内读操作自动放行
  - severity: P0
  - location：crates/rozsa-app/src/permissions/mod.rs
  - description：TS版对workspace-scoped的read/grep/find/ls无条件Allow,Rust版on-request模式全需审批。需在evaluate()黑名单后加workspace read auto-allow。
  - reproduce：
  - fix：
