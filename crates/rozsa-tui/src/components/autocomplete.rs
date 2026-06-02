// components/autocomplete.rs — 自动补全面板：slash command、文件路径、模糊匹配高亮
//
// Internal Framework:
// autocomplete.rs
// ├── AutocompleteState         pub struct 补全面板状态
// ├── AutocompleteAction        pub enum 按键处理结果
// ├── apply_completion()        pub fn 应用补全到文本
// ├── completion_text()         fn 构建补全文本
// ├── handle_autocomplete_key() pub fn 按键处理
// ├── render_autocomplete()     pub fn 渲染面板
// └── request_autocomplete()    pub fn 请求后端补全
//
// Related Docs:
// - [TUI Design](../../../docs/rozsa_framework.md#rozsa-tui--ratatui-终端前端)

use std::{
    error::Error,
    os::unix::net::UnixStream,
    sync::{Arc, Mutex},
};

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::{
    fuzzy::fuzzy_match,
    protocol::{send, ClientMessage, NativeAutocompleteItem},
    components::sidebar::truncate,
    theme::THEME,
};

#[derive(Clone, Debug)]
pub struct AutocompleteState {
    pub prefix: String,
    pub items: Vec<NativeAutocompleteItem>,
    pub selected: usize,
}

impl AutocompleteState {
    pub fn new(prefix: String, items: Vec<NativeAutocompleteItem>) -> Self {
        Self {
            prefix,
            items,
            selected: 0,
        }
    }

    pub fn up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn down(&mut self) {
        self.selected = (self.selected + 1).min(self.items.len().saturating_sub(1));
    }

    pub fn selected_item(&self) -> Option<&NativeAutocompleteItem> {
        self.items.get(self.selected)
    }
}

pub fn apply_completion(text: &str, cursor: usize, state: &AutocompleteState) -> (String, usize) {
    let Some(item) = state.selected_item() else {
        return (text.to_string(), cursor);
    };
    let mut chars = text.chars().collect::<Vec<_>>();
    let prefix_len = state.prefix.chars().count();
    let index = cursor.min(chars.len());
    let start = index.saturating_sub(prefix_len);
    let insert = completion_text(item, &state.prefix);
    chars.splice(start..index, insert.chars());
    let next_cursor = start + insert.chars().count();
    (chars.into_iter().collect(), next_cursor)
}

/// 构建补全后的文本。
/// TS 后端返回的 value 是纯命令名（如 "help"），prefix 是用户已输入的部分（如 "/hel"）。
/// slash command: prefix 以 "/" 开头 → 补全为 "/value "
/// file path: label 以 "/" 结尾 → 补全为 value（不加空格，继续路径）
/// 其他: 补全为 "value "
fn completion_text(item: &NativeAutocompleteItem, prefix: &str) -> String {
    if prefix.starts_with('/') && !prefix[1..].contains('/') {
        // slash command 补全：前缀是 /cmd，value 是命令名
        format!("/{} ", item.value)
    } else if item.label.ends_with('/') {
        // 目录路径：不加空格
        item.value.clone()
    } else {
        format!("{} ", item.value)
    }
}

pub fn render_autocomplete(frame: &mut ratatui::Frame<'_>, area: Rect, state: &AutocompleteState) {
    if state.items.is_empty() || area.height == 0 || area.width == 0 {
        return;
    }
    frame.render_widget(Clear, area);
    let height = area.height.saturating_sub(2) as usize;
    let start = state.selected.saturating_sub(height / 2);
    let end = (start + height).min(state.items.len());
    let mut lines = Vec::new();
    for row in start..end {
        let item = &state.items[row];
        let selected = row == state.selected;
        let base_style = if selected {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(THEME.muted)
        };
        let marker = if selected { "> " } else { "  " };
        let mut spans = vec![Span::styled(marker, base_style)];

        // 模糊匹配高亮
        let label = truncate(&item.label, 28);
        let fm = fuzzy_match(&state.prefix, &label);
        if fm.matches && !fm.positions.is_empty() {
            let chars: Vec<char> = label.chars().collect();
            let mut i = 0;
            let mut buf = String::new();
            for &pos in &fm.positions {
                // 非匹配部分
                while i < pos && i < chars.len() {
                    buf.push(chars[i]);
                    i += 1;
                }
                if !buf.is_empty() {
                    spans.push(Span::styled(buf.clone(), base_style));
                    buf.clear();
                }
                // 匹配字符高亮
                if i < chars.len() {
                    spans.push(Span::styled(
                        chars[i].to_string(),
                        base_style.fg(THEME.accent),
                    ));
                    i += 1;
                }
            }
            // 剩余部分
            while i < chars.len() {
                buf.push(chars[i]);
                i += 1;
            }
            if !buf.is_empty() {
                spans.push(Span::styled(buf, base_style));
            }
        } else {
            spans.push(Span::styled(label.to_string(), base_style));
        }

        if let Some(description) = &item.description {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(
                truncate(description, area.width.saturating_sub(34) as usize),
                Style::default().fg(THEME.dim),
            ));
        }
        lines.push(Line::from(spans));
    }
    let paragraph =
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("Autocomplete"));
    frame.render_widget(paragraph, area);
}

/// 自动补全面板按键处理结果
pub enum AutocompleteAction {
    /// 面板保持打开，不做其他操作
    KeepOpen,
    /// 应用补全并提交（slash command + Enter）
    ApplyAndSubmit,
    /// 应用补全，继续编辑（Tab 或 @file + Enter）
    ApplyAndEdit,
    /// 关闭面板，不应用（Esc）
    Close,
}

/// 处理自动补全面板按键。
/// TS TUI 行为：
/// - Enter：应用补全；若为 slash command 则同时提交，若为 @file 则继续编辑
/// - Tab：应用补全，继续编辑
/// - Esc：关闭面板
/// - Up/Down：导航列表
pub fn handle_autocomplete_key(
    key: KeyEvent,
    mut state: AutocompleteState,
) -> (Option<AutocompleteState>, AutocompleteAction) {
    match key.code {
        KeyCode::Up => {
            state.up();
            (Some(state), AutocompleteAction::KeepOpen)
        }
        KeyCode::Down => {
            state.down();
            (Some(state), AutocompleteAction::KeepOpen)
        }
        KeyCode::Tab => {
            (None, AutocompleteAction::ApplyAndEdit)
        }
        KeyCode::Enter => {
            // Enter 行为取决于 prefix 类型：
            // "/" 前缀（slash command）→ 应用补全并提交
            // "@" 前缀（file path）→ 应用补全，继续编辑
            if state.prefix.starts_with('/') && !state.prefix[1..].contains('/') {
                (None, AutocompleteAction::ApplyAndSubmit)
            } else {
                (None, AutocompleteAction::ApplyAndEdit)
            }
        }
        KeyCode::Esc => (None, AutocompleteAction::Close),
        _ => (Some(state), AutocompleteAction::KeepOpen),
    }
}

/// 向 TS 端发送自动补全请求
pub fn request_autocomplete(
    text: &str,
    cursor: usize,
    writer: &Arc<Mutex<UnixStream>>,
) -> Result<(), Box<dyn Error>> {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    send(
        writer,
        &ClientMessage::AutocompleteRequest {
            id,
            text,
            cursor,
            force: false,
        },
    )?;
    Ok(())
}
