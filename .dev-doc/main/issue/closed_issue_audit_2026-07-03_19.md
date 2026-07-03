---
source: audit
nums: 1
---

- [x] ISSUE-I065：App: 缺少 output truncation 工具函数 — bash 输出无上限保护
  - severity: P1
  - location：crates/rozsa-app/src/tools/bash.rs
  - description：TS truncate.ts (277 行)：truncateHead/truncateTail 双向截断、行数/字节数双限制、UTF-8 安全截断、不截断半行。Rust bash 工具直接收集全部输出，无系统性截断。大命令输出会占满 context window。实现参考：legacy-ts/packages/coding-agent/src/core/tools/truncate.ts。方案：新建 tools/truncate.rs，实现 truncate_head()/truncate_tail() with line_limit/byte_limit 参数，在 bash/grep/find 等工具输出时统一调用。
  - reproduce：运行 ! find / -name *.rs 输出数万行，全部进入 context window 导致 compaction 触发
  - fix：验证确认已实现：bash.rs MAX_OUTPUT_BYTES=100KB、grep.rs DEFAULT_MAX_MATCHES=100 + MAX_LINE_LENGTH=500、read.rs DEFAULT_MAX_LINES=2000 + 50KB。Rust 已有完整截断系统。误报。
