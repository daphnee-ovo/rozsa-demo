---
source: audit
nums: 1
---

- [ ] ISSUE-I004：SkillRegistry 未接入 TUI/AgentSession
  - severity: P1
  - location：crates/rozsa-app/src/skills/mod.rs
  - description：SkillRegistry 实现完整（match_input, find_by_name, build_system_prompt_fragment），但未被 AgentSession、NativeBackend 或 CLI 任何地方引用。Skill 匹配和系统提示注入完全未对接。
  - reproduce：
  - fix：
