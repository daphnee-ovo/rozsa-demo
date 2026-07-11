// render/messages.rs — 消息区域渲染 + message_lines + 所有辅助函数
//
// Internal Framework:
// messages.rs
// ├── render_messages()             消息区域渲染
// ├── reorder_messages_for_display() 消息排序
// ├── message_lines()               消息行生成 (AgentMessage)
// ├── bash_execution_lines()        bash 执行块格式化 (payload Value)
// ├── summary_block_lines()         摘要块行生成 (payload Value)
// ├── compaction_summary_lines()    压缩摘要行
// ├── compaction_collapsed_lines()  压缩折叠行
// ├── branch_summary_lines()        分支摘要行
// ├── tool_result_message_lines()   工具结果行 (ToolResultMessage)
// ├── render_tool_output_lines()    工具输出行渲染
// ├── custom_message_lines()        自定义消息行 (payload Value)
// ├── spinner_char()                状态行 spinner 字符 (pub(super) — 供 status.rs 共用)
// ├── append_content_block_lines()  内容行追加 (ContentBlock)
// ├── append_assistant_text_lines() 助手文本行追加
// ├── wrap_line_with_prefix()       前缀行折行
// ├── padded_hint_line()            带缩进提示行
// ├── padded_colored_line()         带前后景色行
// ├── render_result_line_bg()       结果行背景渲染
// ├── wrap_padded_line()            按背景色 padding 折行
// ├── strip_ansi_sgr()              去除 ANSI 控制序列
// ├── tool_call_preview()           工具调用预览
// ├── role_color()                  角色配色
// ├── display_width()               显示宽度计算
// ├── wrap_text_lines()             文本行换行
// └── WordSplitter                  单词分割迭代器

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use serde_json::Value;

use rozsa_core::messages::{AgentMessage, CustomAgentMessage};
use rozsa_model::types::{
    AssistantMessage, ContentBlock, Message, ToolResultMessage, UserContent, UserMessage,
};

use crate::{app::AppState, theme::THEME};

use super::{cached_message_height, cached_message_lines};

/// 将后端的 messages 数组重排为 UI 展示顺序。
/// 后端为了 LLM 上下文将 compactionSummary 放在 index 0，
/// UI 展示时按 timestamp 将其插入到 preserved messages 之后、新消息之前。
fn reorder_messages_for_display(messages: &[AgentMessage]) -> Vec<&AgentMessage> {
    if messages.is_empty() {
        return vec![];
    }
    let first = &messages[0];
    let compaction_ts = match first {
        AgentMessage::Custom { message } if message.message_type == "compactionSummary" => {
            Some(message.timestamp)
        }
        _ => None,
    };

    let Some(compaction_ts) = compaction_ts else {
        return messages.iter().collect();
    };

    let rest = &messages[1..];
    // 找到第一个 timestamp > compaction_ts 的消息（即 compact 之后的新消息）
    let insert_pos = rest
        .iter()
        .position(|m| message_timestamp(m) > compaction_ts)
        .unwrap_or(rest.len());

    let mut result = Vec::with_capacity(messages.len());
    for msg in &rest[..insert_pos] {
        result.push(msg);
    }
    result.push(first);
    for msg in &rest[insert_pos..] {
        result.push(msg);
    }
    result
}

fn message_timestamp(message: &AgentMessage) -> i64 {
    match message {
        AgentMessage::Standard { message } => match message {
            Message::User(u) => u.timestamp,
            Message::Assistant(a) => a.timestamp,
            Message::ToolResult(t) => t.timestamp,
        },
        AgentMessage::Custom { message } => message.timestamp,
    }
}

