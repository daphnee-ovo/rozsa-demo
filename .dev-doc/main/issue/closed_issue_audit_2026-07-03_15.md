---
source: audit
nums: 1
---

- [x] ISSUE-I056：App: Permission 系统缺少白名单/黑名单规则 — 无法细粒度控制工具权限
  - severity: P1
  - location：crates/rozsa-app/src/permissions/mod.rs
  - description：TS permission 有 7 种规则类型 (toolNames, toolPrefixes, commandExact, commandPrefixes, commandPatterns, pathScopes, pathPatterns)。Rust 有 auto_approve_patterns (正则) + hardcoded blacklist + read-only whitelist，但缺少 settings.json 中的 toolNames/toolPrefixes 等用户可配置的细粒度白名单和 path scope 规则。当前 auto_approve_patterns 是纯正则匹配 trust_key，不区分匹配类型。参考：legacy-ts/packages/coding-agent/src/core/permissions.ts。
  - reproduce：在 settings.json 配置 toolNames allowlist，agent 仍然询问已允许工具的权限
  - fix：新增 allowed_tools + blocked_commands 字段到 settings schema，PermissionPolicy evaluate 中优先检查
