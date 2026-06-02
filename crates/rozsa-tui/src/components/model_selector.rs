// components/model_selector.rs — 模型选择器
//
// Internal Framework:
// model_selector.rs
// ├── ModelEntry              模型条目
// ├── ModelSelectorState      选择器状态
// ├── handle_model_selector_key()  键盘处理
// └── render_model_selector()      渲染
//
// Related Docs:
// - [TUI Design](../../../docs/rozsa_framework.md#rozsa-tui--ratatui-终端前端)

// components/model_selector.rs — 模型选择器面板：模糊搜索、切换模型
//
// Internal Framework:
// model_selector.rs
// ├── ModelEntry                pub struct 模型条目
// ├── ModelSelectorState        pub struct 选择器状态
// ├── handle_model_selector_key() pub fn 按键处理
// └── render_model_selector()   pub fn 渲染面板
//
// Related Docs:
// - [TUI Design](../../../docs/rozsa_framework.md#rozsa-tui--ratatui-终端前端)

use std::{
    error::Error,
    os::unix::net::UnixStream,
    sync::{Arc, Mutex},
};

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use serde::Deserialize;

use crate::{
    protocol::{send, ClientMessage},
    components::sidebar::truncate,
    theme::THEME,
};

/// 模型条目
#[derive(Clone, Debug, Deserialize)]
pub struct ModelEntry {
    pub id: String,
    pub provider: String,
    #[serde(default)]
    pub is_current: bool,
}

/// 模型选择器状态
#[derive(Clone, Debug)]
pub struct ModelSelectorState {
    pub models: Vec<ModelEntry>,
    pub filter: String,
    pub selected: usize,
    pub filtered_indices: Vec<usize>,
}

impl ModelSelectorState {
    pub fn new(models: Vec<ModelEntry>) -> Self {
        let filtered_indices = (0..models.len()).collect::<Vec<_>>();
        Self {
            models,
            filter: String::new(),
            selected: 0,
            filtered_indices,
        }
    }

    /// 模糊匹配：filter 中的所有字符按顺序出现在 "{provider}/{id}" 中（忽略大小写）
    fn apply_filter(&mut self) {
        let filter_lower = self.filter.to_lowercase();
        self.filtered_indices = self
            .models
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                let haystack = format!("{}/{}", entry.provider, entry.id).to_lowercase();
                if fuzzy_match(&filter_lower, &haystack) {
                    Some(index)
                } else {
                    None
                }
            })
            .collect();
        self.selected = self.selected.min(self.filtered_indices.len().saturating_sub(1));
    }
}

/// 模糊匹配：filter 的每个字符按顺序出现在 haystack 中
fn fuzzy_match(filter: &str, haystack: &str) -> bool {
    let mut hay_iter = haystack.chars();
    for ch in filter.chars() {
        loop {
            match hay_iter.next() {
                Some(h) if h == ch => break,
                Some(_) => continue,
                None => return false,
            }
        }
    }
    true
}

/// 处理模型选择器的键盘事件
pub fn handle_model_selector_key(
    key: KeyEvent,
    mut state: ModelSelectorState,
    writer: &Arc<Mutex<UnixStream>>,
) -> Result<Option<ModelSelectorState>, Box<dyn Error>> {
    match key.code {
        KeyCode::Esc => Ok(None),
        KeyCode::Up => {
            state.selected = state.selected.saturating_sub(1);
            Ok(Some(state))
        }
        KeyCode::Down => {
            state.selected = (state.selected + 1).min(state.filtered_indices.len().saturating_sub(1));
            Ok(Some(state))
        }
        KeyCode::Enter => {
            if let Some(&index) = state.filtered_indices.get(state.selected) {
                let id = state.models[index].id.clone();
                send(writer, &ClientMessage::SwitchModel { id: &id })?;
            }
            Ok(None)
        }
        KeyCode::Backspace => {
            state.filter.pop();
            state.apply_filter();
            Ok(Some(state))
        }
        KeyCode::Char(ch) => {
            state.filter.push(ch);
            state.apply_filter();
            Ok(Some(state))
        }
        _ => Ok(Some(state)),
    }
}

/// 渲染模型选择器面板（全屏覆盖）
pub fn render_model_selector(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    state: &ModelSelectorState,
) {
    frame.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .margin(1)
        .split(area);

    let border = Block::default().borders(Borders::ALL).title("Models");
    frame.render_widget(border, area);

    // 搜索框
    let search_text = if state.filter.is_empty() {
        "Search: ".to_string()
    } else {
        format!("Search: {}▌", state.filter)
    };
    let search = Paragraph::new(search_text).block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(THEME.border_muted)),
    );
    frame.render_widget(search, chunks[0]);

    // 模型列表
    let list_height = chunks[1].height as usize;
    let start = state.selected.saturating_sub(list_height / 2);
    let end = (start + list_height).min(state.filtered_indices.len());
    let mut lines = Vec::new();
    for row in start..end {
        let Some(&index) = state.filtered_indices.get(row) else {
            continue;
        };
        let entry = &state.models[index];
        let selected = row == state.selected;
        let style = if selected {
            Style::default()
                .fg(THEME.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let w = chunks[1].width.saturating_sub(2) as usize;
        let current_mark = if entry.is_current { "*" } else { " " };
        let cursor_mark = if selected { ">" } else { " " };
        let display = format!("{}/{}", entry.provider, entry.id);
        let line_text = format!(
            "{} {} {}",
            cursor_mark,
            current_mark,
            truncate(&display, w.saturating_sub(5).max(10)),
        );
        lines.push(Line::styled(line_text, style));
    }
    let list = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(list, chunks[1]);

    // 底部提示
    let hint = Line::styled(
        "Enter select · type to filter · Esc close",
        Style::default().fg(THEME.dim),
    );
    frame.render_widget(Paragraph::new(hint), chunks[2]);
}
