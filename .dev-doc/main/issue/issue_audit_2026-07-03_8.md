---
source: audit
nums: 1
---

- [ ] ISSUE-I054：CLI: 缺少 RPC mode — IDE 集成/程序化 API 不可用
  - severity: P0
  - location：crates/rozsa-cli/src/args.rs
  - description：TS 有完整 RPC mode (755+ 行)：--mode rpc 提供 stdin/stdout JSON-RPC 协议 (25+ 命令)。用于 IDE 扩展、第三方嵌入、SDK 调用。Rust 完全没有实现。实现参考：legacy-ts/packages/coding-agent/src/modes/rpc/。方案：新建 crates/rozsa-rpc/ crate 或在 rozsa-cli 中增加 rpc 子模块，使用 serde_json 处理 JSONL stdin/stdout 协议，复用 AgentSession API。
  - reproduce：尝试 rozsa --mode rpc，无此选项
  - fix：
