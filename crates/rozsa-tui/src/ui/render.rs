// ui/render.rs — 所有渲染实现的辅助函数库
//
// Internal Framework:
// render.rs
// ├── reorder_messages_for_display()  消息排序
// ├── display_width()                 显示宽度计算
// ├── has_at_completion_token()       完成符检测
// ├── wrap_text_lines()               文本行换行
// ├── WordSplitter                    单词分割迭代器
// ├── bash_execution_lines()          bash 执行块格式化
// ├── summary_block_lines()           摘要块行生成
// ├── compaction_summary_lines()      压缩摘要行
// ├── compaction_collapsed_lines()    压缩折叠行
// ├── branch_summary_lines()          分支摘要行
// ├── tool_result_message_lines()     工具结果行
// ├── render_tool_output_lines()      工具输出行渲染
// ├── custom_message_lines()          自定义消息行
// ├── spinner_char()                  状态行 spinner 字符
// ├── append_content_lines()          内容行追加
// ├── append_assistant_text_lines()   助手文本行追加
// ├── wrap_line_with_prefix()         前缀行折行
// └── padded_hint_line()              带缩进提示行
//
// Related Docs:
// - [TUI Design](../../../docs/rozsa_framework.md#rozsa-tui--ratatui-终端前端)
// - [ui/mod.rs](./mod.rs)

use std::collections::BTreeMap;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
};
use serde_json::Value;

use crate::{
    app::{AppState, DialogState},
    input::InputState,
    protocol::NativeUiState,
    theme::THEME,
};

use super::cached_message_lines;
use crate::components::sidebar::truncate;

/// 将后端的 messages 数组重排为 UI 展示顺序。
/// 后端为了 LLM 上下文将 compactionSummary 放在 index 0，
/// UI 展示时按 timestamp 将其插入到 preserved messages 之后、新消息之前。
fn reorder_messages_for_display(messages: &[Value]) -> Vec<&Value> {
    if messages.is_empty() {
        return vec![];
    }
    let first = &messages[0];
    let is_compaction = first
        .get("role")
        .and_then(|v| v.as_str())
        == Some("compactionSummary");

    if !is_compaction {
        return messages.iter().collect();
    }

    let compaction_ts = first
        .get("timestamp")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let rest = &messages[1..];
    // 找到第一个 timestamp > compaction_ts 的消息（即 compact 之后的新消息）
    let insert_pos = rest
        .iter()
        .position(|m| {
            m.get("timestamp")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0)
                > compaction_ts
        })
        .unwrap_or(rest.len());

    let mut result = Vec::with_capacity(messages.len());
    // preserved messages (timestamp <= compaction_ts)
    for msg in &rest[..insert_pos] {
        result.push(msg);
    }
    // compactionSummary at its chronological position
    result.push(first);
    // new messages (timestamp > compaction_ts)
    for msg in &rest[insert_pos..] {
        result.push(msg);
    }
    result
}

pub(super) fn render_messages(frame: &mut ratatui::Frame<'_>, area: Rect, state: &AppState) {
    let mut all_lines = Vec::<Line<'static>>::new();
    let msg_width = area.width as usize;

    // 按时间顺序重排消息：后端把 compactionSummary 放在 index 0（LLM 需要先读摘要），
    // 但 UI 展示时应按时间戳插入正确位置（preserved msgs → compaction → new msgs）
    let messages = reorder_messages_for_display(&state.ui.messages);
    let msg_count = messages.len();

    for (i, message) in messages.iter().enumerate() {
        let is_last_streaming = state.ui.is_streaming && i == msg_count - 1;
        all_lines.extend(cached_message_lines(
            message,
            state.tools_expanded,
            state.thinking_visible,
            state.show_images,
            state.compaction_collapsed,
            is_last_streaming,
            msg_width,
        ));
    }
    // 长通知（>3行）作为消息区域内容渲染
    for notification in state
        .notifications
        .iter()
        .filter(|n| n.message.lines().count() > 3)
    {
        let color = match notification.level.as_str() {
            "error" => THEME.error,
            "warning" => THEME.warning,
            _ => THEME.text,
        };
        all_lines.push(Line::raw(""));
        for line in notification.message.lines() {
            all_lines.push(Line::styled(line.to_string(), Style::default().fg(color)));
        }
    }

    if let Some(error) = &state.ui.error {
        all_lines.push(Line::styled(
            error.clone(),
            Style::default().fg(THEME.error),
        ));
    }
    // streaming/compacting/retry 指示器移到输入框上方独立渲染

    // 行级滚动
    let visible_height = area.height as usize;
    let total = all_lines.len();
    let max_scroll = total.saturating_sub(visible_height);
    let scroll = state.scroll.min(max_scroll);
    let start = total.saturating_sub(visible_height + scroll);
    let end = total.saturating_sub(scroll);
    let has_above = start > 0;
    let has_below = scroll > 0;
    // 指示器占行，需要从内容中扣除对应空间
    let indicator_lines = (has_above as usize) + (has_below as usize);
    let content_take = (end.saturating_sub(start)).saturating_sub(indicator_lines);
    let content_skip = if has_above {
        start + (end.saturating_sub(start)).saturating_sub(content_take)
    } else {
        start
    };
    let mut visible_lines: Vec<Line<'static>> = Vec::new();
    if has_above {
        visible_lines.push(Line::styled(
            format!("↑ {} lines above", content_skip),
            Style::default().fg(THEME.muted),
        ));
    }
    visible_lines.extend(all_lines.into_iter().skip(content_skip).take(content_take));
    if has_below {
        visible_lines.push(Line::styled(
            format!("↓ {scroll} lines below"),
            Style::default().fg(THEME.muted),
        ));
    }
    frame.render_widget(Paragraph::new(visible_lines), area);
}

