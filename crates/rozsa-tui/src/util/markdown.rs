use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    theme::THEME, util::highlight::highlight_code, util::hyperlink::render_link_spans,
    util::terminal_caps::CAPS, util::terminal_image::render_image,
};

pub fn parse_markdown_with_width(text: &str, terminal_width: usize) -> Vec<Line<'static>> {
    parse_markdown_inner(text, terminal_width)
}

pub fn parse_markdown(text: &str) -> Vec<Line<'static>> {
    parse_markdown_inner(text, 80)
}

fn parse_markdown_inner(text: &str, terminal_width: usize) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut in_code_block = false;
    let mut in_latex_block = false;
    let mut code_lang = String::new();
    let mut code_buf = String::new();
    let mut latex_buf = String::new();
    let mut table_buf: Vec<&str> = Vec::new();

    let raw_lines: Vec<&str> = text.lines().collect();
    let total_lines = raw_lines.len();

    for (line_idx, &raw_line) in raw_lines.iter().enumerate() {
        // 表格处理：收集以 | 开头的连续行
        if !in_code_block {
            let trimmed_for_table = raw_line.trim();
            let is_table_line =
                trimmed_for_table.starts_with('|') && trimmed_for_table.ends_with('|');

            if is_table_line {
                table_buf.push(raw_line);
                // 检查是否还有更多表格行
                let next_is_table = line_idx + 1 < total_lines && {
                    let next = raw_lines[line_idx + 1].trim();
                    next.starts_with('|') && next.ends_with('|')
                };
                if !next_is_table {
                    // 表格结束，尝试渲染
                    if let Some(table_lines) = render_table(&table_buf) {
                        lines.extend(table_lines);
                    } else {
                        // 不是有效表格，回退为普通文本
                        for tl in &table_buf {
                            lines.push(parse_inline(tl));
                        }
                    }
                    table_buf.clear();
                }
                continue;
            } else if !table_buf.is_empty() {
                // 不应该到这里（上面已处理），但保险起见
                for tl in &table_buf {
                    lines.push(parse_inline(tl));
                }
                table_buf.clear();
            }
        }

        if raw_line.trim_start().starts_with("```") {
            if !in_code_block {
                // 进入代码块：提取语言标记
                code_lang = raw_line
                    .trim_start()
                    .trim_start_matches('`')
                    .trim()
                    .to_string();
                code_buf.clear();
                lines.push(Line::from(Span::styled(
                    raw_line.to_string(),
                    Style::default().fg(THEME.muted),
                )));
                in_code_block = true;
            } else {
                // 离开代码块：尝试语法高亮
                if !code_lang.is_empty() {
                    if let Some(highlighted) = highlight_code(&code_buf, &code_lang) {
                        for hl_line in highlighted {
                            // 添加 2 空格缩进
                            let mut spans = vec![Span::raw("  ".to_string())];
                            spans.extend(hl_line.spans);
                            lines.push(Line::from(spans));
                        }
                    } else {
                        // 高亮失败：回退为纯色渲染
                        for code_line in code_buf.lines() {
                            lines.push(Line::from(Span::styled(
                                format!("  {}", code_line),
                                Style::default().fg(THEME.md_code_block),
                            )));
                        }
                    }
                } else {
                    // 无语言标记：纯色渲染
                    for code_line in code_buf.lines() {
                        lines.push(Line::from(Span::styled(
                            format!("  {}", code_line),
                            Style::default().fg(THEME.md_code_block),
                        )));
                    }
                }
                lines.push(Line::from(Span::styled(
                    raw_line.to_string(),
                    Style::default().fg(THEME.muted),
                )));
                in_code_block = false;
                code_lang.clear();
            }
            continue;
        }

        // $$ LaTeX 块级公式
        if !in_code_block && raw_line.trim() == "$$" {
            if !in_latex_block {
                in_latex_block = true;
                latex_buf.clear();
                continue;
            } else {
                // 渲染 LaTeX 块：italic + 缩进
                for formula_line in latex_buf.lines() {
                    lines.push(Line::from(Span::styled(
                        format!("  {formula_line}"),
                        Style::default()
                            .fg(THEME.md_code)
                            .add_modifier(Modifier::ITALIC),
                    )));
                }
                in_latex_block = false;
                latex_buf.clear();
                continue;
            }
        }
        if in_latex_block {
            if !latex_buf.is_empty() {
                latex_buf.push('\n');
            }
            latex_buf.push_str(raw_line);
            continue;
        }

        if in_code_block {
            if !code_buf.is_empty() {
                code_buf.push('\n');
            }
            code_buf.push_str(raw_line);
            continue;
        }

        if let Some(heading) = parse_heading(raw_line) {
            lines.push(heading);
        } else if let Some(img_lines) = parse_image_line(raw_line) {
            lines.extend(img_lines);
        } else if let Some(quote_line) = parse_blockquote(raw_line) {
            lines.push(quote_line);
        } else if let Some(list_line) = parse_list_item(raw_line) {
            lines.push(list_line);
        } else if is_horizontal_rule(raw_line) {
            let hr_width = terminal_width.min(80).max(10);
            lines.push(Line::styled(
                "─".repeat(hr_width),
                Style::default().fg(THEME.md_quote_border),
            ));
        } else {
            lines.push(parse_inline(raw_line));
        }
    }

    // 未关闭的代码块：输出已缓存内容
    if in_code_block && !code_buf.is_empty() {
        for code_line in code_buf.lines() {
            lines.push(Line::from(Span::styled(
                format!("  {}", code_line),
                Style::default().fg(THEME.md_code_block),
            )));
        }
    }

    lines
}

