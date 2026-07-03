---
source: audit
nums: 1
---

- [x] ISSUE-I026：agent loop: 缺失 tool_execution_update 事件 — 长时间 tool 无进度反馈
  - severity: P1
  - location：crates/rozsa-core/src/events.rs:30
  - description：TS 有 tool_execution_update 事件+onUpdate 回调允许 tool 流式报告进度。Rust AgentEvent 无此 variant，tool.execute 的 on_update 始终传 None。长时间 bash 命令/文件操作对 UI 呈黑盒等待。需加 ToolExecutionUpdate variant 并传实际回调。
  - reproduce：
  - fix：
