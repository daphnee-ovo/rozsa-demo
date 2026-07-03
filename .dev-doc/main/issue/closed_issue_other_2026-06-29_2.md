---
source: other
nums: 1
---

- [x] ISSUE-I035：subagent tool: send/wait 不返回 subagent 文本输出
  - severity: P1
  - location：crates/rozsa-app/src/tools/subagent.rs
  - description：send/wait/spawn+wait 只返回状态摘要，不含 subagent 实际回复文本。主 agent 无法获取 subagent 产出内容。
  - reproduce：
  - fix：