pub(super) fn render_messages(frame: &mut ratatui::Frame<'_>, area: Rect, state: &AppState) {
    let msg_width = area.width as usize;
    let visible_height = area.height as usize;
    if visible_height == 0 {
        return;
    }

    // 按时间顺序重排消息：后端把 compactionSummary 放在 index 0（LLM 需要先读摘要），
    // 但 UI 展示时应按时间戳插入正确位置（preserved msgs → compaction → new msgs）
    let messages = reorder_messages_for_display(&state.ui.messages);
    let msg_count = messages.len();

    // 收集"尾部附加内容"（消息之后）：长通知 + error。
    // 它们行数固定且短，直接构造，参与滚动总高计算。
    let tail_lines = build_tail_lines(state);

    // ── 1. 算每条消息高度（虚拟滚动）──
    // streaming 时最后一条消息不缓存
    let mut heights: Vec<usize> = Vec::with_capacity(msg_count);
    for (i, message) in messages.iter().enumerate() {
        let is_last_streaming = state.ui.is_streaming && i == msg_count - 1;
        heights.push(cached_message_height(
            message,
            state.tools_expanded,
            state.thinking_visible,
            state.show_images,
            state.compaction_collapsed,
            is_last_streaming,
            msg_width,
        ));
    }
    let msg_lines_total: usize = heights.iter().sum();
    let total_lines = msg_lines_total + tail_lines.len();

    // ── 2. 计算滚动窗口（scroll 从底部向上的行数偏移）──
    let max_scroll = total_lines.saturating_sub(visible_height);
    let scroll = state.scroll.min(max_scroll);
    let view_start_line = total_lines.saturating_sub(visible_height + scroll);
    let view_end_line = total_lines.saturating_sub(scroll);
    let has_above = view_start_line > 0;
    let has_below = scroll > 0;
    let indicator_lines = (has_above as usize) + (has_below as usize);
    let content_capacity = visible_height.saturating_sub(indicator_lines);

    // ── 3. 累加 heights 定位首个可见消息 ──
    // collected_lines 是 view_start_line 之后已收集到 visible_lines 的累计行数（不含 indicator）
    let mut visible_lines: Vec<Line<'static>> = Vec::with_capacity(visible_height);
    if has_above {
        visible_lines.push(Line::styled(
            format!("↑ {} lines above", view_start_line),
            Style::default().fg(THEME.muted),
        ));
    }

    // 找到包含 view_start_line 的消息（如可见区域起点落在消息中部，需 skip 该消息开头若干行）
    let mut accumulated = 0usize;
    let mut first_msg_idx = msg_count; // 未找到 → 视口完全在 tail
    let mut first_msg_line_offset = 0usize;
    for (i, &h) in heights.iter().enumerate() {
        if accumulated + h > view_start_line {
            first_msg_idx = i;
            first_msg_line_offset = view_start_line - accumulated;
            break;
        }
        accumulated += h;
    }

    // ── 4. 渲染可见消息 ──
    let mut produced = 0usize;
    let mut line_cursor = if first_msg_idx < msg_count {
        accumulated
    } else {
        msg_lines_total
    };
    for i in first_msg_idx..msg_count {
        if produced >= content_capacity {
            break;
        }
        let is_last_streaming = state.ui.is_streaming && i == msg_count - 1;
        let lines = cached_message_lines(
            messages[i],
            state.tools_expanded,
            state.thinking_visible,
            state.show_images,
            state.compaction_collapsed,
            is_last_streaming,
            msg_width,
        );
        let skip = if i == first_msg_idx {
            first_msg_line_offset
        } else {
            0
        };
        for line in lines.into_iter().skip(skip) {
            if produced >= content_capacity {
                break;
            }
            // 仅纳入落在 [view_start_line, view_end_line) 内的行
            if line_cursor < view_end_line {
                visible_lines.push(line);
                produced += 1;
            }
            line_cursor += 1;
        }
        // 消息整体被全收完，line_cursor 应跨过整段；上面循环自然推进
    }

    // ── 5. 渲染可见 tail（notifications + error）──
    // tail 在所有消息之后；它的第一行对应全局 line = msg_lines_total
    if produced < content_capacity && line_cursor >= msg_lines_total {
        let tail_start = line_cursor.saturating_sub(msg_lines_total);
        let tail_end = view_end_line
            .saturating_sub(msg_lines_total)
            .min(tail_lines.len());
        for line in tail_lines.iter().take(tail_end).skip(tail_start) {
            if produced >= content_capacity {
                break;
            }
            visible_lines.push(line.clone());
            produced += 1;
        }
    }

    if has_below {
        visible_lines.push(Line::styled(
            format!("↓ {scroll} lines below"),
            Style::default().fg(THEME.muted),
        ));
    }
    frame.render_widget(Paragraph::new(visible_lines), area);
}

/// 构造消息列表之后追加的 lines（长通知 + error）。
/// 这些内容行数较少且每帧固定，直接构造不进缓存。
fn build_tail_lines(state: &AppState) -> Vec<Line<'static>> {
    let mut tail = Vec::<Line<'static>>::new();
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
        tail.push(Line::raw(""));
        for line in notification.message.lines() {
            tail.push(Line::styled(line.to_string(), Style::default().fg(color)));
        }
    }

    if let Some(error) = &state.ui.error {
        tail.push(Line::styled(
            error.clone(),
            Style::default().fg(THEME.error),
        ));
    }
    tail
}

