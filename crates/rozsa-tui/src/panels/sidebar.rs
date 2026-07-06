// components/sidebar.rs — 侧边栏渲染
//
// Internal Framework:
// sidebar.rs
// ├── render_sidebar()  侧边栏渲染 (git/model/tokens/agents/files)
// └── truncate()        文本截断工具
//
// Related Docs:
// - [TUI Design](../../../docs/rozsa_framework.md#rozsa-tui--ratatui-终端前端)

// components/sidebar.rs — 侧边栏渲染：项目信息、模型状态、上下文使用率、代理队列、工具统计等
//
// Internal Framework:
// sidebar.rs
// ├── render_sidebar()     pub fn 主渲染入口
// └── truncate()           pub fn 文本截断（考虑 unicode width）
//
// Related Docs:
// - [TUI Design](../../../docs/rozsa_framework.md#rozsa-tui--ratatui-终端前端)

use std::cmp::Reverse;

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use serde_json::Value;

use crate::{app::AppState, backend::SubagentView, theme::THEME};

pub fn render_sidebar(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    state: &AppState,
    agents: Option<&dyn SubagentView>,
) {
    let ui = &state.ui;
    let runtime = ui.runtime_state.as_ref();
    let inner = area.width.saturating_sub(2) as usize;
    let mut lines = Vec::<Line<'static>>::new();

    let project_name = runtime
        .and_then(|state| state.pointer("/project/sessionName"))
        .and_then(Value::as_str)
        .or_else(|| {
            runtime
                .and_then(|state| state.pointer("/project/projectName"))
                .and_then(Value::as_str)
        })
        .or(ui.session_name.as_deref())
        .unwrap_or(ui.app_name.as_str())
        .to_uppercase();
    lines.push(Line::styled(
        truncate(&project_name, inner),
        Style::default()
            .fg(THEME.accent)
            .add_modifier(Modifier::BOLD),
    ));

    append_git(&mut lines, runtime, inner);
    append_model(&mut lines, runtime, ui, inner);
    append_mode(&mut lines, runtime);
    lines.push(Line::raw(""));
    append_context(&mut lines, ui, inner);
    lines.push(Line::raw(""));
    append_tokens(&mut lines, runtime);
    append_queue(&mut lines, ui, inner);
    append_agents(&mut lines, agents, inner);
    append_files(&mut lines, runtime, inner);
    append_tools(&mut lines, runtime, inner);
    append_notices(&mut lines, state, inner);

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false }).block(
        Block::default()
            .borders(Borders::LEFT)
            .border_style(Style::default().fg(THEME.border_muted)),
    );
    frame.render_widget(paragraph, area);
}

