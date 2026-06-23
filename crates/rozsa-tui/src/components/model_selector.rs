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
};

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
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
    /// 0 = All, 1..N = 各 provider（按 tabs() 顺序）
    pub active_tab: usize,
}

impl ModelSelectorState {
    pub fn new(models: Vec<ModelEntry>) -> Self {
        let filtered_indices = (0..models.len()).collect::<Vec<_>>();
        Self {
            models,
            filter: String::new(),
            selected: 0,
            filtered_indices,
            active_tab: 0,
        }
    }

    /// 返回 tab 列表：["All", provider1, provider2, ...]（去重保序）
    pub fn tabs(&self) -> Vec<&str> {
        let mut tabs: Vec<&str> = vec!["All"];
        for entry in &self.models {
            let name = provider_display_name(&entry.provider);
            if !tabs.contains(&name) {
                tabs.push(name);
            }
        }
        tabs
    }

    pub fn next_tab(&mut self) {
        let count = self.tabs().len();
        self.active_tab = (self.active_tab + 1) % count;
        self.apply_filter();
    }

    pub fn prev_tab(&mut self) {
        let count = self.tabs().len();
        self.active_tab = (self.active_tab + count - 1) % count;
        self.apply_filter();
    }

    /// 模糊匹配 + tab 筛选
    fn apply_filter(&mut self) {
        let filter_lower = self.filter.to_lowercase();
        let tabs = self.tabs();
        let active_provider = if self.active_tab == 0 {
            None
        } else {
            tabs.get(self.active_tab).copied()
        };

        self.filtered_indices = self
            .models
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                if let Some(provider) = active_provider {
                    if provider_display_name(&entry.provider) != provider {
                        return None;
                    }
                }
                let haystack = format_model_display(&entry.provider, &entry.id).to_lowercase();
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

/// 将 provider ID 转换为展示名称（首字母大写或特殊品牌名）
fn provider_display_name(id: &str) -> &str {
    match id {
        "anthropic" => "Anthropic",
        "openai" => "OpenAI",
        "amazon-bedrock" => "Bedrock",
        "google" => "Google",
        "google-vertex" => "Vertex",
        "deepseek" => "DeepSeek",
        "openrouter" => "OpenRouter",
        "xai" => "xAI",
        "groq" => "Groq",
        "cerebras" => "Cerebras",
        "mistral" => "Mistral",
        "nvidia" => "Nvidia",
        "zai" => "Zai",
        "together" => "Together",
        "moonshotai" | "moonshotai-cn" => "MoonshotAI",
        "huggingface" => "HuggingFace",
        "cloudflare-workers-ai" | "cloudflare-ai-gateway" => "Cloudflare",
        "xiaomi" | "xiaomi-token-plan-cn" | "xiaomi-token-plan-ams" | "xiaomi-token-plan-sgp" => "Xiaomi",
        other => other,
    }
}

/// 格式化模型展示文本：[Provider]model_id
fn format_model_display(provider: &str, id: &str) -> String {
    format!("[{}] {}", provider_display_name(provider), id)
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
    writer: &crate::input::Writer,
) -> Result<Option<ModelSelectorState>, Box<dyn Error>> {
    match key.code {
        KeyCode::Esc => Ok(None),
        KeyCode::Tab => {
            state.next_tab();
            Ok(Some(state))
        }
        KeyCode::BackTab => {
            state.prev_tab();
            Ok(Some(state))
        }
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
                let entry = state.models[index].clone();
                send(
                    writer,
                    &ClientMessage::SwitchModel {
                        provider: &entry.provider,
                        id: &entry.id,
                    },
                )?;
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
            Constraint::Length(1), // tab 栏
            Constraint::Length(3), // 搜索框
            Constraint::Min(3),   // 模型列表
            Constraint::Length(1), // 底部提示
        ])
        .margin(1)
        .split(area);

    let border = Block::default().borders(Borders::ALL).title("Models");
    frame.render_widget(border, area);

    // Tab 栏（左侧留 1 字符 padding）
    let tabs = state.tabs();
    let max_width = chunks[0].width as usize;
    let mut tab_spans: Vec<Span> = Vec::new();
    tab_spans.push(Span::raw(" "));
    let mut used_width = 1;
    for (i, tab_name) in tabs.iter().enumerate() {
        if i > 0 && used_width < max_width {
            tab_spans.push(Span::raw("  "));
            used_width += 2;
        }
        let label = format!(" {} ", tab_name);
        let label_width = label.len();
        if used_width + label_width > max_width {
            break;
        }
        let style = if i == state.active_tab {
            Style::default().bg(THEME.accent).fg(ratatui::style::Color::Black).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(THEME.text)
        };
        tab_spans.push(Span::styled(label, style));
        used_width += label_width;
    }
    let tab_line = Line::from(tab_spans);
    frame.render_widget(Paragraph::new(tab_line), chunks[0]);

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
    frame.render_widget(search, chunks[1]);

    // 模型列表
    let list_height = chunks[2].height as usize;
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
        let w = chunks[2].width.saturating_sub(2) as usize;
        let current_mark = if entry.is_current { "*" } else { " " };
        let cursor_mark = if selected { ">" } else { " " };
        let display = format_model_display(&entry.provider, &entry.id);
        let line_text = format!(
            "{} {} {}",
            cursor_mark,
            current_mark,
            truncate(&display, w.saturating_sub(5).max(10)),
        );
        lines.push(Line::styled(line_text, style));
    }
    let list = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(list, chunks[2]);

    // 底部提示
    let hint = Line::styled(
        "Tab/S-Tab switch · Enter select · type to filter · Esc close",
        Style::default().fg(THEME.dim),
    );
    frame.render_widget(Paragraph::new(hint), chunks[3]);
}
