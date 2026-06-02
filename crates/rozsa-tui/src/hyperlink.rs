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

use crate::{terminal_caps::CAPS, theme::THEME};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osc8_format() {
        let result = osc8_hyperlink("https://example.com", "click");
        assert_eq!(result, "\x1b]8;;https://example.com\x07click\x1b]8;;\x07");
    }

    #[test]
    fn url_validation() {
        assert!(is_web_url("https://example.com"));
        assert!(is_web_url("http://localhost:3000"));
        assert!(!is_web_url("ftp://files.example.com"));
        assert!(!is_web_url("mailto:user@example.com"));
        assert!(!is_web_url("/local/path"));
    }

    #[test]
    fn render_link_text_equals_url_no_duplicate() {
        let url = "https://example.com";
        let spans = render_link_spans(url, url);
        // 当 text==url 时，无论是否支持 OSC8，都不应出现 "(url)" 后缀
        let all_text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(!all_text.contains("(https://"));
    }

    #[test]
    fn render_link_text_differs_from_url_shows_both() {
        // 当终端不支持 hyperlinks 时，text != url 应显示 "text (url)"
        // 注：CAPS.hyperlinks 可能在测试中为 false
        let spans = render_link_spans("click here", "https://example.com");
        let all_text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(all_text.contains("click here"));
        // 在非 OSC8 环境下应有 URL 后缀
        if !CAPS.hyperlinks {
            assert!(all_text.contains("https://example.com"));
        }
    }

    #[test]
    fn render_link_non_web_url() {
        let spans = render_link_spans("file link", "/local/path");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content.as_ref(), "file link");
    }
}
