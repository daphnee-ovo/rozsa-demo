// hyperlink.rs — OSC 8 终端超链接
//
// Internal Framework:
// hyperlink.rs
// ├── osc8_hyperlink()       — 生成 OSC 8 序列
// ├── is_web_url()           — URL 验证（仅 http/https）
// └── render_link_span()     — 生成带超链接的 Span 或降级文本
//
// Related Docs:
// - [SPEC](../../../dev-doc/refactor/tui/SPEC.md)
// - [OSC 8 spec](https://gist.github.com/egmontkob/eb114294efbcd5adb1944c9f3cb5feda)

use ratatui::{
    style::{Modifier, Style},
    text::Span,
};

use crate::{theme::THEME, util::terminal_caps::CAPS};

/// 生成 OSC 8 超链接转义序列
pub fn osc8_hyperlink(url: &str, text: &str) -> String {
    format!("\x1b]8;;{url}\x07{text}\x1b]8;;\x07")
}

/// 验证 URL 是否为 http/https 协议
pub fn is_web_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

/// 根据终端能力渲染链接：支持 OSC 8 时使用超链接，否则降级为 "text (url)" 形式
pub fn render_link_spans(text: &str, url: &str) -> Vec<Span<'static>> {
    if !is_web_url(url) {
        return vec![Span::styled(
            text.to_string(),
            Style::default()
                .fg(THEME.md_link)
                .add_modifier(Modifier::UNDERLINED),
        )];
    }

    if CAPS.hyperlinks {
        // 终端支持 OSC 8：文本内嵌超链接序列
        let linked = osc8_hyperlink(url, text);
        vec![Span::styled(
            linked,
            Style::default()
                .fg(THEME.md_link)
                .add_modifier(Modifier::UNDERLINED),
        )]
    } else if text == url {
        // 文字与 URL 相同时不重复显示
        vec![Span::styled(
            text.to_string(),
            Style::default()
                .fg(THEME.md_link)
                .add_modifier(Modifier::UNDERLINED),
        )]
    } else {
        // 降级：text (url)
        vec![
            Span::styled(
                text.to_string(),
                Style::default()
                    .fg(THEME.md_link)
                    .add_modifier(Modifier::UNDERLINED),
            ),
            Span::styled(format!(" ({url})"), Style::default().fg(THEME.dim)),
        ]
    }
}
