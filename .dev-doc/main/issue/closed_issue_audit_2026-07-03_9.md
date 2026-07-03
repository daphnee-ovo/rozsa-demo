---
source: audit
nums: 1
---

- [x] ISSUE-I055：TUI: 缺少 @file 文件附件支持 — 无法在输入中引用文件/图片
  - severity: P1
  - location：crates/rozsa-tui/src/input/editor.rs
  - description：TS native mode 支持 @filename 语法：自动检测文件类型，图片 base64 编码+自动缩放，文本文件包裹在 <file> 标签中，支持 @"path with spaces" 引号路径。Rust 完全没有实现。实现参考：legacy-ts/packages/coding-agent/src/modes/native/native-file-attachments.ts。方案：在 input 模块增加 @ 触发的文件路径解析，autocomplete 补全文件路径，提交时替换为 base64/text content。
  - reproduce：在 TUI 输入框输入 @src/main.rs 期望附件该文件，实际作为纯文本发送
  - fix：验证确认已实现：autocomplete.rs @file 补全 + mouse.rs attach_image + keys.rs take_images。误报。
