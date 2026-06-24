// crates/rozsa-tui/src/lib.rs — 库入口
//
// Internal Framework:
// lib.rs
// ├── app             — 应用主循环 (event loop + state)
// ├── ui/             — UI 渲染 (layout + render dispatch)
// ├── input/          — 输入处理 (keyboard + mouse + paste)
// ├── components/     — UI 组件 (editor, sidebar, selectors, permission...)
// ├── backend/        — 后端通信抽象 (socket/mock)
// ├── command/        — 命令系统
// ├── protocol        — 协议类型定义
// ├── overlay         — Overlay 定位与焦点
// ├── keymap          — 快捷键绑定匹配
// ├── theme/          — 颜色主题
// ├── markdown        — Markdown 渲染
// ├── highlight       — 代码语法高亮
// ├── hyperlink       — OSC 8 终端超链接
// ├── terminal_image  — 终端图片协议
// ├── terminal_caps   — 终端能力检测
// ├── ansi            — ANSI SGR 解析
// ├── fuzzy           — Fuzzy 匹配
// ├── undo            — Undo 栈
// └── kill_ring       — Kill Ring
//
// Related Docs:
// - [TUI Design](../../docs/rozsa_framework.md#rozsa-tui--ratatui-终端前端)
// - [Protocol](../../packages/coding-agent/src/modes/native/protocol.ts)

pub mod app;
pub mod ui;
pub mod input;
pub mod components;
#[allow(dead_code, unused_imports, clippy::large_enum_variant)]
pub mod backend;
#[allow(dead_code)]
pub mod command;
pub mod protocol;
pub mod view_model;
pub mod overlay;
pub mod keymap;
pub mod theme;
pub mod markdown;
pub mod highlight;
pub mod hyperlink;
pub mod terminal_image;
pub mod terminal_caps;
pub mod ansi;
pub mod fuzzy;
pub mod undo;
pub mod kill_ring;

/// 统一换行符：\r\n 和 \r 归一化为 \n
pub fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}