pub(super) fn render_notifications(frame: &mut ratatui::Frame<'_>, area: Rect, state: &AppState) {
    if area.height == 0 {
        return;
    }
    // 只渲染短通知（<=3行）在顶部，取最后几条
    let short: Vec<&crate::app::Notification> = state
        .notifications
        .iter()
        .filter(|n| n.message.lines().count() <= 3)
        .collect();
    let skip = short.len().saturating_sub(area.height as usize);
    let lines: Vec<Line<'static>> = short
        .into_iter()
        .skip(skip)
        .map(|n| {
            let color = match n.level.as_str() {
                "error" => Color::Red,
                "warning" => Color::Yellow,
                _ => Color::DarkGray,
            };
            Line::styled(n.message.clone(), Style::default().fg(color))
        })
        .collect();
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

pub(super) fn render_pending(frame: &mut ratatui::Frame<'_>, area: Rect, ui: &NativeUiState) {
    if area.height == 0 || ui.pending_messages.is_empty() {
        return;
    }
    let mut lines = vec![Line::styled(
        "Queued messages",
        Style::default()
            .fg(THEME.warning)
            .add_modifier(Modifier::BOLD),
    )];
    for message in &ui.pending_messages {
        lines.push(Line::raw(format!(
            "  {}",
            truncate(message, area.width.saturating_sub(3) as usize)
        )));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// 状态行：贴在输入框上方，动态 spinner + 状态文字
pub(super) fn render_status(frame: &mut ratatui::Frame<'_>, area: Rect, state: &AppState) {
    if area.height == 0 {
        return;
    }
    let spin = spinner_char();
    let line = if state.compacting {
        Line::from(vec![
            Span::styled(format!("{spin} "), Style::default().fg(THEME.accent)),
            Span::styled("Compacting session...", Style::default().fg(THEME.muted)),
        ])
    } else if let Some(retry) = &state.retry {
        let remaining = retry.remaining();
        Line::from(vec![
            Span::styled(format!("{spin} "), Style::default().fg(THEME.warning)),
            Span::styled(
                format!("Retrying in {}s: {}", remaining, retry.reason),
                Style::default().fg(THEME.muted),
            ),
        ])
    } else if state.ui.is_streaming {
        // 优先用 status["working"] (后端推送的动态 working message)
        let msg = state
            .ui
            .status
            .get("working")
            .map(|s| s.as_str())
            .or(state.working_message.as_deref())
            .unwrap_or("Working...");
        Line::from(vec![
            Span::styled(format!("{spin} "), Style::default().fg(THEME.accent)),
            Span::styled(msg.to_string(), Style::default().fg(THEME.muted)),
        ])
    } else {
        return;
    };
    frame.render_widget(Paragraph::new(vec![line]), area);
}

pub(super) fn render_widgets(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    widgets: &BTreeMap<String, Vec<String>>,
) {
    if area.height == 0 || widgets.is_empty() {
        return;
    }
    let mut lines = Vec::new();
    for widget_lines in widgets.values() {
        for line in widget_lines {
            if line.starts_with('[') && line.ends_with(']') {
                lines.push(Line::styled(
                    line.clone(),
                    Style::default().add_modifier(Modifier::BOLD),
                ));
            } else {
                lines.push(Line::styled(line.clone(), Style::default().fg(THEME.muted)));
            }
        }
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
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

pub(super) fn render_dialog(frame: &mut ratatui::Frame<'_>, area: Rect, dialog: &DialogState) {
    frame.render_widget(Clear, area);
    // 可用行数（减去边框 2 行 + 可能的 message 1 行）
    let header_lines: u16 = if dialog.message.is_some() { 1 } else { 0 };
    let visible_height = area.height.saturating_sub(2 + header_lines) as usize;

    let mut lines: Vec<Line<'_>> = Vec::new();
    if let Some(message) = &dialog.message {
        lines.push(Line::raw(message.clone()));
    }
    if dialog.kind == "select" || dialog.kind == "confirm" {
        let options = if dialog.kind == "confirm" && dialog.options.is_empty() {
            vec!["Yes".to_string(), "No".to_string()]
        } else {
            dialog.options.clone()
        };
        // 滚动：保证 selected 在可见窗口内
        let total = options.len();
        let scroll_offset = if visible_height == 0 || total <= visible_height {
            0
        } else {
            dialog.selected.saturating_sub(visible_height / 2).min(total.saturating_sub(visible_height))
        };
        let end = (scroll_offset + visible_height).min(total);
        if scroll_offset > 0 {
            lines.push(Line::styled("  ↑ more", Style::default().fg(Color::DarkGray)));
        }
        for index in scroll_offset..end {
            let option = &options[index];
            let marker = if index == dialog.selected { "> " } else { "  " };
            let style = if index == dialog.selected {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            lines.push(Line::styled(format!("{marker}{option}"), style));
        }
        if end < total {
            lines.push(Line::styled("  ↓ more", Style::default().fg(Color::DarkGray)));
        }
    } else {
        lines.push(Line::raw(dialog.input.clone()));
    }
    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(Block::default().borders(Borders::ALL).title(dialog.title.clone()));
    frame.render_widget(paragraph, area);
}

/// 计算字符串显示宽度（CJK 字符占 2 列）
fn display_width(s: &str) -> usize {
    use unicode_width::UnicodeWidthChar;
    s.chars()
        .map(|c| UnicodeWidthChar::width(c).unwrap_or(0))
        .sum()
}

fn has_at_completion_token(text: &str) -> bool {
    text.split_whitespace().any(|token| token.starts_with('@'))
}

/// 按显示宽度对文本行做 word-wrap，返回 wrapped 后的行列表
fn wrap_text_lines(lines: &[String], max_width: usize) -> Vec<String> {
    use unicode_width::UnicodeWidthChar;
    let mut result = Vec::new();
    for line in lines {
        if line.is_empty() {
            result.push(String::new());
            continue;
        }
        let mut current = String::new();
        let mut current_width: usize = 0;
        for word in WordSplitter::new(line) {
            let word_width = display_width(&word);
            if current_width == 0 && word_width <= max_width {
                current.push_str(&word);
                current_width = word_width;
            } else if current_width + word_width <= max_width {
                current.push_str(&word);
                current_width += word_width;
            } else if current_width == 0 {
                // 单个 word 超宽（如无空格的 CJK 长文本），按字符逐个断行
                for ch in word.chars() {
                    let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
                    if current_width + cw > max_width && current_width > 0 {
                        result.push(current);
                        current = String::new();
                        current_width = 0;
                    }
                    current.push(ch);
                    current_width += cw;
                }
            } else {
                // 当前行放不下这个 word，先换行
                result.push(current);
                let trimmed = word.trim_start();
                let trimmed_width = display_width(trimmed);
                if trimmed_width <= max_width {
                    current = trimmed.to_string();
                    current_width = trimmed_width;
                } else {
                    // 新行也放不下，按字符断行
                    current = String::new();
                    current_width = 0;
                    for ch in trimmed.chars() {
                        let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
                        if current_width + cw > max_width && current_width > 0 {
                            result.push(current);
                            current = String::new();
                            current_width = 0;
                        }
                        current.push(ch);
                        current_width += cw;
                    }
                }
            }
        }
        result.push(current);
    }
    result
}

/// 按空格和单词边界分割文本，保留空格在 token 前面
struct WordSplitter<'a> {
    remainder: &'a str,
}

impl<'a> WordSplitter<'a> {
    fn new(s: &'a str) -> Self {
        Self { remainder: s }
    }
}

impl<'a> Iterator for WordSplitter<'a> {
    type Item = String;
    fn next(&mut self) -> Option<String> {
        if self.remainder.is_empty() {
            return None;
        }
        // 收集前导空格 + 非空格字符作为一个 token
        let mut end = 0;
        let mut chars = self.remainder.chars();
        // 前导空格
        while let Some(c) = chars.clone().next() {
            if c == ' ' {
                end += c.len_utf8();
                chars.next();
            } else {
                break;
            }
        }
        // 非空格字符
        for c in chars {
            if c == ' ' {
                break;
            }
            end += c.len_utf8();
        }
        if end == 0 {
            end = self.remainder.len();
        }
        let token = self.remainder[..end].to_string();
        self.remainder = &self.remainder[end..];
        Some(token)
    }
}

pub(super) fn message_lines(
    message: &Value,
    tools_expanded: bool,
    thinking_visible: bool,
    show_images: bool,
    compaction_collapsed: bool,
    is_last_streaming: bool,
    width: usize,
) -> Vec<Line<'static>> {
    let role = message
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or("message");

    match role {
        "bashExecution" => {
            let mut lines = vec![Line::raw("")];
            lines.extend(bash_execution_lines(message, width));
            return lines;
        }
        "compactionSummary" => {
            let mut lines = vec![Line::raw("")];
            if compaction_collapsed {
                lines.extend(compaction_collapsed_lines(message));
            } else {
                lines.extend(compaction_summary_lines(message));
            }
            return lines;
        }
        "branchSummary" => {
            let mut lines = vec![Line::raw("")];
            lines.extend(branch_summary_lines(message));
            return lines;
        }
        "custom" => return custom_message_lines(message),
        "toolResult" => {
            // 不加空行分隔 — 与前面的 toolCall 形成连续的 Box
            return tool_result_message_lines(message, tools_expanded, width);
        }
        _ => {}
    }

    // 消息间空行分隔
    let mut lines = vec![Line::raw("")];

    if role == "user" {
        // 用户消息：Codex 风格 — "› " bold+dim 前缀 + 背景色 + word-wrap
        let user_bg = Style::default()
            .bg(THEME.user_message_bg)
            .fg(THEME.user_msg);
        let prefix_width: usize = 2; // "› " 或 "  "
        let wrap_width = width.saturating_sub(prefix_width).max(10);

        // 收集文本行（统一处理 \r\n、\r、\n 换行）
        // 优先使用 displayText（如 skill 展开后保留原始输入供显示）
        let mut text_lines: Vec<String> = Vec::new();
        if let Some(display) = message.get("displayText").and_then(Value::as_str) {
            let normalized = crate::normalize_newlines(display);
            for line in normalized.lines() {
                text_lines.push(line.to_string());
            }
        } else if let Some(content) = message.get("content").and_then(Value::as_array) {
            for item in content {
                if let Some(text) = item.get("text").and_then(Value::as_str) {
                    let normalized = crate::normalize_newlines(text);
                    for line in normalized.lines() {
                        text_lines.push(line.to_string());
                    }
                    if normalized.ends_with('\n') {
                        text_lines.push(String::new());
                    }
                }
            }
        } else if let Some(text) = message.get("content").and_then(Value::as_str) {
            let normalized = crate::normalize_newlines(text);
            for line in normalized.lines() {
                text_lines.push(line.to_string());
            }
        }

        // Word-wrap 并添加前缀
        let wrapped = wrap_text_lines(&text_lines, wrap_width);
        // 顶部 padding — 填满背景
        let pad_line = " ".repeat(width);
        lines.push(Line::styled(pad_line.clone(), user_bg));
        for (i, wrapped_line) in wrapped.iter().enumerate() {
            let prefix = if i == 0 { "› " } else { "  " };
            let prefix_span = if i == 0 {
                Span::styled(
                    prefix,
                    Style::default()
                        .bg(THEME.user_message_bg)
                        .add_modifier(Modifier::BOLD | Modifier::DIM),
                )
            } else {
                Span::styled(prefix, user_bg)
            };
            // 内容 + 右填充到满宽
            let content_width = display_width(wrapped_line);
            let right_pad = width.saturating_sub(prefix_width + content_width);
            let mut spans = vec![prefix_span, Span::styled(wrapped_line.clone(), user_bg)];
            if right_pad > 0 {
                spans.push(Span::styled(" ".repeat(right_pad), user_bg));
            }
            lines.push(Line::from(spans));
        }
        // 底部 padding
        lines.push(Line::styled(pad_line, user_bg));
    } else {
        // Assistant 消息：无 role 标签，内容有 1 格左缩进
        if let Some(content) = message.get("content").and_then(Value::as_array) {
            let content_len = content.len();
            let mut last_tool_is_bash = false;
            for (idx, item) in content.iter().enumerate() {
                let is_last_item = is_last_streaming && idx == content_len - 1;
                let item_type = item.get("type").and_then(Value::as_str).unwrap_or("");
                if item_type == "toolCall" {
                    let name = item.get("name").and_then(Value::as_str).unwrap_or("");
                    last_tool_is_bash =
                        name.eq_ignore_ascii_case("bash") || name.eq_ignore_ascii_case("shell");
                }
                // 只看紧邻的下一个元素是否是 toolResult
                let result_info = if item_type == "toolCall" {
                    content.get(idx + 1).and_then(|next| {
                        if next.get("type").and_then(Value::as_str) == Some("toolResult") {
                            let is_err = next
                                .get("isError")
                                .and_then(Value::as_bool)
                                .unwrap_or(false);
                            Some(is_err)
                        } else {
                            None
                        }
                    })
                } else {
                    None
                };
                append_content_lines(
                    &mut lines,
                    item,
                    tools_expanded,
                    thinking_visible,
                    show_images,
                    is_last_item,
                    role,
                    last_tool_is_bash,
                    result_info,
                    width,
                );
                if item_type == "toolResult" {
                    last_tool_is_bash = false;
                }
            }
        } else if let Some(text) = message.get("content").and_then(Value::as_str) {
            append_assistant_text_lines(&mut lines, text, width);
        }
    }
    lines
}

fn bash_execution_lines(message: &Value, width: usize) -> Vec<Line<'static>> {
    let command = message.get("command").and_then(Value::as_str).unwrap_or("");
    let output = message.get("output").and_then(Value::as_str).unwrap_or("");
    let exit_code = message.get("exitCode").and_then(Value::as_i64);
    let cancelled = message
        .get("cancelled")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let bg = if exit_code.is_some_and(|c| c != 0) {
        THEME.tool_error_bg
    } else {
        THEME.tool_success_bg
    };
    let block_style = Style::default().bg(bg);
    let pad_line = " ".repeat(width);

    // Box(1,1) 风格：顶部 padding
    let mut lines = vec![Line::styled(pad_line.clone(), block_style)];

    // $ command — 超宽时 wrap
    let cmd_display = format!(" $ {command}");
    let cmd_style = Style::default()
        .bg(bg)
        .fg(THEME.text)
        .add_modifier(Modifier::BOLD);
    lines.extend(wrap_padded_line(&cmd_display, cmd_style, block_style, width));

    // 输出内容 — 按渲染行数折叠
    let rendered_output = render_tool_output_lines(output, bg, width, false);
    lines.extend(rendered_output);

    // 状态
    if cancelled {
        lines.push(
            Line::styled("  (cancelled)", Style::default().fg(THEME.warning)).style(block_style),
        );
    } else if let Some(code) = exit_code {
        if code != 0 {
            lines.push(
                Line::styled(format!("  (exit {code})"), Style::default().fg(THEME.error))
                    .style(block_style),
            );
        }
    }

    // 底部 padding
    lines.push(Line::styled(pad_line, block_style));
    lines
}

fn summary_block_lines(message: &Value, label: &str) -> Vec<Line<'static>> {
    let summary = message.get("summary").and_then(Value::as_str).unwrap_or("");
    let bg = THEME.custom_message_bg;
    let block_style = Style::default().bg(bg);
    let mut lines = vec![
        Line::raw("").style(block_style),
        Line::from(vec![
            Span::raw(" "),
            Span::styled(
                format!("[{label}]"),
                Style::default()
                    .fg(THEME.custom_message_label)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
        .style(block_style),
    ];
    let all_lines: Vec<&str> = summary.lines().collect();
    let max_display = 40;
    let display_count = all_lines.len().min(max_display);
    for line in &all_lines[..display_count] {
        lines.push(
            Line::from(vec![
                Span::raw(" "),
                Span::styled(line.to_string(), Style::default().fg(THEME.text)),
            ])
            .style(block_style),
        );
    }
    if all_lines.len() > max_display {
        lines.push(
            Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    format!("  ... +{} more lines", all_lines.len() - max_display),
                    Style::default().fg(THEME.muted),
                ),
            ])
            .style(block_style),
        );
    }
    lines.push(Line::raw("").style(block_style));
    lines
}

fn compaction_summary_lines(message: &Value) -> Vec<Line<'static>> {
    summary_block_lines(message, "compaction")
}

fn compaction_collapsed_lines(message: &Value) -> Vec<Line<'static>> {
    let summary = message.get("summary").and_then(Value::as_str).unwrap_or("");
    let line_count = summary.lines().count();
    vec![Line::from(vec![
        Span::styled(
            format!("  [compaction] ({line_count} lines — Ctrl+O to expand)"),
            Style::default().fg(THEME.muted),
        ),
    ])]
}

