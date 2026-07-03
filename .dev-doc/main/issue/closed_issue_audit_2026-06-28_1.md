---
source: audit
nums: 1
---

- [x] ISSUE-I014：agent loop: compaction stop 在 tool result 后中断，model 无机会生成最终回答
  - severity: P0
  - location：crates/rozsa-core/src/agent_loop.rs:192
  - description：should_stop_after_turn（由 compaction 阈值触发）检查时机在 tool result 已入 context 之后。若此时判定停止，loop 直接 return，用户看到的最后消息是 tool result 而非 assistant 总结。应在 tool call 存在时跳过 compaction stop 或在 stop 前允许 model 再做一次 text-only 回答。
  - reproduce：
  - fix：
