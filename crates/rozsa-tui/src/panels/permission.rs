// components/permission.rs — 权限许可面板：approve/reject/trust 交互
//
// Internal Framework:
// permission.rs
// ├── PermissionState           pub struct 权限许可状态
// ├── handle_permission_key()   pub fn 按键处理
// └── render_permission()       pub fn 渲染面板
//
// Related Docs:
// - [TUI Design](../../../docs/rozsa_framework.md#rozsa-tui--ratatui-终端前端)

use std::{
    collections::BTreeMap,
    error::Error,
};

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::{
    input::keymap::matches_action,
    protocol::{send, ClientMessage, NativePermissionPrompt},
    panels::sidebar::truncate,
    theme::THEME,
};

#[derive(Clone, Debug)]
pub struct PermissionState {
    pub prompt: NativePermissionPrompt,
    pub selected: usize,
    pub trust_mode: bool,
    pub created_at: std::time::Instant,
}

impl PermissionState {
    pub fn new(prompt: NativePermissionPrompt) -> Self {
        Self {
            prompt,
            selected: 0,
            trust_mode: false,
            created_at: std::time::Instant::now(),
        }
    }

    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() >= std::time::Duration::from_secs(300)
    }

    pub fn remaining_secs(&self) -> u64 {
        300u64.saturating_sub(self.created_at.elapsed().as_secs())
    }
}

pub fn handle_permission_key(
    key: KeyEvent,
    permission: PermissionState,
    writer: &crate::input::Writer,
    keybindings: &BTreeMap<String, Vec<String>>,
) -> Result<Option<PermissionState>, Box<dyn Error>> {
    if permission.trust_mode {
        return handle_trust_key(key, permission, writer, keybindings);
    }
    handle_main_key(key, permission, writer, keybindings)
}

fn handle_main_key(
    key: KeyEvent,
    mut permission: PermissionState,
    writer: &crate::input::Writer,
    keybindings: &BTreeMap<String, Vec<String>>,
) -> Result<Option<PermissionState>, Box<dyn Error>> {
    if shortcut(key, 'y') {
        send_permission(writer, &permission.prompt.id, "approve_once", None)?;
        Ok(None)
    } else if shortcut(key, 'n') || matches_action(keybindings, key, "tui.select.cancel") {
        send_permission(writer, &permission.prompt.id, "reject", None)?;
        Ok(None)
    } else if shortcut(key, 'a') {
        send_permission(writer, &permission.prompt.id, "reject_alternative", None)?;
        Ok(None)
    } else if shortcut(key, 't') {
        enter_trust(permission, writer)
    } else if matches_action(keybindings, key, "tui.select.up") {
        let max = 3; // 4 options: approve, trust, reject, reject_alternative
        permission.selected = if permission.selected == 0 { max } else { permission.selected - 1 };
        Ok(Some(permission))
    } else if matches_action(keybindings, key, "tui.select.down") {
        let max = 3;
        permission.selected = if permission.selected >= max { 0 } else { permission.selected + 1 };
        Ok(Some(permission))
    } else if matches_action(keybindings, key, "tui.select.confirm") {
        match permission.selected {
            0 => {
                send_permission(writer, &permission.prompt.id, "approve_once", None)?;
                Ok(None)
            }
            1 => enter_trust(permission, writer),
            2 => {
                send_permission(writer, &permission.prompt.id, "reject", None)?;
                Ok(None)
            }
            _ => {
                send_permission(writer, &permission.prompt.id, "reject_alternative", None)?;
                Ok(None)
            }
        }
    } else {
        Ok(Some(permission))
    }
}

fn shortcut(key: KeyEvent, target: char) -> bool {
    matches!(key.code, KeyCode::Char(ch) if ch.to_ascii_lowercase() == target)
}

fn enter_trust(
    mut permission: PermissionState,
    writer: &crate::input::Writer,
) -> Result<Option<PermissionState>, Box<dyn Error>> {
    if permission.prompt.trust_levels.len() <= 1 {
        let key = permission
            .prompt
            .trust_levels
            .first()
            .map(|level| level.key.as_str());
        send_permission(writer, &permission.prompt.id, "approve_session", key)?;
        return Ok(None);
    }
    permission.trust_mode = true;
    permission.selected = 0;
    Ok(Some(permission))
}

