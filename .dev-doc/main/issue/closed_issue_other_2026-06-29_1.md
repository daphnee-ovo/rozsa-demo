---
source: other
nums: 1
---

- [x] ISSUE-I034：subagent tool 未注册 — 模型无法 spawn subagent
  - severity: P1
  - location：crates/rozsa-app/src/tools/
  - description：SubagentManager 已完成但缺少 Tool trait 包装注册，导致模型看不到 subagent 工具
  - reproduce：
  - fix：