/// 解析 ![alt](path) 图片行
/// 如果终端支持图片协议，尝试渲染；否则降级为 [alt] 文本
fn parse_image_line(line: &str) -> Option<Vec<Line<'static>>> {
    let trimmed = line.trim();
    if !trimmed.starts_with("![") {
        return None;
    }
    let chars: Vec<char> = trimmed.chars().collect();
    // 解析 ![alt](path)
    if chars.len() < 5 || chars[0] != '!' || chars[1] != '[' {
        return None;
    }
    let mut i = 2;
    let mut alt = String::new();
    while i < chars.len() && chars[i] != ']' {
        alt.push(chars[i]);
        i += 1;
    }
    if i >= chars.len() {
        return None;
    }
    i += 1; // skip ']'
    if i >= chars.len() || chars[i] != '(' {
        return None;
    }
    i += 1; // skip '('
    let mut path = String::new();
    while i < chars.len() && chars[i] != ')' {
        path.push(chars[i]);
        i += 1;
    }
    if path.is_empty() {
        return None;
    }

    // base64 data URI 检查
    if path.starts_with("data:image/") {
        if let Some(b64_start) = path.find(";base64,") {
            let b64_data = &path[b64_start + 8..];
            if let Ok(data) =
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64_data)
            {
                if let Some((seq, rows)) = render_image(&data, 60, 20) {
                    let mut lines = vec![Line::raw(seq)];
                    for _ in 1..rows {
                        lines.push(Line::raw(""));
                    }
                    return Some(lines);
                }
            }
        }
    }

    // 本地文件路径
    if CAPS.images.is_some() {
        if let Ok(data) = std::fs::read(&path) {
            if let Some((seq, rows)) = render_image(&data, 60, 20) {
                let mut lines = vec![Line::raw(seq)];
                for _ in 1..rows {
                    lines.push(Line::raw(""));
                }
                return Some(lines);
            }
        }
    }

    // 降级：显示 [alt] 文本
    Some(vec![Line::styled(
        format!("[{alt}]"),
        Style::default().fg(THEME.dim),
    )])
}

fn parse_blockquote(line: &str) -> Option<Line<'static>> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with("> ") && trimmed != ">" && !trimmed.starts_with(">") {
        return None;
    }
    let content = if trimmed == ">" {
        ""
    } else if trimmed.starts_with("> ") {
        &trimmed[2..]
    } else {
        &trimmed[1..]
    };
    // 支持嵌套引用
    if let Some(nested) = parse_blockquote(content) {
        let mut spans = vec![Span::styled(
            "│ ",
            Style::default().fg(THEME.md_quote_border),
        )];
        spans.extend(nested.spans);
        return Some(Line::from(spans));
    }
    let inline = parse_inline(content);
    let mut spans = vec![Span::styled(
        "│ ",
        Style::default().fg(THEME.md_quote_border),
    )];
    for span in inline.spans {
        spans.push(Span::styled(
            span.content.to_string(),
            span.style.fg(THEME.md_quote).add_modifier(Modifier::ITALIC),
        ));
    }
    Some(Line::from(spans))
}

fn is_horizontal_rule(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.len() < 3 {
        return false;
    }
    let c = trimmed.chars().next().unwrap_or(' ');
    (c == '-' || c == '*' || c == '_') && trimmed.chars().all(|ch| ch == c || ch == ' ')
}