fn append_git(lines: &mut Vec<Line<'static>>, runtime: Option<&Value>, inner: usize) {
    if !runtime
        .and_then(|state| state.pointer("/gitStatus/enabled"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return;
    }
    let branch = runtime
        .and_then(|state| state.pointer("/gitStatus/branch"))
        .and_then(Value::as_str)
        .unwrap_or("detached");
    let changes = runtime
        .and_then(|state| state.pointer("/gitStatus/uncommittedChangesCount"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let mut spans = vec![Span::raw(truncate(branch, inner.saturating_sub(10)))];
    if changes > 0 {
        spans.push(Span::raw(" ·"));
        spans.push(Span::styled(
            changes.to_string(),
            Style::default().fg(THEME.assistant_msg),
        ));
        spans.push(Span::raw(" changes"));
    }
    lines.push(Line::from(spans));
}

fn append_model(
    lines: &mut Vec<Line<'static>>,
    runtime: Option<&Value>,
    ui: &crate::protocol::NativeUiState,
    inner: usize,
) {
    let model_name = runtime
        .and_then(|state| state.pointer("/modelUsage/model"))
        .and_then(Value::as_str)
        .or(ui.model.as_ref().map(|model| model.id.as_str()))
        .unwrap_or("no-model");
    let thinking = runtime
        .and_then(|state| state.pointer("/modelUsage/reasoningEffort"))
        .and_then(Value::as_str)
        .unwrap_or(&ui.thinking_level);
    lines.push(Line::raw(truncate(
        &format!("{}|{}", format_model_name(model_name), title_case(thinking)),
        inner,
    )));
}

fn append_mode(lines: &mut Vec<Line<'static>>, runtime: Option<&Value>) {
    let permission = runtime
        .and_then(|state| state.pointer("/permission/mode"))
        .and_then(Value::as_str)
        .unwrap_or("on-request");
    let permission_label = match permission {
        "free-permission" => "free",
        "auto-permission" => "auto",
        _ => "on-request",
    };
    let edit_mode = runtime
        .and_then(|state| state.pointer("/editMode"))
        .and_then(Value::as_str)
        .unwrap_or("normal");
    lines.push(Line::from(vec![
        Span::styled("▶ ", Style::default().fg(THEME.accent)),
        Span::styled(
            permission_label,
            Style::default().fg(if permission_label == "auto" {
                THEME.success
            } else {
                THEME.warning
            }),
        ),
        Span::raw(" · "),
        Span::styled(edit_mode.to_string(), Style::default().fg(THEME.muted)),
    ]));
}

fn append_context(
    lines: &mut Vec<Line<'static>>,
    ui: &crate::protocol::NativeUiState,
    inner: usize,
) {
    let percent = ui
        .context_usage
        .as_ref()
        .and_then(|ctx| ctx.get("percent"))
        .and_then(Value::as_f64);
    let label = percent
        .map(|value| format!("{value:.0}%"))
        .unwrap_or_else(|| "—".to_string());
    lines.push(Line::styled("CONTEXT", Style::default().fg(THEME.muted)));
    lines.push(progress_line(percent.unwrap_or(0.0), &label, inner));
    lines.push(Line::raw(""));
}

fn append_tokens(lines: &mut Vec<Line<'static>>, runtime: Option<&Value>) {
    let prompt = runtime
        .and_then(|state| state.pointer("/modelUsage/promptTokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let completion = runtime
        .and_then(|state| state.pointer("/modelUsage/completionTokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let session = runtime
        .and_then(|state| state.pointer("/modelUsage/sessionTotalTokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    lines.push(Line::from(vec![
        Span::styled("TOKENS", Style::default().fg(THEME.muted)),
        Span::styled(
            format!(" [{}]", fmt_number(session)),
            Style::default().fg(THEME.muted),
        ),
    ]));
    lines.push(Line::raw(format!(
        "In {} · Out {}",
        fmt_number(prompt),
        fmt_number(completion)
    )));
}

fn append_queue(lines: &mut Vec<Line<'static>>, ui: &crate::protocol::NativeUiState, inner: usize) {
    if ui.pending_messages.is_empty() {
        return;
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled("QUEUE", Style::default().fg(THEME.muted)));
    for message in ui.pending_messages.iter().take(5) {
        lines.push(Line::raw(format!(
            "- {}",
            truncate(message, inner.saturating_sub(2))
        )));
    }
}

fn append_agents(lines: &mut Vec<Line<'static>>, agents: Option<&dyn SubagentView>, inner: usize) {
    let Some(view) = agents else {
        return;
    };
    let subagents = view.list_subagents_sync();
    if subagents.is_empty() {
        return;
    }
    let viewing = view.viewing_subagent_id_sync();
    lines.push(Line::raw(""));
    lines.push(Line::styled("AGENTS", Style::default().fg(THEME.muted)));
    lines.push(Line::styled(
        if viewing.is_none() {
            "▶ main"
        } else {
            "● main"
        },
        Style::default().fg(THEME.accent),
    ));
    for agent in subagents.iter().take(5) {
        let icon = if Some(&agent.id) == viewing.as_ref() {
            "▶"
        } else {
            match agent.status {
                rozsa_app::subagent::SubagentStatus::Running => "○",
                _ => "●",
            }
        };
        let status_str = format!("{:?}", agent.status).to_lowercase();
        let name_max = inner.saturating_sub(status_str.len() + 4);
        lines.push(Line::raw(format!(
            "{icon} {} {}",
            truncate(&agent.name, name_max),
            status_str
        )));
    }
    if subagents.len() > 5 {
        lines.push(Line::styled(
            format!("  …{} more", subagents.len() - 5),
            Style::default().fg(THEME.muted),
        ));
    }
}

fn append_files(lines: &mut Vec<Line<'static>>, runtime: Option<&Value>, inner: usize) {
    if let Some(files) = runtime
        .and_then(|state| state.pointer("/gitStatus/uncommittedFiles"))
        .and_then(Value::as_array)
    {
        if !files.is_empty() {
            append_file_section(lines, "PROJECT FILES", files, inner, 10);
        }
    }
    if let Some(files) = runtime
        .and_then(|state| state.pointer("/changedFiles"))
        .and_then(Value::as_array)
    {
        if !files.is_empty() {
            append_file_section(lines, "SESSION FILES", files, inner, 5);
        }
    }
}

fn append_tools(lines: &mut Vec<Line<'static>>, runtime: Option<&Value>, inner: usize) {
    let Some(tools) = runtime
        .and_then(|state| state.pointer("/toolCallStats"))
        .and_then(Value::as_array)
    else {
        return;
    };
    if tools.is_empty() {
        return;
    }
    let mut sorted = tools.iter().collect::<Vec<_>>();
    sorted.sort_by_key(|tool| Reverse(tool.get("callCount").and_then(Value::as_u64).unwrap_or(0)));
    lines.push(Line::raw(""));
    lines.push(Line::styled("TOOLS", Style::default().fg(THEME.muted)));
    let mut current = String::new();
    for tool in sorted.iter().take(4) {
        let name = tool
            .get("toolName")
            .and_then(Value::as_str)
            .unwrap_or("tool");
        let count = tool.get("callCount").and_then(Value::as_u64).unwrap_or(0);
        let part = format!("{name} ×{count}");
        if current.is_empty() {
            current = part;
        } else if current.len() + part.len() + 2 <= inner {
            current.push_str("  ");
            current.push_str(&part);
        } else {
            lines.push(Line::raw(truncate(&current, inner)));
            current = part;
        }
    }
    if !current.is_empty() {
        lines.push(Line::raw(truncate(&current, inner)));
    }
}

fn append_notices(lines: &mut Vec<Line<'static>>, state: &AppState, inner: usize) {
    if state.notifications.is_empty() {
        return;
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled("NOTICES", Style::default().fg(THEME.muted)));
    for notice in state.notifications.iter().rev().take(3) {
        let color = match notice.level.as_str() {
            "error" => THEME.error,
            "warning" => THEME.warning,
            _ => THEME.muted,
        };
        lines.push(Line::styled(
            truncate(&notice.message, inner),
            Style::default().fg(color),
        ));
    }
}

fn append_file_section(
    lines: &mut Vec<Line<'static>>,
    title: &str,
    files: &[Value],
    inner: usize,
    limit: usize,
) {
    let total_add = files
        .iter()
        .filter_map(|file| file.get("additions").and_then(Value::as_u64))
        .sum::<u64>();
    let total_del = files
        .iter()
        .filter_map(|file| file.get("deletions").and_then(Value::as_u64))
        .sum::<u64>();
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled(title.to_string(), Style::default().fg(THEME.muted)),
        if total_add > 0 {
            Span::styled(
                format!(" +{total_add}"),
                Style::default().fg(THEME.assistant_msg),
            )
        } else {
            Span::raw("")
        },
        if total_del > 0 {
            Span::styled(format!(" -{total_del}"), Style::default().fg(THEME.error))
        } else {
            Span::raw("")
        },
    ]));
    for file in files.iter().take(limit) {
        let path = file.get("path").and_then(Value::as_str).unwrap_or("file");
        let name = path.rsplit('/').next().unwrap_or(path);
        let icon = match file
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("modified")
        {
            "added" => "+",
            "deleted" => "-",
            _ => "~",
        };
        let add = file.get("additions").and_then(Value::as_u64).unwrap_or(0);
        let del = file.get("deletions").and_then(Value::as_u64).unwrap_or(0);
        let stats_str = match (add, del) {
            (0, 0) => String::new(),
            (a, 0) => format!(" +{a}"),
            (0, d) => format!(" -{d}"),
            (a, d) => format!(" +{a}-{d}"),
        };
        let name_max = inner.saturating_sub(2 + stats_str.len());
        let mut spans = vec![
            Span::styled(format!("{icon} "), Style::default().fg(THEME.muted)),
            Span::raw(truncate(name, name_max)),
        ];
        if add > 0 {
            spans.push(Span::styled(
                format!(" +{add}"),
                Style::default().fg(THEME.assistant_msg),
            ));
        }
        if del > 0 {
            spans.push(Span::styled(
                format!("-{del}"),
                Style::default().fg(THEME.error),
            ));
        }
        lines.push(Line::from(spans));
    }
    if files.len() > limit {
        lines.push(Line::styled(
            format!("  …{} more", files.len() - limit),
            Style::default().fg(THEME.muted),
        ));
    }
}

fn progress_line(percent: f64, label: &str, inner: usize) -> Line<'static> {
    let safe_percent = percent.clamp(0.0, 100.0);
    let label_width = label.chars().count() + 1;
    let bar_width = inner.saturating_sub(label_width).max(1);
    let filled = ((safe_percent / 100.0) * bar_width as f64).round() as usize;
    let empty = bar_width.saturating_sub(filled);
    let color = if safe_percent > 90.0 {
        THEME.error
    } else if safe_percent > 70.0 {
        THEME.warning
    } else {
        THEME.assistant_msg
    };
    Line::from(vec![
        Span::styled("▓".repeat(filled), Style::default().fg(color)),
        Span::styled("░".repeat(empty), Style::default().fg(THEME.muted)),
        Span::raw(" "),
        Span::styled(label.to_string(), Style::default().fg(color)),
    ])
}

fn fmt_number(value: u64) -> String {
    if value < 1_000 {
        value.to_string()
    } else if value < 1_000_000 {
        let value = value as f64 / 1_000.0;
        if value < 10.0 {
            format!("{value:.1}k")
        } else {
            format!("{value:.0}k")
        }
    } else {
        format!("{:.1}M", value as f64 / 1_000_000.0)
    }
}

fn title_case(text: &str) -> String {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    format!("{}{}", first.to_uppercase(), chars.as_str())
}

fn format_model_name(model: &str) -> String {
    let mut name = model.rsplit('.').next().unwrap_or(model).to_string();
    if let Some(stripped) = name.strip_suffix("-v1") {
        name = stripped.to_string();
    }
    name
}

pub fn truncate(text: &str, max_width: usize) -> String {
    use unicode_width::UnicodeWidthChar;
    let mut width = 0;
    let mut out = String::new();
    for ch in text.chars() {
        let w = ch.width().unwrap_or(0);
        if width + w > max_width {
            out.push('…');
            return out;
        }
        out.push(ch);
        width += w;
    }
    out
}