fn handle_trust_key(
    key: KeyEvent,
    mut permission: PermissionState,
    writer: &crate::input::Writer,
    keybindings: &BTreeMap<String, Vec<String>>,
) -> Result<Option<PermissionState>, Box<dyn Error>> {
    if matches_action(keybindings, key, "tui.select.cancel") {
        permission.trust_mode = false;
        permission.selected = 1;
        Ok(Some(permission))
    } else if matches_action(keybindings, key, "tui.select.up") {
        permission.selected = permission.selected.saturating_sub(1);
        Ok(Some(permission))
    } else if matches_action(keybindings, key, "tui.select.down") {
        permission.selected =
            (permission.selected + 1).min(permission.prompt.trust_levels.len().saturating_sub(1));
        Ok(Some(permission))
    } else if matches_action(keybindings, key, "tui.select.confirm") {
        let trust_key = permission
            .prompt
            .trust_levels
            .get(permission.selected)
            .map(|level| level.key.as_str());
        send_permission(writer, &permission.prompt.id, "approve_session", trust_key)?;
        Ok(None)
    } else {
        match key.code {
            KeyCode::Char(ch) if ch.is_ascii_digit() => {
                let index = ch.to_digit(10).unwrap_or(0).saturating_sub(1) as usize;
                if let Some(level) = permission.prompt.trust_levels.get(index) {
                    send_permission(
                        writer,
                        &permission.prompt.id,
                        "approve_session",
                        Some(&level.key),
                    )?;
                    Ok(None)
                } else {
                    Ok(Some(permission))
                }
            }
            _ => Ok(Some(permission)),
        }
    }
}

pub fn render_permission(frame: &mut ratatui::Frame<'_>, area: Rect, permission: &PermissionState) {
    frame.render_widget(Clear, area);
    let prompt = &permission.prompt;
    let width = area.width.saturating_sub(4) as usize;
    let tool = prompt
        .request
        .get("toolName")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("tool");
    let command = prompt
        .request
        .get("command")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let risk = prompt
        .context
        .get("riskLevel")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    let mut lines = vec![
        Line::styled(
            "Permission required",
            Style::default()
                .fg(THEME.warning)
                .add_modifier(Modifier::BOLD),
        ),
        Line::raw(format!("tool: {tool}  risk: {risk}")),
    ];
    if !command.is_empty() {
        lines.push(Line::styled(
            truncate(command.lines().next().unwrap_or(command), width),
            Style::default().fg(THEME.muted),
        ));
    }
    lines.push(Line::raw(""));
    if permission.trust_mode {
        lines.push(Line::styled(
            "Trust scope",
            Style::default().fg(THEME.accent),
        ));
        for (index, level) in prompt.trust_levels.iter().take(9).enumerate() {
            let marker = if index == permission.selected {
                ">"
            } else {
                " "
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{marker} [{}] ", index + 1),
                    Style::default().fg(THEME.accent),
                ),
                Span::raw(truncate(&level.label, width.saturating_sub(6))),
            ]));
        }
        lines.push(Line::styled(
            "Esc back",
            Style::default().fg(THEME.muted),
        ));
    } else {
        let options = [
            ("y", "approve"),
            ("t", "trust for session"),
            ("n", "reject"),
            ("a", "reject and ask for alternative"),
        ];
        for (index, (key, label)) in options.iter().enumerate() {
            let marker = if index == permission.selected {
                ">"
            } else {
                " "
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{marker} [{key}] "),
                    Style::default().fg(THEME.accent),
                ),
                Span::raw(*label),
            ]));
        }
    }
    // 倒计时警告（剩余 < 60 秒时显示）
    let remaining = permission.remaining_secs();
    if remaining < 60 {
        lines.push(Line::styled(
            format!("⏱ auto-reject in {remaining}s"),
            Style::default().fg(THEME.error),
        ));
    } else if remaining < 240 {
        lines.push(Line::styled(
            format!("⏱ {remaining}s remaining"),
            Style::default().fg(THEME.warning),
        ));
    }

    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(Block::default().borders(Borders::ALL).title("Permission"));
    frame.render_widget(paragraph, area);
}

fn send_permission(
    writer: &crate::input::Writer,
    id: &str,
    choice: &str,
    trust_key: Option<&str>,
) -> Result<(), Box<dyn Error>> {
    send(
        writer,
        &ClientMessage::PermissionResponse {
            id,
            choice,
            trust_key,
        },
    )
}
