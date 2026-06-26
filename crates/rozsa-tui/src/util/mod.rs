// util/ — 工具模块
//
// Internal Framework:
// util/
// ├── ansi.rs           — ANSI SGR 解析
// ├── markdown.rs       — Markdown 渲染
// ├── highlight.rs      — 代码语法高亮
// ├── hyperlink.rs      — OSC 8 终端超链接
// ├── fuzzy.rs          — Fuzzy 匹配
// ├── terminal_caps.rs  — 终端能力检测
// └── terminal_image.rs — 终端图片协议

pub mod ansi;
pub mod markdown;
pub mod highlight;
pub mod hyperlink;
pub mod fuzzy;
pub mod terminal_caps;
pub mod terminal_image;
