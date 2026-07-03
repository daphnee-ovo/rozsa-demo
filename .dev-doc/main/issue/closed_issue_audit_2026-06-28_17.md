---
source: audit
nums: 1
---

- [x] ISSUE-I030：agent loop: hook panic 导致整个 loop task crash — 无 panic 保护
  - severity: P1
  - location：crates/rozsa-core/src/agent_loop.rs:518
  - description：TS 对 afterToolCall/prepareToolCall 有 try-catch，异常转为 error result 循环继续。Rust 若 post_tool_use 闭包 panic 则 crash 整个 run_loop task。需用 catch_unwind 或 spawn+catch 包裹 hook 调用做 graceful degradation。
  - reproduce：
  - fix：