fn branch_summary_lines(message: &Value) -> Vec<Line<'static>> {
    summary_block_lines(message, "branch")
}

fn tool_result_message_lines(
    message: &Value,
    tools_expanded: bool,
    width: usize,
) -> Vec<Line<'static>> {
    let tool_name = message
        .get("toolName")
        .and_then(Value::as_str)
        .unwrap_or("tool");
    let is_error = message
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    // 与 toolCall 完全一样的背景色
    let _ = is_error;
    let bg_color = THEME.tool_pending_bg;
    let block_style = Style::default().bg(bg_color);
    let pad_line = " ".repeat(width);

    let mut lines: Vec<Line<'static>> = Vec::new();
    // 不显示顶部 padding — 由前面的 toolCall 提供
    let _ = tool_name;
    // 错误时显示标记
    if is_error {
        lines.push(padded_colored_line(
            " (error)",
            bg_color,
            THEME.error,
            width,
        ));
    }

    if let Some(content) = message.get("content").and_then(Value::as_array) {
        for item in content {
            if let Some(text) = item.get("text").and_then(Value::as_str) {
                let rendered = render_tool_output_lines(text, bg_color, width, tools_expanded);
                lines.extend(rendered);
            }
        }
    }
    // 底部 padding — 关闭块
    lines.push(Line::styled(pad_line, block_style));
    lines
}

