// crates/rozsa-tui/src/lib.rs — 库入口
//
// Internal Framework:
// lib.rs
// ├── app             — 应用主循环 (event loop + state)
// ├── render/         — UI 渲染 (layout + render dispatch + overlay)
// ├── input/          — 输入处理 (keyboard + mouse + paste + keymap + undo + kill_ring + editor)
// ├── panels/         — UI 面板 (sidebar, selectors, permission, autocomplete...)
// ├── widgets/        — 可复用 UI 原子
// ├── backend/        — 后端通信抽象 (socket/mock)
// ├── command/        — 命令系统
// ├── protocol        — 协议类型定义
// ├── theme/          — 颜色主题
// ├── util/           — 工具模块 (markdown, highlight, hyperlink, terminal, ansi, fuzzy)
// └── data/           — 数据 provider (autocomplete_provider, session_search, session_tree)
//
// Related Docs:
// - [TUI Architecture](../../docs/tui/architecture.md)
// - [Protocol](../../packages/coding-agent/src/modes/native/protocol.ts)

pub mod app;
pub mod render;
pub mod input;
pub mod panels;
pub mod widgets;
#[allow(dead_code, unused_imports, clippy::large_enum_variant)]
pub mod backend;
#[allow(dead_code)]
pub mod command;
pub mod protocol;
pub mod theme;
pub mod util;
pub mod data;

/// 统一换行符：\r\n 和 \r 归一化为 \n
pub fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}
