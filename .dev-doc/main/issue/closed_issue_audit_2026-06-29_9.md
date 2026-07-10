---
source: audit
nums: 1
---

- [x] ISSUE-I045：permission: 审计日志 — JSONL记录+secret redaction
  - severity: P2
  - location：crates/rozsa-app/src/permissions/
  - description：TS版所有decision写JSONL审计(含redacted args)。Rust版无审计记录。需PermissionAuditEntry+JSONL writer+redaction。
  - reproduce：
  - fix：