/// 计算字符串显示宽度（CJK 字符占 2 列）
fn display_width(s: &str) -> usize {
    use unicode_width::UnicodeWidthChar;
    s.chars()
        .map(|c| UnicodeWidthChar::width(c).unwrap_or(0))
        .sum()
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
    message: &AgentMessage,
    tools_expanded: bool,
    thinking_visible: bool,
    show_images: bool,
    compaction_collapsed: bool,
    is_last_streaming: bool,
    width: usize,
) -> Vec<Line<'static>> {
    match message {
        AgentMessage::Standard { message } => match message {
            Message::User(u) => user_message_lines(u, width),
            Message::Assistant(a) => {
                assistant_message_lines(a, thinking_visible, show_images, is_last_streaming, width)
            }
            Message::ToolResult(t) => tool_result_message_lines(t, tools_expanded, width),
        },
        AgentMessage::Custom { message } => {
            custom_dispatched_lines(message, compaction_collapsed, width)
        }
    }
}

fn custom_dispatched_lines(
    message: &CustomAgentMessage,
    compaction_collapsed: bool,
    width: usize,
) -> Vec<Line<'static>> {
    match message.message_type.as_str() {
        "bashExecution" => {
            let mut lines = vec![Line::raw("")];
            lines.extend(bash_execution_lines(&message.payload, width));
            lines
        }
        "compactionSummary" => {
            let mut lines = vec![Line::raw("")];
            if compaction_collapsed {
                lines.extend(compaction_collapsed_lines(&message.payload));
            } else {
                lines.extend(compaction_summary_lines(&message.payload));
            }
            lines
        }
        "branchSummary" => {
            let mut lines = vec![Line::raw("")];
            lines.extend(branch_summary_lines(&message.payload));
            lines
        }
        "custom" => custom_message_lines(&message.payload),
        _ => Vec::new(),
    }
}

fn user_message_lines(u: &UserMessage, width: usize) -> Vec<Line<'static>> {
    let mut lines = vec![Line::raw("")];
    // 用户消息：Codex 风格 — "› " bold+dim 前缀 + 背景色 + word-wrap
    let user_bg = Style::default()
        .bg(THEME.user_message_bg)
        .fg(THEME.user_msg);
    let prefix_width: usize = 2; // "› " 或 "  "
    let wrap_width = width.saturating_sub(prefix_width).max(10);

    // 收集文本行（统一处理 \r\n、\r、\n 换行）
    // 优先使用 displayText（如 skill 展开后保留原始输入供显示）
    let mut text_lines: Vec<String> = Vec::new();
    if let Some(display) = &u.display_text {
        let normalized = crate::normalize_newlines(display);
        for line in normalized.lines() {
            text_lines.push(line.to_string());
        }
    } else {
        match &u.content {
            UserContent::Text(text) => {
                let normalized = crate::normalize_newlines(text);
                for line in normalized.lines() {
                    text_lines.push(line.to_string());
                }
            }
            UserContent::Blocks(blocks) => {
                for block in blocks {
                    if let ContentBlock::Text { text, .. } = block {
                        let normalized = crate::normalize_newlines(text);
                        for line in normalized.lines() {
                            text_lines.push(line.to_string());
                        }
                        if normalized.ends_with('\n') {
                            text_lines.push(String::new());
                        }
                    }
                }
            }
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
    lines
}

fn assistant_message_lines(
    a: &AssistantMessage,
    thinking_visible: bool,
    show_images: bool,
    is_last_streaming: bool,
    width: usize,
) -> Vec<Line<'static>> {
    let mut lines = vec![Line::raw("")];
    let content_len = a.content.len();
    for (idx, block) in a.content.iter().enumerate() {
        let is_last_item = is_last_streaming && idx == content_len - 1;
        append_content_block_lines(
            &mut lines,
            block,
            thinking_visible,
            show_images,
            is_last_item,
            width,
        );
    }
    lines
}

fn bash_execution_lines(payload: &Value, width: usize) -> Vec<Line<'static>> {
    let command = payload.get("command").and_then(Value::as_str).unwrap_or("");
    let output = payload.get("output").and_then(Value::as_str).unwrap_or("");
    let exit_code = payload.get("exitCode").and_then(Value::as_i64);
    let cancelled = payload
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
    lines.extend(wrap_padded_line(
        &cmd_display,
        cmd_style,
        block_style,
        width,
    ));

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

fn summary_block_lines(payload: &Value, label: &str) -> Vec<Line<'static>> {
    let summary = payload.get("summary").and_then(Value::as_str).unwrap_or("");
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

