---
source: audit
nums: 1
---

- [x] ISSUE-I031：agent loop: pre_tool_use hook 不接收 CancellationToken — 权限对话框无法中断
  - severity: P1
  - location：crates/rozsa-core/src/config.rs:59
  - description：TS 的 beforeToolCall 接收 signal，权限对话框可被 abort 中断。Rust PreToolUseContext 无 CancellationToken。长时间等待用户权限审批时 cancel 无法生效。需加 signal 字段。
  - reproduce：
  - fix：
