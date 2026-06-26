use rozsa_tui::util::hyperlink::{is_web_url, osc8_hyperlink, render_link_spans};
use rozsa_tui::util::terminal_caps::CAPS;

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
