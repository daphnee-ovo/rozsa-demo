// render/status.rs — 状态行 / 通知 / pending / widgets 渲染

use std::collections::BTreeMap;

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use crate::{app::AppState, protocol::NativeUiState, theme::THEME};

use super::messages::spinner_char;
use crate::panels::sidebar::truncate;

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