fn compaction_summary_lines(payload: &Value) -> Vec<Line<'static>> {
    summary_block_lines(payload, "compaction")
}

fn compaction_collapsed_lines(payload: &Value) -> Vec<Line<'static>> {
    let summary = payload.get("summary").and_then(Value::as_str).unwrap_or("");
    let line_count = summary.lines().count();
    vec![Line::from(vec![Span::styled(
        format!("  [compaction] ({line_count} lines — Ctrl+O to expand)"),
        Style::default().fg(THEME.muted),
    )])]
}

fn branch_summary_lines(payload: &Value) -> Vec<Line<'static>> {
    summary_block_lines(payload, "branch")
}

fn tool_result_message_lines(
    t: &ToolResultMessage,
    tools_expanded: bool,
    width: usize,
) -> Vec<Line<'static>> {
    let is_error = t.is_error;

    // 与 toolCall 完全一样的背景色
    let bg_color = THEME.tool_pending_bg;
    let block_style = Style::default().bg(bg_color);
    let pad_line = " ".repeat(width);

    let mut lines: Vec<Line<'static>> = Vec::new();
    // 不显示顶部 padding — 由前面的 toolCall 提供
    // 错误时显示标记
    if is_error {
        lines.push(padded_colored_line(
            " (error)",
            bg_color,
            THEME.error,
            width,
        ));
    }

    for block in &t.content {
        if let ContentBlock::Text { text, .. } = block {
            let rendered = render_tool_output_lines(text, bg_color, width, tools_expanded);
            lines.extend(rendered);
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

fn custom_message_lines(payload: &Value) -> Vec<Line<'static>> {
    let custom_type = payload
        .get("customType")
        .and_then(Value::as_str)
        .unwrap_or("custom");
    let display = payload
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
    if let Some(text) = payload.get("content").and_then(Value::as_str) {
        for line in text.lines().take(20) {
            lines.push(
                Line::from(vec![
                    Span::raw(" "),
                    Span::styled(line.to_string(), Style::default().fg(THEME.text)),
                ])
                .style(block_style),
            );
        }
    } else if let Some(content) = payload.get("content").and_then(Value::as_array) {
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
pub(super) fn spinner_char() -> char {
    use std::time::{SystemTime, UNIX_EPOCH};
    let tick = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_millis()
        / 100;
    SPINNER_FRAMES[tick as usize % SPINNER_FRAMES.len()]
}

fn append_content_block_lines(
    lines: &mut Vec<Line<'static>>,
    block: &ContentBlock,
    thinking_visible: bool,
    show_images: bool,
    is_last_streaming: bool,
    width: usize,
) {
    match block {
        ContentBlock::Text { text, .. } => {
            append_assistant_text_lines(lines, text, width);
        }
        ContentBlock::Thinking {
            thinking, redacted, ..
        } => {
            if !thinking_visible {
                lines.push(Line::styled(
                    "  (thinking hidden — Ctrl-T to show)",
                    Style::default().fg(THEME.muted),
                ));
            } else if *redacted {
                lines.push(Line::styled(
                    "  (thinking redacted)",
                    Style::default().fg(THEME.muted),
                ));
            } else {
                for line in wrap_text_lines(
                    &thinking.lines().map(str::to_string).collect::<Vec<_>>(),
                    width.saturating_sub(1).max(10),
                ) {
                    lines.push(Line::from(vec![
                        Span::raw(" "),
                        Span::styled(line, Style::default().fg(THEME.muted)),
                    ]));
                }
            }
        }
        ContentBlock::ToolCall(call) => {
            let name = call.name.as_str();
            let preview = tool_call_preview(name, Some(&call.arguments));
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
                let command = call
                    .arguments
                    .get("command")
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
            // 关闭块（独立 toolCall 块；后续 ToolResult 会单独渲染）
            lines.push(Line::styled(pad_line, block_style));
        }
        ContentBlock::Image { data, .. } => {
            if !show_images {
                lines.push(Line::styled(
                    "  (image hidden)",
                    Style::default().fg(THEME.muted),
                ));
            } else {
                let rendered =
                    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, data)
                        .ok()
                        .and_then(|bytes| {
                            crate::util::terminal_image::render_image(&bytes, 60, 20)
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
    }
}

fn append_assistant_text_lines(lines: &mut Vec<Line<'static>>, text: &str, width: usize) {
    let parsed = crate::util::markdown::parse_markdown_with_width(text, width);
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
            let mut parsed = crate::util::ansi::parse_ansi_line(&content);
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