fn parse_heading(line: &str) -> Option<Line<'static>> {
    let trimmed = line.trim_start();
    let level = trimmed.chars().take_while(|&c| c == '#').count();
    if level == 0 || level > 6 {
        return None;
    }
    // 确保 # 后面有空格或内容
    if trimmed.len() > level && !trimmed.as_bytes()[level].is_ascii_whitespace() {
        return None;
    }
    let rest = trimmed[level..].trim_start();
    if rest.is_empty() && trimmed.len() == level {
        return None;
    }
    let style = match level {
        1 | 2 | 3 => Style::default()
            .fg(THEME.heading)
            .add_modifier(Modifier::BOLD),
        _ => Style::default()
            .fg(THEME.heading)
            .add_modifier(Modifier::BOLD | Modifier::DIM),
    };
    Some(Line::from(Span::styled(rest.to_string(), style)))
}

fn parse_list_item(line: &str) -> Option<Line<'static>> {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    let (bullet, rest) =
        if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
            trimmed.split_at(2)
        } else {
            let dot_pos = trimmed.find(". ")?;
            if dot_pos > 3 || !trimmed[..dot_pos].chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            trimmed.split_at(dot_pos + 2)
        };
    // 每级嵌套 4 空格缩进（对齐 TS TUI）
    let nesting_level = indent / 2;
    let prefix = " ".repeat(nesting_level * 4);

    // Task list 支持: - [x] / - [ ]
    let (task_marker, content) = if rest.starts_with("[x] ") || rest.starts_with("[X] ") {
        (Some("☑ "), &rest[4..])
    } else if rest.starts_with("[ ] ") {
        (Some("☐ "), &rest[4..])
    } else {
        (None, rest)
    };

    let inline = parse_inline(content);
    let mut spans = vec![
        Span::raw(prefix),
        Span::styled(
            bullet.to_string(),
            Style::default().fg(THEME.md_list_bullet),
        ),
    ];
    if let Some(marker) = task_marker {
        spans.push(Span::styled(
            marker.to_string(),
            Style::default().fg(THEME.accent),
        ));
    }
    spans.extend(inline.spans);
    Some(Line::from(spans))
}

/// 渲染 Markdown 表格为 box-drawing 字符边框
/// 表格至少需要 2 行（表头 + 分隔符），分隔符行格式为 |---|---|
fn render_table(table_lines: &[&str]) -> Option<Vec<Line<'static>>> {
    if table_lines.len() < 2 {
        return None;
    }

    // 验证第二行是分隔符行（|---|---|）
    let separator = table_lines[1].trim();
    let sep_cells: Vec<&str> = separator.trim_matches('|').split('|').collect();
    let is_separator = sep_cells.iter().all(|c| {
        let t = c.trim();
        t.chars().all(|ch| ch == '-' || ch == ':') && !t.is_empty()
    });
    if !is_separator {
        return None;
    }

    // 解析所有行的单元格内容
    let mut rows: Vec<Vec<String>> = Vec::new();
    for (i, line) in table_lines.iter().enumerate() {
        if i == 1 {
            continue; // 跳过分隔符行
        }
        let cells: Vec<String> = line
            .trim()
            .trim_matches('|')
            .split('|')
            .map(|c| c.trim().to_string())
            .collect();
        rows.push(cells);
    }

    if rows.is_empty() {
        return None;
    }

    // 计算列数和每列最大宽度
    let num_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    if num_cols == 0 {
        return None;
    }

    let mut col_widths: Vec<usize> = vec![0; num_cols];
    for row in &rows {
        for (col_idx, cell) in row.iter().enumerate() {
            if col_idx < num_cols {
                let width = UnicodeWidthStr::width(cell.as_str());
                if width > col_widths[col_idx] {
                    col_widths[col_idx] = width;
                }
            }
        }
    }

    // 每列最少 1 字符宽度
    for w in col_widths.iter_mut() {
        if *w == 0 {
            *w = 1;
        }
    }

    let border_style = Style::default().fg(THEME.text);
    let cell_style = Style::default().fg(THEME.text);
    let mut result: Vec<Line<'static>> = Vec::new();

    // 顶部边框：┌───┬───┐
    let mut top = String::from("┌");
    for (i, &w) in col_widths.iter().enumerate() {
        top.push_str(&"─".repeat(w + 2));
        if i < num_cols - 1 {
            top.push('┬');
        }
    }
    top.push('┐');
    result.push(Line::from(Span::styled(top, border_style)));

    for (row_idx, row) in rows.iter().enumerate() {
        // 数据行：│ cell │ cell │
        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.push(Span::styled("│".to_string(), border_style));
        for (col_idx, w) in col_widths.iter().enumerate() {
            let cell_content = row.get(col_idx).map(|s| s.as_str()).unwrap_or("");
            let cell_width = UnicodeWidthStr::width(cell_content);
            let padding = w.saturating_sub(cell_width);
            // 对 cell 内容应用 inline 格式（bold/italic/code）
            spans.push(Span::styled(" ".to_string(), cell_style));
            let cell_line = parse_inline(cell_content);
            spans.extend(cell_line.spans);
            spans.push(Span::styled(
                format!("{} ", " ".repeat(padding)),
                cell_style,
            ));
            spans.push(Span::styled("│".to_string(), border_style));
        }
        result.push(Line::from(spans));

        // 表头后的分隔线：├───┼───┤
        if row_idx == 0 && rows.len() > 1 {
            let mut mid = String::from("├");
            for (i, &w) in col_widths.iter().enumerate() {
                mid.push_str(&"─".repeat(w + 2));
                if i < num_cols - 1 {
                    mid.push('┼');
                }
            }
            mid.push('┤');
            result.push(Line::from(Span::styled(mid, border_style)));
        }
    }

    // 底部边框：└───┴───┘
    let mut bottom = String::from("└");
    for (i, &w) in col_widths.iter().enumerate() {
        bottom.push_str(&"─".repeat(w + 2));
        if i < num_cols - 1 {
            bottom.push('┴');
        }
    }
    bottom.push('┘');
    result.push(Line::from(Span::styled(bottom, border_style)));

    Some(result)
}

