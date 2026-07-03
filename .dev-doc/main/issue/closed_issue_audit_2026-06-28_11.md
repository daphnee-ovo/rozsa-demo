---
source: audit
nums: 1
---

- [x] ISSUE-I024：agent loop: 完全缺失 tool argument validation — LLM 返回错误类型参数导致 tool 失败
  - severity: P0
  - location：crates/rozsa-core/src/agent_loop.rs:467
  - description：TS 有完整 JSON Schema 验证+类型强转（string→number 等）。Rust 零验证，直接传 raw arguments 给 tool.execute。LLM 高频返回 '42' 而非 42 等类型错误，tool deserialize 失败。需加 jsonschema crate 验证层。
  - reproduce：
  - fix：
