// widgets/tab_bar.rs — 可复用的水平 tab 条
//
// Internal Framework:
// tab_bar.rs
// ├── TabBarState       pub struct tab 状态
// └── render_tab_bar()  pub fn 渲染

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::theme::THEME;

/// Tab 条状态
pub struct TabBarState {
    pub tabs: Vec<String>,
    pub active: usize,
    /// 可选的特殊高亮 tab（区别于 active），用于跨面板指示
    pub highlight: Option<usize>,
}

/// 渲染水平 tab 条，超出宽度时显示 ‹ / › 溢出指示。
///
/// - Active tab: THEME.accent 背景 + 黑色文字 + 加粗
/// - Highlight tab: THEME.border_accent 边框色（与 active 区分）
/// - 溢出: 两端显示 ‹ / ›，颜色为 THEME.muted
pub fn render_tab_bar(frame: &mut ratatui::Frame<'_>, area: Rect, state: &TabBarState) {
    let max_width = area.width as usize;
    if max_width == 0 || state.tabs.is_empty() {
        return;
    }

    let muted_style = Style::default().fg(THEME.muted);

    // 计算每个 tab 的渲染宽度：" name " + 紧跟前导的双空格分隔（首个 tab 前是单空格 padding）
    let label_widths: Vec<usize> = state
        .tabs
        .iter()
        .map(|name| name.chars().count() + 2)
        .collect();

    // 选择一个能容纳 active 的窗口
    let total_with_separators = |start: usize, end: usize| -> usize {
        let mut w = 1; // 前导 padding
        for i in start..end {
            if i > start {
                w += 2;
            }
            w += label_widths[i];
        }
        w
    };

    // 从 active 出发向左右尽可能扩展
    let mut left = state.active;
    let mut right = state.active + 1;
    // 预留溢出指示空间
    let reserve = |has_left: bool, has_right: bool| -> usize {
        let mut r = 0;
        if has_left {
            r += 2;
        }
        if has_right {
            r += 2;
        }
        r
    };

    // 先确保 active 单独能放下
    while right > left
        && total_with_separators(left, right)
            + reserve(left > 0, right < state.tabs.len())
            > max_width
        && right - left > 1
    {
        // active 已经是唯一，且仍然放不下 — 退出
        break;
    }

    loop {
        let has_left = left > 0;
        let has_right = right < state.tabs.len();
        let need = total_with_separators(left, right) + reserve(has_left, has_right);
        if need >= max_width {
            break;
        }
        let mut grew = false;
        if right < state.tabs.len() {
            let new_has_right = right + 1 < state.tabs.len();
            let new_need = total_with_separators(left, right + 1)
                + reserve(has_left, new_has_right);
            if new_need <= max_width {
                right += 1;
                grew = true;
            }
        }
        if !grew && left > 0 {
            let new_has_left = left - 1 > 0;
            let new_need = total_with_separators(left - 1, right)
                + reserve(new_has_left, has_right);
            if new_need <= max_width {
                left -= 1;
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }

    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::raw(" "));
    if left > 0 {
        spans.push(Span::styled("‹ ", muted_style));
    }

    for i in left..right {
        if i > left {
            spans.push(Span::raw("  "));
        }
        let label = format!(" {} ", state.tabs[i]);
        let style = if i == state.active {
            Style::default()
                .bg(THEME.accent)
                .fg(ratatui::style::Color::Black)
                .add_modifier(Modifier::BOLD)
        } else if state.highlight == Some(i) {
            Style::default()
                .fg(THEME.border_accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(THEME.text)
        };
        spans.push(Span::styled(label, style));
    }

    if right < state.tabs.len() {
        spans.push(Span::styled(" ›", muted_style));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}
