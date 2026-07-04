---
source: audit
nums: 1
---

- [x] ISSUE-I059：App: Edit tool 缺少 fuzzy matching / diff preview / BOM 处理 — 编辑质量低于 TS
  - severity: P1
  - location：crates/rozsa-app/src/tools/edit.rs
  - description：TS edit-diff.ts (455 行)：fuzzy text matching with Unicode normalization、multi-edit overlap detection、CRLF/LF detection and preservation、UTF-8 BOM handling、unified patch generation、display-oriented diff with context。Rust edit 工具缺少这些高级能力。实现参考：legacy-ts/packages/coding-agent/src/core/tools/edit-diff.ts。方案：在 tools/edit.rs 增加 fuzzy_match() (Unicode NFKC normalize + whitespace tolerance)、detect_line_ending()、strip_bom()、generate_patch() 函数。
  - reproduce：edit 工具因空白差异找不到 old_string 匹配，TS 版通过 fuzzy match 可找到
  - fix：edit.rs 增加 CRLF 保持、BOM 透明处理、whitespace-normalized fuzzy match fallback，新增测试