/// 渲染工具输出文本，按渲染行数判断折叠
fn render_tool_output_lines(
    text: &str,
    bg: Color,
    width: usize,
    tools_expanded: bool,
) -> Vec<Line<'static>> {
    let max_rendered = if tools_expanded { 50 } else { 20 };

    // 先渲染全部行（带 wrap）
    let source_lines: Vec<&str> = text.lines().collect();
    let mut all_rendered: Vec<Line<'static>> = Vec::new();
    for line in &source_lines {
        all_rendered.extend(render_result_line_bg(line, bg, width));
    }

    let total_rendered = all_rendered.len();
    if total_rendered <= max_rendered {
        return all_rendered;
    }

    if !tools_expanded {
        // 折叠：显示前 5 渲染行 + hint + 后 5 渲染行
        let head = 5.min(total_rendered);
        let tail = 5.min(total_rendered.saturating_sub(head));
        let hidden = total_rendered.saturating_sub(head + tail);
        let mut result: Vec<Line<'static>> = all_rendered[..head].to_vec();
        result.push(padded_hint_line(
            &format!(" … ({hidden} lines hidden — Ctrl-O expand)"),
            bg,
            width,
        ));
        result.extend(all_rendered[total_rendered - tail..].to_vec());
        result
    } else {
        // 展开模式但超过 50：截断
        let mut result: Vec<Line<'static>> = all_rendered[..max_rendered].to_vec();
        result.push(padded_hint_line(
            &format!(" … ({} more lines)", total_rendered - max_rendered),
            bg,
            width,
        ));
        result
    }
}

