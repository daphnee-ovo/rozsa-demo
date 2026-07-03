---
source: audit
nums: 1
---

- [x] ISSUE-I027：agent loop: ToolResultMessage 丢失 details 字段 — 结构化工具细节永久丢失
  - severity: P1
  - location：crates/rozsa-core/src/agent_loop.rs:675
  - description：ToolResult 有 details: Value，但 finalized_to_result_message 构造 ToolResultMessage 时忽略它，ToolResultMessage struct 也无此字段。所有结构化工具细节（diff view、file tree metadata）在 core→UI 边界丢失。需在 ToolResultMessage 加 details 字段并传递。
  - reproduce：
  - fix：
