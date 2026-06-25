---
source: audit
nums: 1
---

- [x] ISSUE-I001：PermissionPolicy 未从 pre_tool_use hook 调用 — tool 执行无守卫
  - severity: P0
  - location：crates/rozsa-app/src/agent_session.rs
  - description：PermissionPolicy 已完整实现，pre_tool_use hook 已存在，但 hook 闭包未实例化 policy 逻辑。所有 tool 调用仍无权限守卫，用户在 TUI 拒绝权限请求实际无效。需在 AgentSession 构建时将 PermissionPolicy::evaluate() 接入 hook。
  - reproduce：
  - fix：
