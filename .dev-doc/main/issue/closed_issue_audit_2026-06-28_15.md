---
source: audit
nums: 1
---

- [x] ISSUE-I028：agent loop: 缺失 prepareArguments 机制 — tool 无法预处理 LLM 原始参数
  - severity: P1
  - location：crates/rozsa-core/src/tool.rs:28
  - description：TS 每个 tool 可定义 prepareArguments(args) 在 validation 前修正参数（移除多余字段、重命名 deprecated 参数、补默认值）。Rust Tool trait 无此方法，raw arguments 直接使用。需加 fn prepare_arguments(&self, args: Value) -> Value 默认实现。
  - reproduce：
  - fix：
