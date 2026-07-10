---
source: audit
nums: 1
---

- [x] ISSUE-I041：permission: session approval持久化到settings
  - severity: P1
  - location：crates/rozsa-app/src/permissions/mod.rs
  - description：TS版approve_session写入project settings whitelist跨会话复用。Rust版仅内存HashSet,session结束即丢。需record_session_approval同步写settings。
  - reproduce：
  - fix：