fn parse_inline(line: &str) -> Line<'static> {
    let spans = parse_inline_spans(line, Modifier::empty());
    Line::from(spans)
}

/// 递归解析 inline markdown，支持嵌套格式（如 `**bold *italic***`）
fn parse_inline_spans(line: &str, inherited_modifier: Modifier) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut buf = String::new();

    let base_style = if inherited_modifier.is_empty() {
        Style::default()
    } else {
        Style::default().add_modifier(inherited_modifier)
    };

    while i < len {
        if chars[i] == '`' {
            if !buf.is_empty() {
                spans.push(Span::styled(buf.clone(), base_style));
                buf.clear();
            }
            i += 1;
            let mut code = String::new();
            while i < len && chars[i] != '`' {
                code.push(chars[i]);
                i += 1;
            }
            if i < len {
                i += 1;
            }
            spans.push(Span::styled(
                code,
                Style::default()
                    .fg(THEME.md_code)
                    .add_modifier(inherited_modifier),
            ));
        } else if chars[i] == '*' && i + 1 < len && chars[i + 1] == '*' {
            if !buf.is_empty() {
                spans.push(Span::styled(buf.clone(), base_style));
                buf.clear();
            }
            i += 2;
            let mut bold_content = String::new();
            while i + 1 < len && !(chars[i] == '*' && chars[i + 1] == '*') {
                bold_content.push(chars[i]);
                i += 1;
            }
            if i + 1 < len {
                i += 2;
            }
            // 递归解析内部内容（继承 BOLD）
            let inner = parse_inline_spans(&bold_content, inherited_modifier | Modifier::BOLD);
            spans.extend(inner);
        } else if chars[i] == '~' && i + 1 < len && chars[i + 1] == '~' {
            if !buf.is_empty() {
                spans.push(Span::styled(buf.clone(), base_style));
                buf.clear();
            }
            i += 2;
            let mut strike_content = String::new();
            while i + 1 < len && !(chars[i] == '~' && chars[i + 1] == '~') {
                strike_content.push(chars[i]);
                i += 1;
            }
            if i + 1 < len {
                i += 2;
            }
            let inner =
                parse_inline_spans(&strike_content, inherited_modifier | Modifier::CROSSED_OUT);
            spans.extend(inner);
        } else if chars[i] == '=' && i + 1 < len && chars[i + 1] == '=' {
            // ==highlight== — 高亮标记
            if !buf.is_empty() {
                spans.push(Span::styled(buf.clone(), base_style));
                buf.clear();
            }
            i += 2;
            let mut hl_content = String::new();
            while i + 1 < len && !(chars[i] == '=' && chars[i + 1] == '=') {
                hl_content.push(chars[i]);
                i += 1;
            }
            if i + 1 < len {
                i += 2;
            }
            // 用反色 (REVERSED) 模拟高亮背景
            let inner = parse_inline_spans(&hl_content, inherited_modifier | Modifier::REVERSED);
            spans.extend(inner);
        } else if chars[i] == '*' {
            if !buf.is_empty() {
                spans.push(Span::styled(buf.clone(), base_style));
                buf.clear();
            }
            i += 1;
            let mut italic_content = String::new();
            while i < len && chars[i] != '*' {
                italic_content.push(chars[i]);
                i += 1;
            }
            if i < len {
                i += 1;
            }
            let inner = parse_inline_spans(&italic_content, inherited_modifier | Modifier::ITALIC);
            spans.extend(inner);
        } else if chars[i] == '_' && i + 1 < len && chars[i + 1] == '_' {
            // __bold__
            if !buf.is_empty() {
                spans.push(Span::styled(buf.clone(), base_style));
                buf.clear();
            }
            i += 2;
            let mut bold_content = String::new();
            while i + 1 < len && !(chars[i] == '_' && chars[i + 1] == '_') {
                bold_content.push(chars[i]);
                i += 1;
            }
            if i + 1 < len {
                i += 2;
            }
            let inner = parse_inline_spans(&bold_content, inherited_modifier | Modifier::BOLD);
            spans.extend(inner);
        } else if chars[i] == '_' && (i == 0 || chars[i - 1].is_whitespace() || chars[i - 1] == '(')
        {
            // _italic_ — only at word boundary
            if !buf.is_empty() {
                spans.push(Span::styled(buf.clone(), base_style));
                buf.clear();
            }
            i += 1;
            let mut italic_content = String::new();
            while i < len && chars[i] != '_' {
                italic_content.push(chars[i]);
                i += 1;
            }
            if i < len {
                i += 1;
            }
            if !italic_content.is_empty() {
                let inner =
                    parse_inline_spans(&italic_content, inherited_modifier | Modifier::ITALIC);
                spans.extend(inner);
            }
        } else if chars[i] == '$'
            && i + 1 < len
            && !chars[i + 1].is_whitespace()
            && chars[i + 1] != '$'
        {
            // LaTeX 公式：$inline$ — 要求 $ 后紧跟非空白字符（避免与货币符号冲突）
            if !buf.is_empty() {
                spans.push(Span::styled(buf.clone(), base_style));
                buf.clear();
            }
            i += 1;
            let mut formula = String::new();
            while i < len && chars[i] != '$' {
                formula.push(chars[i]);
                i += 1;
            }
            if i < len && !formula.is_empty() && !formula.ends_with(' ') {
                i += 1;
                spans.push(Span::styled(
                    formula,
                    Style::default()
                        .fg(THEME.md_code)
                        .add_modifier(Modifier::ITALIC | inherited_modifier),
                ));
            } else {
                // 不是有效 LaTeX，回退为普通文本
                buf.push('$');
                buf.push_str(&formula);
            }
        } else if chars[i] == '[' {
            // [text](url) 链接语法 — 使用 OSC 8 超链接（如终端支持）
            if let Some((text, url, end)) = parse_link_at(&chars, i) {
                if !buf.is_empty() {
                    spans.push(Span::styled(buf.clone(), base_style));
                    buf.clear();
                }
                spans.extend(render_link_spans(&text, &url));
                i = end;
            } else {
                buf.push(chars[i]);
                i += 1;
            }
        } else {
            buf.push(chars[i]);
            i += 1;
        }
    }

    if !buf.is_empty() {
        spans.push(Span::styled(buf, base_style));
    }

    spans
}

/// 尝试解析 [text](url) 链接，返回 (text, url, end_index)
fn parse_link_at(chars: &[char], start: usize) -> Option<(String, String, usize)> {
    let len = chars.len();
    if start >= len || chars[start] != '[' {
        return None;
    }
    let mut i = start + 1;
    let mut text = String::new();
    while i < len && chars[i] != ']' {
        if chars[i] == '\n' {
            return None;
        }
        text.push(chars[i]);
        i += 1;
    }
    if i >= len || text.is_empty() {
        return None;
    }
    i += 1; // skip ']'
    if i >= len || chars[i] != '(' {
        return None;
    }
    i += 1; // skip '('
    let mut url = String::new();
    let mut paren_depth = 1;
    while i < len && paren_depth > 0 {
        if chars[i] == '\n' {
            return None;
        }
        if chars[i] == '(' {
            paren_depth += 1;
            url.push(chars[i]);
        } else if chars[i] == ')' {
            paren_depth -= 1;
            if paren_depth > 0 {
                url.push(chars[i]);
            }
        } else {
            url.push(chars[i]);
        }
        i += 1;
    }
    if paren_depth != 0 || url.is_empty() {
        return None;
    }
    Some((text, url, i))
}
