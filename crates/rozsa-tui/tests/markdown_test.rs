use ratatui::style::Modifier;
use rozsa_tui::util::markdown::{parse_markdown, parse_markdown_with_width};

#[test]
fn blockquote() {
    let lines = parse_markdown("> hello world");
    assert_eq!(lines.len(), 1);
    let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(text.contains("│ "));
    assert!(text.contains("hello world"));
}

#[test]
fn strikethrough() {
    let lines = parse_markdown("~~deleted~~");
    assert_eq!(lines.len(), 1);
    let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(text, "deleted");
}

#[test]
fn horizontal_rule() {
    let lines = parse_markdown("---");
    assert_eq!(lines.len(), 1);
    let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(text.contains("─"));
}

#[test]
fn heading() {
    let lines = parse_markdown("# Title");
    assert_eq!(lines.len(), 1);
    let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(text, "Title");
}

#[test]
fn list_items() {
    let lines = parse_markdown("- item one\n- item two");
    assert_eq!(lines.len(), 2);
}

#[test]
fn inline_code() {
    let lines = parse_markdown("use `code` here");
    assert_eq!(lines.len(), 1);
    assert!(lines[0].spans.len() >= 3);
}

// --- h4-h6 headings ---

#[test]
fn heading_h4_to_h6() {
    for level in 4..=6 {
        let hashes = "#".repeat(level);
        let md = format!("{hashes} Sub-heading");
        let lines = parse_markdown(&md);
        assert_eq!(lines.len(), 1, "h{level} should produce one line");
        let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "Sub-heading");
        // h4-h6 should have BOLD | DIM
        let style = lines[0].spans[0].style;
        assert!(style.add_modifier.contains(Modifier::BOLD));
        assert!(style.add_modifier.contains(Modifier::DIM));
    }
}

#[test]
fn heading_invalid_no_space() {
    // "#foo" is not a valid heading (no space after #)
    let lines = parse_markdown("#foo");
    let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(text, "#foo");
}

// --- Nested blockquote ---

#[test]
fn blockquote_nested() {
    let lines = parse_markdown(">> nested");
    assert_eq!(lines.len(), 1);
    let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
    // Should have two "│ " prefixes for nesting
    assert!(text.starts_with("│ │ "));
    assert!(text.contains("nested"));
}

// --- Task list ---

#[test]
fn task_list_checked() {
    let lines = parse_markdown("- [x] Done task");
    assert_eq!(lines.len(), 1);
    let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(text.contains("☑"));
    assert!(text.contains("Done task"));
}

#[test]
fn task_list_unchecked() {
    let lines = parse_markdown("- [ ] Todo task");
    assert_eq!(lines.len(), 1);
    let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(text.contains("☐"));
    assert!(text.contains("Todo task"));
}

// --- Underscore emphasis ---

#[test]
fn underscore_italic() {
    let lines = parse_markdown("hello _world_ end");
    assert_eq!(lines.len(), 1);
    let spans = &lines[0].spans;
    let italic_span = spans.iter().find(|s| s.content.as_ref() == "world").unwrap();
    assert!(italic_span.style.add_modifier.contains(Modifier::ITALIC));
}

#[test]
fn underscore_bold() {
    let lines = parse_markdown("hello __bold__ end");
    assert_eq!(lines.len(), 1);
    let spans = &lines[0].spans;
    let bold_span = spans.iter().find(|s| s.content.as_ref() == "bold").unwrap();
    assert!(bold_span.style.add_modifier.contains(Modifier::BOLD));
}

// --- Nested inline formatting ---

#[test]
fn nested_bold_italic() {
    let lines = parse_markdown("**bold *italic* end**");
    assert_eq!(lines.len(), 1);
    let spans = &lines[0].spans;
    // "bold " should be BOLD only
    let bold_span = spans.iter().find(|s| s.content.as_ref() == "bold ").unwrap();
    assert!(bold_span.style.add_modifier.contains(Modifier::BOLD));
    assert!(!bold_span.style.add_modifier.contains(Modifier::ITALIC));
    // "italic" should be BOLD + ITALIC
    let italic_span = spans.iter().find(|s| s.content.as_ref() == "italic").unwrap();
    assert!(italic_span.style.add_modifier.contains(Modifier::BOLD));
    assert!(italic_span.style.add_modifier.contains(Modifier::ITALIC));
}