fn custom_message_lines(message: &Value) -> Vec<Line<'static>> {
    let custom_type = message
        .get("customType")
        .and_then(Value::as_str)
        .unwrap_or("custom");
    let display = message
        .get("display")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    if !display {
        return Vec::new();
    }
    let bg = THEME.custom_message_bg;
    let block_style = Style::default().bg(bg);

    let mut lines = vec![Line::raw(""), Line::raw("").style(block_style)];
    // Label: [type]
    lines.push(
        Line::from(vec![
            Span::raw(" "),
            Span::styled(
                format!("[{custom_type}]"),
                Style::default()
                    .fg(THEME.custom_message_label)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
        .style(block_style),
    );
    if let Some(text) = message.get("content").and_then(Value::as_str) {
        for line in text.lines().take(20) {
            lines.push(
                Line::from(vec![
                    Span::raw(" "),
                    Span::styled(line.to_string(), Style::default().fg(THEME.text)),
                ])
                .style(block_style),
            );
        }
    } else if let Some(content) = message.get("content").and_then(Value::as_array) {
        for item in content.iter().take(5) {
            if let Some(text) = item.get("text").and_then(Value::as_str) {
                for line in text.lines().take(10) {
                    lines.push(
                        Line::from(vec![
                            Span::raw(" "),
                            Span::styled(line.to_string(), Style::default().fg(THEME.text)),
                        ])
                        .style(block_style),
                    );
                }
            }
        }
    }
    // 底部 padding
    lines.push(Line::raw("").style(block_style));
    lines
}

/// Spinner 字符序列（Braille 动画）
const SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// 获取当前 spinner 字符，基于系统时间
fn spinner_char() -> char {
    use std::time::{SystemTime, UNIX_EPOCH};
    let tick = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_millis()
        / 100;
    SPINNER_FRAMES[tick as usize % SPINNER_FRAMES.len()]
}

fn append_content_lines(
    lines: &mut Vec<Line<'static>>,
    item: &Value,
    tools_expanded: bool,
    thinking_visible: bool,
    show_images: bool,
    is_last_streaming: bool,
    _role: &str,
    _is_bash_context: bool,
    result_info: Option<bool>, // Some(is_error) 表示有 result 及其状态，None 表示无 result
    width: usize,
) {
    let item_type = item.get("type").and_then(Value::as_str).unwrap_or("");
    match item_type {
        "text" => {
            if let Some(text) = item.get("text").and_then(Value::as_str) {
                append_assistant_text_lines(lines, text, width);
            }
        }
        "thinking" => {
            if !thinking_visible {
                lines.push(Line::styled(
                    "  (thinking hidden — Ctrl-T to show)",
                    Style::default().fg(THEME.muted),
                ));
            } else if let Some(text) = item
                .get("thinking")
                .and_then(Value::as_str)
                .or_else(|| item.get("text").and_then(Value::as_str))
            {
                if item
                    .get("redacted")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    lines.push(Line::styled(
                        "  (thinking redacted)",
                        Style::default().fg(THEME.muted),
                    ));
                } else {
                    for line in wrap_text_lines(
                        &text.lines().map(str::to_string).collect::<Vec<_>>(),
                        width.saturating_sub(1).max(10),
                    ) {
                        lines.push(Line::from(vec![
                            Span::raw(" "),
                            Span::styled(line, Style::default().fg(THEME.muted)),
                        ]));
                    }
                }
            }
        }
        "toolCall" => {
            let name = item.get("name").and_then(Value::as_str).unwrap_or("tool");
            let params = item.get("arguments");
            let preview = tool_call_preview(name, params);
            let is_bash = name.eq_ignore_ascii_case("bash") || name.eq_ignore_ascii_case("shell");

            // toolCall 统一用 pending_bg（中性深灰 box）
            let bg = THEME.tool_pending_bg;
            let block_style = Style::default().bg(bg);
            let pad_line = " ".repeat(width);
            lines.push(Line::raw("")); // Spacer(1)
                                       // 顶部 padding
            lines.push(Line::styled(pad_line.clone(), block_style));

            if is_bash {
                // Bash: "$ command" bold — 超宽时 wrap
                let command = params
                    .and_then(|p| p.get("command"))
                    .and_then(Value::as_str)
                    .unwrap_or(&preview);
                let command = command.replace('\t', "    ");
                let display = if is_last_streaming {
                    let spin = spinner_char();
                    format!(" {spin} $ {command}")
                } else {
                    format!(" $ {command}")
                };
                let cmd_style = Style::default()
                    .bg(bg)
                    .fg(THEME.text)
                    .add_modifier(Modifier::BOLD);
                lines.extend(wrap_padded_line(&display, cmd_style, block_style, width));
            } else {
                // 其他工具: "toolName  preview" — 超宽时 wrap
                let preview_clean = preview.replace('\t', "    ");
                let content_text = format!(" {}  {}", name, preview_clean);
                let content_style = Style::default().bg(bg).fg(THEME.muted);
                let name_style = Style::default()
                    .bg(bg)
                    .fg(THEME.text)
                    .add_modifier(Modifier::BOLD);
                let dw = display_width(&content_text);
                if dw <= width {
                    let rp = width.saturating_sub(dw);
                    lines.push(Line::from(vec![
                        Span::styled(" ", block_style),
                        Span::styled(name.to_string(), name_style),
                        Span::styled(format!("  {preview_clean}"), content_style),
                        Span::styled(" ".repeat(rp), block_style),
                    ]));
                } else {
                    lines.extend(wrap_padded_line(
                        &content_text,
                        content_style,
                        block_style,
                        width,
                    ));
                }
            }
            // 没有 result 时关闭块（有 result 时由 toolResult 关闭）
            if result_info.is_none() {
                lines.push(Line::styled(pad_line, block_style));
            }
        }
        "toolResult" => {
            let is_error = item
                .get("isError")
                .and_then(Value::as_bool)
                .unwrap_or(false);

            // 与 toolCall 完全一样的背景色
            let bg = THEME.tool_pending_bg;
            let block_style = Style::default().bg(bg);
            let pad_line = " ".repeat(width);

            if is_error {
                lines.push(padded_colored_line(" (error)", bg, THEME.error, width));
            }

            if let Some(text) = item
                .get("content")
                .and_then(Value::as_array)
                .and_then(|arr| arr.first())
                .and_then(|c| c.get("text"))
                .and_then(Value::as_str)
            {
                let rendered = render_tool_output_lines(text, bg, width, tools_expanded);
                lines.extend(rendered);
            } else {
                lines.push(padded_hint_line(" (no output)", bg, width));
            }
            // 底部 padding — 关闭整个 toolCall+toolResult 块
            lines.push(Line::styled(pad_line, block_style));
        }
        "image" => {
            if !show_images {
                lines.push(Line::styled(
                    "  (image hidden)",
                    Style::default().fg(THEME.muted),
                ));
            } else {
                // 尝试从 content 中提取 base64 图片数据并渲染
                let rendered = item
                    .get("source")
                    .and_then(|s| s.get("data"))
                    .and_then(Value::as_str)
                    .and_then(|b64| {
                        base64::Engine::decode(
                            &base64::engine::general_purpose::STANDARD,
                            b64,
                        )
                        .ok()
                    })
                    .and_then(|data| {
                        crate::terminal_image::render_image(&data, 60, 20)
                    });
                if let Some((seq, rows)) = rendered {
                    lines.push(Line::raw(format!("  {seq}")));
                    for _ in 1..rows {
                        lines.push(Line::raw(""));
                    }
                } else {
                    lines.push(Line::styled("  [image]", Style::default().fg(THEME.muted)));
                }
            }
        }
        _ => {
            if !item_type.is_empty() {
                lines.push(Line::styled(
                    format!("  [{item_type}]"),
                    Style::default().fg(THEME.muted),
                ));
            }
        }
    }
}

fn append_assistant_text_lines(lines: &mut Vec<Line<'static>>, text: &str, width: usize) {
    let parsed = crate::markdown::parse_markdown_with_width(text, width);
    for line in parsed {
        lines.extend(wrap_line_with_prefix(line, " ", width));
    }
}

fn wrap_line_with_prefix(line: Line<'static>, prefix: &str, width: usize) -> Vec<Line<'static>> {
    let prefix_width = display_width(prefix);
    let min_content_width = 10;
    if width <= prefix_width + min_content_width {
        let mut spans = vec![Span::raw(prefix.to_string())];
        spans.extend(line.spans);
        return vec![Line::from(spans)];
    }

    let max_content = width - prefix_width;
    let mut result = Vec::new();
    let mut current_spans: Vec<Span<'static>> = vec![Span::raw(prefix.to_string())];
    let mut current_width = prefix_width;
    // 用于 word-boundary 回溯
    let mut word_buf = String::new();
    let mut word_style = Style::default();
    let mut word_width: usize = 0;

    let flush_word = |current_spans: &mut Vec<Span<'static>>,
                      current_width: &mut usize,
                      result: &mut Vec<Line<'static>>,
                      word: &mut String,
                      w_width: &mut usize,
                      style: Style,
                      prefix: &str,
                      prefix_width: usize,
                      max_content: usize| {
        if word.is_empty() {
            return;
        }
        if *current_width > prefix_width && *current_width + *w_width > prefix_width + max_content {
            // 换行
            result.push(Line::from(std::mem::take(current_spans)));
            *current_spans = vec![Span::raw(prefix.to_string())];
            *current_width = prefix_width;
            // 去除换行后的前导空格
            let trimmed = word.trim_start();
            let trimmed_width: usize = trimmed
                .chars()
                .map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(0))
                .sum();
            current_spans.push(Span::styled(trimmed.to_string(), style));
            *current_width += trimmed_width;
        } else {
            current_spans.push(Span::styled(word.clone(), style));
            *current_width += *w_width;
        }
        word.clear();
        *w_width = 0;
    };

    for span in line.spans {
        let style = span.style;
        for ch in span.content.chars() {
            let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            if ch.is_whitespace() && ch != '\n' {
                // 空格是词边界 — 先 flush 前面的 word
                if !word_buf.is_empty() {
                    flush_word(
                        &mut current_spans,
                        &mut current_width,
                        &mut result,
                        &mut word_buf,
                        &mut word_width,
                        word_style,
                        prefix,
                        prefix_width,
                        max_content,
                    );
                }
                word_buf.push(ch);
                word_width += ch_width;
                word_style = style;
                flush_word(
                    &mut current_spans,
                    &mut current_width,
                    &mut result,
                    &mut word_buf,
                    &mut word_width,
                    word_style,
                    prefix,
                    prefix_width,
                    max_content,
                );
            } else {
                // 如果当前 word 加上这个字符会超出整行宽度（超长不可断词），按字符断
                if word_width + ch_width > max_content && !word_buf.is_empty() {
                    flush_word(
                        &mut current_spans,
                        &mut current_width,
                        &mut result,
                        &mut word_buf,
                        &mut word_width,
                        word_style,
                        prefix,
                        prefix_width,
                        max_content,
                    );
                }
                if word_buf.is_empty() {
                    word_style = style;
                }
                word_buf.push(ch);
                word_width += ch_width;
            }
        }
        // span 结束但 word 可能跨 span，先 flush
        if !word_buf.is_empty() {
            flush_word(
                &mut current_spans,
                &mut current_width,
                &mut result,
                &mut word_buf,
                &mut word_width,
                word_style,
                prefix,
                prefix_width,
                max_content,
            );
        }
    }

    result.push(Line::from(current_spans));
    result
}

fn padded_hint_line(text: &str, bg: Color, width: usize) -> Line<'static> {
    padded_colored_line(text, bg, THEME.muted, width)
}

