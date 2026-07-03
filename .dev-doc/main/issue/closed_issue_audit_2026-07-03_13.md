---
source: audit
nums: 1
---

- [x] ISSUE-I059：App: 缺少 HTML 导出 — 无法分享/归档会话
  - severity: P1
  - location：crates/rozsa-app/src/lib.rs
  - description：TS 有完整 HTML 导出 (6 文件 ~100KB)：standalone HTML 含嵌入 CSS/JS、ANSI→HTML 转换、主题感知配色、tool call 折叠/展开、语法高亮、响应式设计。Rust 仅有 JSONL 导出，/export 命令列出但无实现。实现参考：legacy-ts/packages/coding-agent/src/core/export-html/。方案：新建 rozsa-app/src/export/ 模块；使用 askama 模板引擎生成 HTML；ANSI 颜色转 CSS classes。
  - reproduce：执行 /export session.html 期望生成 HTML 文件，命令无效果
  - fix：转入 docs/TODO.md — 长线规划，不作为当前迭代 issue 跟踪
