// render/input_box.rs — 输入框渲染

use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

use crate::{app::AppState, input::InputState, theme::THEME};

fn has_at_completion_token(text: &str) -> bool {
    text.split_whitespace().any(|token| token.starts_with('@'))
}

pub(super) fn render_input(frame: &mut ratatui::Frame<'_>, area: Rect, input: &InputState, state: &AppState) {
    let visible_rows = area.height.saturating_sub(2) as usize; // 减去上下边框

    let full_text = input.text();
    let is_bash_mode = full_text.trim_start().starts_with('!');
    let border_color = if is_bash_mode {
        THEME.bash_mode
    } else {
        THEME.border_muted
    };

    let text_lines: Vec<Line<'static>> = if input.is_empty() {
        vec![Line::from(vec![Span::styled(
            "> ",
            Style::default().fg(THEME.dim),
        )])]
    } else {
        let has_args = full_text.starts_with('/') && full_text.contains(' ');
        let prefix_style = if is_bash_mode {
            Some(Style::default().fg(THEME.bash_mode))
        } else if full_text.starts_with('/') || has_at_completion_token(&full_text) {
            if state.input_has_valid_match || has_args {
                let color = if full_text.starts_with('/') {
                    THEME.accent
                } else {
                    THEME.md_link
                };
                Some(Style::default().fg(color))
            } else {
                Some(Style::default().fg(THEME.dim))
            }
        } else {
            None
        };
        input
            .lines
            .iter()
            .enumerate()
            .map(|(idx, l)| {
                if idx == 0 {
                    if let Some(style) = prefix_style {
                        // slash command 只高亮命令部分（第一个空格之前），args 用正常色
                        if full_text.starts_with('/') {
                            if let Some(space_pos) = l.find(' ') {
                                let cmd_part = &l[..space_pos];
                                let args_part = &l[space_pos..];
                                Line::from(vec![
                                    Span::styled(cmd_part.to_string(), style),
                                    Span::raw(args_part.to_string()),
                                ])
                            } else {
                                Line::styled(l.clone(), style)
                            }
                        } else {
                            Line::styled(l.clone(), style)
                        }
                    } else {
                        Line::raw(l.clone())
                    }
                } else {
                    Line::raw(l.clone())
                }
            })
            .collect()
    };

    // 滚动指示器：光标行在可见区域外时显示在边框标题
    let total_lines = input.lines.len();
    let scroll_offset = input
        .cursor_row
        .saturating_sub(visible_rows.saturating_sub(1));
    let lines_above = scroll_offset;
    let lines_below = total_lines.saturating_sub(scroll_offset + visible_rows);

    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color));

    if is_bash_mode {
        block = block.title_top(Line::styled(" $ bash ", Style::default().fg(THEME.bash_mode)));
    }

    if lines_above > 0 {
        block = block.title_top(Line::styled(
            format!(" ↑ {} more ", lines_above),
            Style::default().fg(THEME.dim),
        ));
    }
    if lines_below > 0 {
        block = block.title_bottom(Line::styled(
            format!(" ↓ {} more ", lines_below),
            Style::default().fg(THEME.dim),
        ));
    }

    let paragraph = Paragraph::new(text_lines)
        .scroll((scroll_offset as u16, 0))
        .block(block);
    frame.render_widget(paragraph, area);

    // 光标位置（相对于可见区域，grapheme-aware）
    use unicode_segmentation::UnicodeSegmentation;
    let cursor_display_width: usize = input.lines[input.cursor_row]
        .graphemes(true)
        .take(input.cursor_col)
        .map(|g| unicode_width::UnicodeWidthStr::width(g))
        .sum();
    let cursor_x =
        area.x + 1 + cursor_display_width.min(area.width.saturating_sub(2) as usize) as u16;
    let visible_cursor_row = input.cursor_row.saturating_sub(scroll_offset);
    let cursor_y = area.y + 1 + visible_cursor_row as u16;
    frame.set_cursor_position((cursor_x, cursor_y));
}
