---
source: audit
nums: 1
---

- [x] ISSUE-I042：permission: subagent tool call穿透主permission policy
  - severity: P1
  - location：crates/rozsa-app/src/subagent/manager.rs
  - description：TS版subagent每个tool call经主permissionManager.check(附source=subagentId)。Rust版subagent仅SubagentScope检查,不走主PermissionPolicy,bash/write完全绕过权限。
  - reproduce：
  - fix：