#[test]
fn nested_italic_inside_bold() {
    // **bold *italic* more** — 在 bold 内部嵌套 italic
    let lines = parse_markdown("**bold *nested* more**");
    assert_eq!(lines.len(), 1);
    let spans = &lines[0].spans;
    let bold_span = spans.iter().find(|s| s.content.as_ref() == "bold ").unwrap();
    assert!(bold_span.style.add_modifier.contains(Modifier::BOLD));
    assert!(!bold_span.style.add_modifier.contains(Modifier::ITALIC));
    let nested_span = spans.iter().find(|s| s.content.as_ref() == "nested").unwrap();
    assert!(nested_span.style.add_modifier.contains(Modifier::BOLD));
    assert!(nested_span.style.add_modifier.contains(Modifier::ITALIC));
    let more_span = spans.iter().find(|s| s.content.as_ref() == " more").unwrap();
    assert!(more_span.style.add_modifier.contains(Modifier::BOLD));
    assert!(!more_span.style.add_modifier.contains(Modifier::ITALIC));
}

// --- HR width adaptation ---

#[test]
fn hr_width_adapts_to_terminal() {
    let lines_narrow = parse_markdown_with_width("---", 40);
    let text_narrow: String = lines_narrow[0].spans.iter().map(|s| s.content.as_ref()).collect();
    let narrow_count = text_narrow.chars().filter(|&c| c == '─').count();

    let lines_wide = parse_markdown_with_width("---", 120);
    let text_wide: String = lines_wide[0].spans.iter().map(|s| s.content.as_ref()).collect();
    let wide_count = text_wide.chars().filter(|&c| c == '─').count();

    // Narrow terminal: width capped at terminal width
    assert!(narrow_count <= 40);
    // Wide terminal: capped at 80
    assert!(wide_count <= 80);
    // Narrow should be less than wide
    assert!(narrow_count < wide_count);
}

// --- Table inline format ---

#[test]
fn table_cell_inline_format() {
    let md = "| **bold** | `code` |\n|---|---|\n| normal | text |";
    let lines = parse_markdown(md);
    // Should render without crashing, and table contains styled content
    assert!(lines.len() >= 3); // top border + header + separator + data + bottom border
    let all_text: String = lines.iter()
        .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref().to_string()))
        .collect::<Vec<_>>()
        .join("");
    assert!(all_text.contains("bold"));
    assert!(all_text.contains("code"));
}

// --- Link parsing with parentheses in URL ---

#[test]
fn link_with_parens_in_url() {
    let lines = parse_markdown("[wiki](https://en.wikipedia.org/wiki/Rust_(programming_language))");
    assert_eq!(lines.len(), 1);
    let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(text.contains("wiki"));
}

// --- Highlight ==text== ---

#[test]
fn highlight_mark() {
    let lines = parse_markdown("hello ==highlighted== end");
    assert_eq!(lines.len(), 1);
    let spans = &lines[0].spans;
    let hl_span = spans.iter().find(|s| s.content.as_ref() == "highlighted").unwrap();
    assert!(hl_span.style.add_modifier.contains(Modifier::REVERSED));
}

// --- Inline LaTeX $formula$ ---

#[test]
fn inline_latex() {
    let lines = parse_markdown("The formula $E = mc^2$ is famous");
    assert_eq!(lines.len(), 1);
    let spans = &lines[0].spans;
    let formula_span = spans.iter().find(|s| s.content.as_ref() == "E = mc^2").unwrap();
    assert!(formula_span.style.add_modifier.contains(Modifier::ITALIC));
}

// --- Block LaTeX $$ ---

#[test]
fn block_latex() {
    let md = "text\n$$\n\\int_0^1 f(x) dx\n$$\nmore";
    let lines = parse_markdown(md);
    let all_text: String = lines.iter()
        .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref().to_string()))
        .collect::<Vec<_>>()
        .join("|");
    assert!(all_text.contains("\\int_0^1 f(x) dx"));
}
