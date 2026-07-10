---
source: other
nums: 1
---

- [x] ISSUE-I013：fix: 权限审批 choice 字符串不匹配 — approve 被当作 deny
  - severity: P1
  - location：crates/rozsa-tui/src/panels/permission.rs, crates/rozsa-tui/src/backend/native.rs
  - description：面板发送 approve_once/approve_session，后端期望 allow/allow-session，导致所有审批被当作 deny。同时 trust_key 来源错误 — 面板传的是 UI 选项 key 而非 ApprovalInfo.trust_key。修复：统一 choice 字符串为 allow/allow-session/deny，trust_key 从 prompt.request.trustKey 取。
  - reproduce：
  - fix：