fn padded_colored_line(text: &str, bg: Color, fg: Color, width: usize) -> Line<'static> {
    let dw = display_width(text);
    let rp = width.saturating_sub(dw);
    Line::from(vec![
        Span::styled(text.to_string(), Style::default().bg(bg).fg(fg)),
        Span::styled(" ".repeat(rp), Style::default().bg(bg)),
    ])
}

fn render_result_line_bg(line: &str, bg: Color, width: usize) -> Vec<Line<'static>> {
    let base_style = if bg == Color::Reset {
        Style::default()
    } else {
        Style::default().bg(bg)
    };
    // tab → spaces
    let line = line.replace('\t', "    ");
    // 左 padding 1, 右 padding 1 → 内容可用宽度 = width - 2
    let content_width = width.saturating_sub(2);
    let content = format!(" {line}");
    let stripped = strip_ansi_sgr(&content);
    let cw = display_width(&stripped);

    // 确定行颜色
    let trimmed = line.trim_start();
    let fg = if trimmed.starts_with('+') && !trimmed.starts_with("+++") {
        Some(THEME.assistant_msg)
    } else if trimmed.starts_with('-') && !trimmed.starts_with("---") {
        Some(THEME.error)
    } else if trimmed.starts_with("@@") {
        Some(THEME.accent)
    } else {
        None
    };

    // 无需 wrap（content 含左 1 格 padding，content_width 留了右 1 格）
    if cw <= content_width || width <= 2 {
        let rp = width.saturating_sub(cw);
        let result_line = if let Some(fg) = fg {
            Line::from(vec![
                Span::styled(content, base_style.fg(fg)),
                Span::styled(" ".repeat(rp), base_style),
            ])
        } else {
            let mut parsed = crate::ansi::parse_ansi_line(&content);
            if bg != Color::Reset {
                parsed.spans.push(Span::styled(" ".repeat(rp), base_style));
                parsed = parsed.style(base_style);
            }
            parsed
        };
        return vec![result_line];
    }

    // Wrap: 按 content_width 拆分为多行
    let source = &stripped;
    let mut result = Vec::new();
    let mut chars = source.chars().peekable();
    let mut is_first = true;

    while chars.peek().is_some() {
        let mut line_str = String::new();
        let mut line_width = 0;
        let prefix = if is_first { "" } else { " " }; // 续行缩进
        let prefix_w: usize = if is_first { 0 } else { 1 };
        let effective_width = content_width.saturating_sub(prefix_w);

        while let Some(&ch) = chars.peek() {
            let ch_w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            if line_width + ch_w > effective_width {
                break;
            }
            line_str.push(ch);
            line_width += ch_w;
            chars.next();
        }

        let total_w = prefix_w + line_width;
        let rp = width.saturating_sub(total_w);
        let display_content = format!("{prefix}{line_str}");
        let result_line = if let Some(fg) = fg {
            Line::from(vec![
                Span::styled(display_content, base_style.fg(fg)),
                Span::styled(" ".repeat(rp), base_style),
            ])
        } else {
            Line::from(vec![
                Span::styled(display_content, base_style.fg(THEME.text)),
                Span::styled(" ".repeat(rp), base_style),
            ])
        };
        result.push(result_line);
        is_first = false;
    }

    if result.is_empty() {
        vec![Line::styled(" ".repeat(width), base_style)]
    } else {
        result
    }
}

