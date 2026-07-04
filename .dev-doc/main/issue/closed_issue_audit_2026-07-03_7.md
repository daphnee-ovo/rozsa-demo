---
source: audit
nums: 1
---

- [x] ISSUE-I052：CLI: 缺少 print mode (--print/-p) — 非交互式单次执行不可用
  - severity: P0
  - location：crates/rozsa-cli/src/args.rs
  - description：Rust 已有隐式 print 行为（提供 positional prompt 时打印文本并退出，run.rs:180-198），但缺少：1) 显式 --print/-p flag（TS 用户习惯）；2) --mode json 输出格式（事件流）；3) 多消息顺序执行支持；4) SIGTERM/SIGHUP graceful shutdown。TS 参考：legacy-ts/packages/coding-agent/src/modes/print-mode.ts (159 行)。方案：args.rs 增加 --print/-p (alias for prompt mode) + --output-format text|json；json 模式将 AgentEvent 序列化为 JSONL 输出。
  - reproduce：运行 rozsa --print -p explain this code 期望得到纯文本输出，实际无此标志
  - fix：args.rs 增加 --print/-p + --output-format text|json；run.rs prompt 分支增加 JSON 事件流输出
