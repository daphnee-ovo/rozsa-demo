// rozsa-tui — Rust native TUI frontend
//
// Internal Framework:
// main.rs
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

mod app;
#[allow(dead_code, unused_imports, clippy::large_enum_variant)]
mod backend;
#[allow(dead_code)]
mod command;
mod data;
mod input;
mod panels;
mod protocol;
mod render;
mod theme;
mod util;
mod widgets;

/// 统一换行符：\r\n 和 \r 归一化为 \n
fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ROZSA_TUI_MODE=legacy 时退出，让 TS 前端接管
    if std::env::var("ROZSA_TUI_MODE").as_deref() == Ok("legacy") {
        return Ok(());
    }
    app::run().await
}