/// 单行文本按 display width wrap 成多行，每行带背景色 padding（右留 1 格）
fn wrap_padded_line(
    text: &str,
    text_style: Style,
    pad_style: Style,
    width: usize,
) -> Vec<Line<'static>> {
    let text = text.replace('\t', "    ");
    let content_width = width.saturating_sub(1); // 右 padding 1
    let dw = display_width(&text);
    if dw <= content_width || width <= 2 {
        let rp = width.saturating_sub(dw);
        return vec![Line::from(vec![
            Span::styled(text, text_style),
            Span::styled(" ".repeat(rp), pad_style),
        ])];
    }

    let mut result = Vec::new();
    let mut chars = text.chars().peekable();
    let mut is_first = true;

    while chars.peek().is_some() {
        let mut line_str = String::new();
        let mut line_width = 0;
        let prefix = if is_first { "" } else { "  " };
        let prefix_w = display_width(prefix);
        let effective_width = content_width.saturating_sub(prefix_w);

        while let Some(&ch) = chars.peek() {
            let ch_w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            if line_width + ch_w > effective_width {
                break;
            }
            line_str.push(ch);
            line_width += ch_w;
            chars.next();
        }

        let total_w = prefix_w + line_width;
        let rp = width.saturating_sub(total_w);
        result.push(Line::from(vec![
            Span::styled(format!("{prefix}{line_str}"), text_style),
            Span::styled(" ".repeat(rp), pad_style),
        ]));
        is_first = false;
    }

    if result.is_empty() {
        vec![Line::styled(" ".repeat(width), pad_style)]
    } else {
        result
    }
}


