// widgets/hints_bar.rs — 底部快捷键提示条
//
// Internal Framework:
// hints_bar.rs
// ├── HintItem            pub struct 单条提示
// └── render_hints_bar()  pub fn 渲染

use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::theme::THEME;

/// 单条键位提示
pub struct HintItem {
    pub key: String,
    pub action: String,
}

/// 渲染底部提示条："key action  key action  key action"
/// key 用 accent 色，action 用 muted 色，条目间双空格分隔。
pub fn render_hints_bar(frame: &mut ratatui::Frame<'_>, area: Rect, hints: &[HintItem]) {
    if area.width == 0 || hints.is_empty() {
        return;
    }
    let key_style = Style::default().fg(THEME.accent);
    let action_style = Style::default().fg(THEME.muted);

    let mut spans: Vec<Span> = Vec::new();
    for (i, hint) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(hint.key.clone(), key_style));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(hint.action.clone(), action_style));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}
