// render/dialog.rs — Dialog overlay 渲染 + centered_rect 工具

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::{app::DialogState, theme::THEME};

pub(super) fn render_dialog(frame: &mut ratatui::Frame<'_>, area: Rect, dialog: &DialogState) {
    frame.render_widget(Clear, area);

    let has_tabs = dialog.has_tabs();
    // 可用行数（减去边框 2 行 + 可能的 message 1 行 + tab 栏 1 行）
    let tab_lines: u16 = if has_tabs { 1 } else { 0 };
    let header_lines: u16 = if dialog.message.is_some() { 1 } else { 0 };
    let visible_height = area.height.saturating_sub(2 + header_lines + tab_lines) as usize;

    let mut lines: Vec<Line<'_>> = Vec::new();

    // Tab 栏
    if has_tabs {
        let tabs = dialog.tabs();
        let mut tab_spans: Vec<Span> = Vec::new();
        tab_spans.push(Span::raw(" "));
        for (i, tab_name) in tabs.iter().enumerate() {
            if i > 0 {
                tab_spans.push(Span::raw("  "));
            }
            let label = format!(" {} ", tab_name);
            let style = if i == dialog.active_tab {
                Style::default()
                    .bg(THEME.accent)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(THEME.text)
            };
            tab_spans.push(Span::styled(label, style));
        }
        lines.push(Line::from(tab_spans));
    }

    if let Some(message) = &dialog.message {
        lines.push(Line::raw(message.clone()));
    }
    if dialog.kind == "select" || dialog.kind == "confirm" {
        let (display_options, selected_index) = if has_tabs {
            let opts: Vec<String> = dialog
                .filtered_indices
                .iter()
                .map(|&i| strip_category_prefix(&dialog.options[i]))
                .collect();
            (opts, dialog.selected)
        } else if dialog.kind == "confirm" && dialog.options.is_empty() {
            (vec!["Yes".to_string(), "No".to_string()], dialog.selected)
        } else {
            (dialog.options.clone(), dialog.selected)
        };
        // 滚动：保证 selected 在可见窗口内
        let total = display_options.len();
        let scroll_offset = if visible_height == 0 || total <= visible_height {
            0
        } else {
            selected_index
                .saturating_sub(visible_height / 2)
                .min(total.saturating_sub(visible_height))
        };
        let end = (scroll_offset + visible_height).min(total);
        if scroll_offset > 0 {
            lines.push(Line::styled(
                "  ↑ more",
                Style::default().fg(Color::DarkGray),
            ));
        }
        for index in scroll_offset..end {
            let option = &display_options[index];
            let marker = if index == selected_index { "> " } else { "  " };
            let style = if index == selected_index {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            lines.push(Line::styled(format!("{marker}{option}"), style));
        }
        if end < total {
            lines.push(Line::styled(
                "  ↓ more",
                Style::default().fg(Color::DarkGray),
            ));
        }
    } else {
        lines.push(Line::raw(dialog.input.clone()));
    }
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false }).block(
        Block::default()
            .borders(Borders::ALL)
            .title(dialog.title.clone()),
    );
    frame.render_widget(paragraph, area);
}

/// 去掉 "[Category] " 前缀，返回纯展示文本
fn strip_category_prefix(s: &str) -> String {
    if s.starts_with('[') {
        if let Some(end) = s.find("] ") {
            return s[end + 2..].to_string();
        }
    }
    s.to_string()
}

pub(super) fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1]);
    horizontal[1]
}