fn strip_ansi_sgr(text: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\x1b' && i + 1 < chars.len() && chars[i + 1] == '[' {
            i += 2;
            while i < chars.len() && chars[i] != 'm' {
                i += 1;
            }
            if i < chars.len() {
                i += 1;
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

fn tool_call_preview(name: &str, params: Option<&Value>) -> String {
    let Some(params) = params else {
        return String::new();
    };
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "bash" | "shell" => params
            .get("command")
            .and_then(Value::as_str)
            .map(|cmd| {
                let first_line = cmd.lines().next().unwrap_or(cmd);
                if first_line.len() > 80 {
                    format!("{}…", &first_line[..80])
                } else {
                    first_line.to_string()
                }
            })
            .unwrap_or_default(),
        "edit" | "write" | "read" => params
            .get("file_path")
            .or_else(|| params.get("path"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        "find" | "grep" => params
            .get("pattern")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        _ => String::new(),
    }
}

#[allow(dead_code)]
fn role_color(role: &str) -> Color {
    match role {
        "user" => THEME.user_msg,
        "assistant" => THEME.assistant_msg,
        "tool" => THEME.tool_call,
        "system" => Color::Rgb(149, 117, 205), // #9575cd
        "branch_summary" => Color::Rgb(149, 117, 205),
        "compaction_summary" => THEME.accent,
        _ => THEME.text,
    }
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
