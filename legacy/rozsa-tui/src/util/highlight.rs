// highlight.rs — 代码语法高亮
//
// Internal Framework:
// highlight.rs
// ├── SYNTAX_SET            — 全局语法集（LazyLock）
// ├── THEME                 — 全局主题（LazyLock）
// ├── highlight_code()      — 对代码块进行语法高亮
// └── syntect_to_ratatui()  — 将 syntect 样式转换为 ratatui 样式
//
// Related Docs:
// - [codex-rs highlight](../../../codex/codex-rs/tui/src/render/highlight.rs)

use std::sync::LazyLock;

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use syntect::{
    easy::HighlightLines,
    highlighting::{FontStyle, Style as SyntectStyle, ThemeSet},
    parsing::SyntaxSet,
    util::LinesWithEndings,
};

use crate::theme::THEME;

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(|| two_face::syntax::extra_newlines());

fn normalize_language(lang: &str) -> String {
    match lang.to_lowercase().as_str() {
        "typescript" | "ts" => "TypeScript".to_string(),
        "javascript" | "js" => "JavaScript".to_string(),
        "python" | "py" => "py".to_string(),
        "rust" | "rs" => "rs".to_string(),
        "golang" => "go".to_string(),
        "c++" | "cpp" | "cxx" => "cpp".to_string(),
        "c#" | "csharp" => "cs".to_string(),
        "shell" | "sh" | "bash" | "zsh" => "sh".to_string(),
        "yml" => "yaml".to_string(),
        "dockerfile" => "Dockerfile".to_string(),
        "makefile" => "Makefile".to_string(),
        "objc" | "objective-c" => "m".to_string(),
        "jsx" => "jsx".to_string(),
        "tsx" => "tsx".to_string(),
        other => other.to_string(),
    }
}

static HIGHLIGHT_THEME: LazyLock<syntect::highlighting::Theme> = LazyLock::new(|| {
    let ts = ThemeSet::load_defaults();
    ts.themes
        .get("base16-ocean.dark")
        .cloned()
        .unwrap_or_else(|| ts.themes.values().next().unwrap().clone())
});

/// 对代码进行语法高亮，返回 ratatui Line 列表
/// 如果语言不支持或内容过大则返回 None
pub fn highlight_code(code: &str, language: &str) -> Option<Vec<Line<'static>>> {
    if code.len() > 512_000 || code.lines().count() > 10_000 {
        return None;
    }

    let lang = normalize_language(language);
    let syntax = SYNTAX_SET
        .find_syntax_by_token(&lang)
        .or_else(|| SYNTAX_SET.find_syntax_by_extension(&lang))
        .or_else(|| SYNTAX_SET.find_syntax_by_name(language))?;

    let mut highlighter = HighlightLines::new(syntax, &HIGHLIGHT_THEME);
    let mut lines = Vec::new();

    for line_text in LinesWithEndings::from(code) {
        let ranges = highlighter.highlight_line(line_text, &SYNTAX_SET).ok()?;

        let spans: Vec<Span<'static>> = ranges
            .into_iter()
            .map(|(style, text)| {
                let ratatui_style = syntect_to_ratatui(style);
                Span::styled(text.trim_end_matches('\n').to_string(), ratatui_style)
            })
            .collect();

        lines.push(Line::from(spans));
    }

    Some(lines)
}

/// 将 syntect 高亮样式转换为 ratatui 样式
fn syntect_to_ratatui(style: SyntectStyle) -> Style {
    let fg = syntect_color_to_ratatui(style.foreground);
    let mut ratatui_style = Style::default().fg(fg);

    if style.font_style.contains(FontStyle::BOLD) {
        ratatui_style = ratatui_style.add_modifier(Modifier::BOLD);
    }
    if style.font_style.contains(FontStyle::ITALIC) {
        ratatui_style = ratatui_style.add_modifier(Modifier::ITALIC);
    }
    if style.font_style.contains(FontStyle::UNDERLINE) {
        ratatui_style = ratatui_style.add_modifier(Modifier::UNDERLINED);
    }

    ratatui_style
}

fn syntect_color_to_ratatui(color: syntect::highlighting::Color) -> Color {
    if color.a == 0x01 {
        // 终端默认色
        THEME.text
    } else if color.a == 0x00 {
        // ANSI 调色板索引色
        Color::Indexed(color.r)
    } else {
        Color::Rgb(color.r, color.g, color.b)
    }
}
