---
source: audit
nums: 1
---

- [x] ISSUE-I007：Token 估算 chars/4 不适用 CJK
  - severity: P2
  - location：crates/rozsa-app/src/compaction/mod.rs:126
  - description：compaction 阈值判断使用 (chars as u64) / 4 近似 token 数。对中文内容实际 token/char 比约 1:1，导致压缩时机判断偏差可达 4 倍。需引入精确 tokenizer 或区分语言的估算。
  - reproduce：
  - fix：
