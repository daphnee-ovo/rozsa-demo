// components/session_selector.rs — 会话选择器
//
// Internal Framework:
// session_selector.rs
// ├── SessionEntry             会话条目
// ├── FlatSessionNode          扁平树节点
// ├── SessionSelectorState     选择器状态
// ├── handle_session_selector_key()  键盘处理
// └── render_session_selector()      渲染
//
// Related Docs:
// - [TUI Design](../../../docs/rozsa_framework.md#rozsa-tui--ratatui-终端前端)

// components/session_selector.rs — Session 选择器：多 scope、排序、搜索、重命名、删除
//
// Internal Framework:
// session_selector.rs
// ├── SessionEntry                 pub struct 会话条目
// ├── Scope                        pub enum (Current, All)
// ├── SortMode                     pub enum (Threaded, Recent, Relevance)
// ├── NameFilter                   pub enum (All, Named)
// ├── SelectorMode                 pub enum (Normal, Rename, ConfirmDelete)
// ├── StatusMessage                pub struct 状态消息
// ├── FlatSessionNode              pub struct 树节点展平结构
// ├── SessionSelectorState         pub struct 选择器主状态
// ├── handle_session_selector_key() pub fn 按键处理
// └── render_session_selector()    pub fn 渲染面板
//
// Related Docs:
// - [TUI Design](../../../docs/rozsa_framework.md#rozsa-tui--ratatui-终端前端)

use std::{
    error::Error,
    time::Instant,
};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph},
};
use serde::Deserialize;

use crate::{
    protocol::{send, ClientMessage},
    components::sidebar::truncate,
    theme::THEME,
};

#[derive(Clone, Debug, Deserialize)]
pub struct SessionEntry {
    pub path: String,
    pub name: Option<String>,
    #[serde(rename = "firstMessage", default)]
    pub first_message: String,
    #[serde(default)]
    pub cwd: String,
    #[serde(rename = "messageCount", default)]
    pub message_count: u32,
    #[serde(rename = "lastModified")]
    pub last_modified: String,
    #[serde(rename = "parentSessionPath")]
    pub parent_session_path: Option<String>,
    #[serde(rename = "allMessagesText", default)]
    pub all_messages_text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Scope {
    Current,
    All,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SortMode {
    Threaded,
    Recent,
    Relevance,
}

#[derive(Clone, Debug, PartialEq)]
pub enum NameFilter {
    All,
    Named,
}

#[derive(Clone, Debug)]
pub enum SelectorMode {
    Normal,
    Rename { buffer: String },
    ConfirmDelete { path: String },
}

#[derive(Clone, Debug)]
pub struct StatusMessage {
    pub text: String,
    pub is_error: bool,
    pub created: Instant,
}

#[derive(Clone, Debug)]
pub struct FlatSessionNode {
    pub entry_index: usize,
    pub depth: usize,
    pub is_last: bool,
    pub ancestor_continues: Vec<bool>,
}

#[derive(Clone, Debug)]
pub struct SessionSelectorState {
    pub entries: Vec<SessionEntry>,
    pub display_nodes: Vec<FlatSessionNode>,
    pub selected: usize,
    pub query: String,
    pub mode: SelectorMode,
    pub scope: Scope,
    pub sort_mode: SortMode,
    pub name_filter: NameFilter,
    pub show_path: bool,
    pub loading: bool,
    /// 追踪正在加载的 scope，防止快速切换时显示错误 scope 的数据
    pub loading_scope: Option<Scope>,
    pub current_session_path: Option<String>,
    pub status_message: Option<StatusMessage>,
}

impl SessionSelectorState {
    pub fn new(entries: Vec<SessionEntry>, current_session_path: Option<String>) -> Self {
        let mut state = Self {
            entries,
            display_nodes: Vec::new(),
            selected: 0,
            query: String::new(),
            mode: SelectorMode::Normal,
            scope: Scope::Current,
            sort_mode: SortMode::Threaded,
            name_filter: NameFilter::All,
            show_path: false,
            loading: false,
            loading_scope: None,
            current_session_path,
            status_message: None,
        };
        state.rebuild_display();
        state
    }

    pub fn selected_entry(&self) -> Option<&SessionEntry> {
        self.display_nodes
            .get(self.selected)
            .and_then(|node| self.entries.get(node.entry_index))
    }

    /// 显示名称：有 name 用 name，否则用 firstMessage
    pub fn display_name(entry: &SessionEntry) -> std::borrow::Cow<'_, str> {
        let raw = entry
            .name
            .as_deref()
            .filter(|n| !n.is_empty())
            .unwrap_or(&entry.first_message);
        // session 的 firstMessage 可能是展开的 skill XML，提取为 /skill:name
        // 新格式: <skill>\n<name>...</name>
        if let Some(rest) = raw.strip_prefix("<skill>\n<name>") {
            if let Some(end) = rest.find("</name>") {
                let name = &rest[..end];
                return std::borrow::Cow::Owned(format!("/skill:{name}"));
            }
        }
        // 旧格式: <skill name="...">
        if let Some(rest) = raw.strip_prefix("<skill name=\"") {
            if let Some(end) = rest.find('"') {
                let name = &rest[..end];
                return std::borrow::Cow::Owned(format!("/skill:{name}"));
            }
        }
        std::borrow::Cow::Borrowed(raw)
    }

    pub fn rebuild_display(&mut self) {
        // 先按 name_filter 过滤
        let filtered_indices: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| match self.name_filter {
                NameFilter::All => true,
                NameFilter::Named => e.name.as_ref().is_some_and(|n| !n.is_empty()),
            })
            .map(|(i, _)| i)
            .collect();

        // 搜索过滤
        let query = self.query.trim();
        let after_search: Vec<(usize, f64)> = if query.is_empty() {
            filtered_indices.iter().map(|&i| (i, 0.0)).collect()
        } else {
            super::session_search::filter_sessions(&self.entries, &filtered_indices, query)
        };

        // 排序 + 树构建
        if self.sort_mode == SortMode::Threaded && query.is_empty() {
            let indices: Vec<usize> = after_search.iter().map(|(i, _)| *i).collect();
            self.display_nodes = super::session_tree::build_and_flatten(&self.entries, &indices);
        } else {
            let mut sorted = after_search;
            match self.sort_mode {
                SortMode::Recent => {
                    sorted.sort_by(|(a, _), (b, _)| {
                        self.entries[*b]
                            .last_modified
                            .cmp(&self.entries[*a].last_modified)
                    });
                }
                SortMode::Relevance => {
                    sorted.sort_by(|(_, sa), (_, sb)| {
                        sb.partial_cmp(sa).unwrap_or(std::cmp::Ordering::Equal)
                    });
                }
                SortMode::Threaded => {} // 不应到达这里
            }
            self.display_nodes = sorted
                .iter()
                .map(|(i, _)| FlatSessionNode {
                    entry_index: *i,
                    depth: 0,
                    is_last: true,
                    ancestor_continues: vec![],
                })
                .collect();
        }

        self.selected = self.selected.min(self.display_nodes.len().saturating_sub(1));
    }

    pub fn set_entries(&mut self, entries: Vec<SessionEntry>, current_session_path: Option<String>) {
        self.entries = entries;
        self.current_session_path = current_session_path;
        self.loading = false;
        self.rebuild_display();
    }

    pub fn handle_session_deleted(&mut self, path: &str, method: &str, error: Option<&str>) {
        if let Some(err) = error {
            self.status_message = Some(StatusMessage {
                text: format!("删除失败: {}", err),
                is_error: true,
                created: Instant::now(),
            });
        } else {
            self.entries.retain(|e| e.path != path);
            let msg = if method == "trash" {
                "Session moved to trash"
            } else {
                "Session deleted"
            };
            self.status_message = Some(StatusMessage {
                text: msg.to_string(),
                is_error: false,
                created: Instant::now(),
            });
            self.rebuild_display();
        }
    }

    fn is_current_session(&self, path: &str) -> bool {
        self.current_session_path
            .as_deref()
            .is_some_and(|p| p == path)
    }
}

pub fn handle_session_selector_key(
    key: KeyEvent,
    state: SessionSelectorState,
    writer: &crate::input::Writer,
) -> Result<Option<SessionSelectorState>, Box<dyn Error>> {
    match &state.mode {
        SelectorMode::Rename { .. } => handle_rename_key(key, state, writer),
        SelectorMode::ConfirmDelete { .. } => handle_confirm_delete_key(key, state, writer),
        SelectorMode::Normal => handle_normal_key(key, state, writer),
    }
}

fn handle_normal_key(
    key: KeyEvent,
    mut state: SessionSelectorState,
    writer: &crate::input::Writer,
) -> Result<Option<SessionSelectorState>, Box<dyn Error>> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    // Ctrl 组合键
    if ctrl {
        match key.code {
            // Ctrl+S: 切换排序模式
            KeyCode::Char('s') => {
                state.sort_mode = match state.sort_mode {
                    SortMode::Threaded => SortMode::Recent,
                    SortMode::Recent => SortMode::Relevance,
                    SortMode::Relevance => SortMode::Threaded,
                };
                state.rebuild_display();
                return Ok(Some(state));
            }
            // Ctrl+N: 切换 name 过滤
            KeyCode::Char('n') => {
                state.name_filter = match state.name_filter {
                    NameFilter::All => NameFilter::Named,
                    NameFilter::Named => NameFilter::All,
                };
                state.rebuild_display();
                return Ok(Some(state));
            }
            // Ctrl+D: 删除确认
            KeyCode::Char('d') => {
                if let Some(entry) = state.selected_entry() {
                    if state.is_current_session(&entry.path) {
                        state.status_message = Some(StatusMessage {
                            text: "Cannot delete current session".to_string(),
                            is_error: true,
                            created: Instant::now(),
                        });
                    } else {
                        let path = entry.path.clone();
                        state.mode = SelectorMode::ConfirmDelete { path };
                    }
                }
                return Ok(Some(state));
            }
            // Ctrl+P: 切换路径显示
            KeyCode::Char('p') => {
                state.show_path = !state.show_path;
                return Ok(Some(state));
            }
            _ => {}
        }
    }

    match key.code {
        KeyCode::Esc => {
            if state.query.is_empty() {
                Ok(None)
            } else {
                state.query.clear();
                state.rebuild_display();
                Ok(Some(state))
            }
        }
        KeyCode::Tab => {
            // 切换 scope
            state.scope = match state.scope {
                Scope::Current => Scope::All,
                Scope::All => Scope::Current,
            };
            state.loading = true;
            state.loading_scope = Some(state.scope.clone());
            let scope_str = match state.scope {
                Scope::Current => "current",
                Scope::All => "all",
            };
            send(writer, &ClientMessage::ListSessions { scope: scope_str })?;
            Ok(Some(state))
        }
        KeyCode::Up => {
            state.selected = state.selected.saturating_sub(1);
            Ok(Some(state))
        }
        KeyCode::Down => {
            state.selected = (state.selected + 1).min(state.display_nodes.len().saturating_sub(1));
            Ok(Some(state))
        }
        KeyCode::PageUp => {
            state.selected = state.selected.saturating_sub(10);
            Ok(Some(state))
        }
        KeyCode::PageDown => {
            state.selected = (state.selected + 10).min(state.display_nodes.len().saturating_sub(1));
            Ok(Some(state))
        }
        KeyCode::Enter => {
            if let Some(entry) = state.selected_entry() {
                let path = entry.path.clone();
                send(writer, &ClientMessage::SwitchSession { path: &path })?;
            }
            Ok(None)
        }
        KeyCode::Char('r') if ctrl => {
            let buffer = state
                .selected_entry()
                .and_then(|e| e.name.clone())
                .unwrap_or_default();
            state.mode = SelectorMode::Rename { buffer };
            Ok(Some(state))
        }
        KeyCode::Backspace => {
            state.query.pop();
            state.rebuild_display();
            Ok(Some(state))
        }
        KeyCode::Char(ch) if !ctrl => {
            state.query.push(ch);
            state.rebuild_display();
            Ok(Some(state))
        }
        _ => Ok(Some(state)),
    }
}

fn handle_confirm_delete_key(
    key: KeyEvent,
    mut state: SessionSelectorState,
    writer: &crate::input::Writer,
) -> Result<Option<SessionSelectorState>, Box<dyn Error>> {
    match key.code {
        KeyCode::Enter => {
            if let SelectorMode::ConfirmDelete { ref path } = state.mode {
                let p = path.clone();
                send(writer, &ClientMessage::DeleteSession { path: &p })?;
            }
            state.mode = SelectorMode::Normal;
            Ok(Some(state))
        }
        KeyCode::Esc => {
            state.mode = SelectorMode::Normal;
            Ok(Some(state))
        }
        _ => Ok(Some(state)), // 拦截所有其他按键
    }
}

fn handle_rename_key(
    key: KeyEvent,
    mut state: SessionSelectorState,
    writer: &crate::input::Writer,
) -> Result<Option<SessionSelectorState>, Box<dyn Error>> {
    match key.code {
        KeyCode::Esc => {
            state.mode = SelectorMode::Normal;
            Ok(Some(state))
        }
        KeyCode::Enter => {
            if let SelectorMode::Rename { ref buffer } = state.mode {
                if let Some(entry) = state.selected_entry() {
                    let path = entry.path.clone();
                    let name = buffer.clone();
                    send(
                        writer,
                        &ClientMessage::RenameSession {
                            path: &path,
                            name: &name,
                        },
                    )?;
                }
            }
            // 重命名后重新请求列表
            let scope_str = match state.scope {
                Scope::Current => "current",
                Scope::All => "all",
            };
            state.mode = SelectorMode::Normal;
            state.loading = true;
            send(writer, &ClientMessage::ListSessions { scope: scope_str })?;
            Ok(Some(state))
        }
        KeyCode::Backspace => {
            if let SelectorMode::Rename { ref mut buffer } = state.mode {
                buffer.pop();
            }
            Ok(Some(state))
        }
        KeyCode::Char(ch) => {
            if let SelectorMode::Rename { ref mut buffer } = state.mode {
                buffer.push(ch);
            }
            Ok(Some(state))
        }
        _ => Ok(Some(state)),
    }
}

/// 将 ISO 时间字符串格式化为相对时间
fn format_relative_time(iso: &str) -> String {
    use chrono::{DateTime, Utc};
    let Ok(dt) = iso.parse::<DateTime<Utc>>() else {
        return iso.to_string();
    };
    let now = Utc::now();
    let diff = now.signed_duration_since(dt);
    let mins = diff.num_minutes();
    if mins < 1 {
        "now".to_string()
    } else if mins < 60 {
        format!("{}m", mins)
    } else if diff.num_hours() < 24 {
        format!("{}h", diff.num_hours())
    } else if diff.num_days() < 7 {
        format!("{}d", diff.num_days())
    } else if diff.num_days() < 30 {
        format!("{}w", diff.num_days() / 7)
    } else if diff.num_days() < 365 {
        format!("{}mo", diff.num_days() / 30)
    } else {
        format!("{}y", diff.num_days() / 365)
    }
}

pub fn render_session_selector(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    state: &SessionSelectorState,
) {
    frame.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // header + hints
            Constraint::Length(2), // search + blank
            Constraint::Min(3),   // list
            Constraint::Length(1), // scroll indicator / status
        ])
        .margin(1)
        .split(area);

    let border = Block::default().borders(Borders::ALL).title("Sessions");
    frame.render_widget(border, area);

    // === Header ===
    let scope_indicator = match state.scope {
        Scope::Current => "◉ Current | ○ All",
        Scope::All => "○ Current | ◉ All",
    };
    let title = match state.scope {
        Scope::Current => "Resume Session (Current Folder)",
        Scope::All => "Resume Session (All)",
    };
    let sort_label = match state.sort_mode {
        SortMode::Threaded => "Threaded",
        SortMode::Recent => "Recent",
        SortMode::Relevance => "Fuzzy",
    };
    let name_label = match state.name_filter {
        NameFilter::All => "All",
        NameFilter::Named => "Named",
    };
    let header_line1 = format!("{title}    {scope_indicator}  Sort: {sort_label}  Name: {name_label}");

    let (hint_line1, hint_line2) = match &state.mode {
        SelectorMode::ConfirmDelete { .. } => (
            "Delete session? Enter confirm · Esc cancel".to_string(),
            String::new(),
        ),
        _ => {
            let path_state = if state.show_path { "(on)" } else { "(off)" };
            (
                "Tab scope · re:<pattern> regex · \"phrase\" exact".to_string(),
                format!("^S sort · ^N named · ^D delete · ^P path {path_state} · r rename"),
            )
        }
    };

    let header_lines = vec![
        Line::styled(header_line1, Style::default().add_modifier(Modifier::BOLD)),
        Line::styled(
            hint_line1,
            Style::default().fg(if matches!(state.mode, SelectorMode::ConfirmDelete { .. }) {
                THEME.error
            } else {
                THEME.dim
            }),
        ),
        Line::styled(hint_line2, Style::default().fg(THEME.dim)),
    ];
    frame.render_widget(Paragraph::new(header_lines), chunks[0]);

    // === Search ===
    let search_text = match &state.mode {
        SelectorMode::Rename { buffer } => format!("Rename: {buffer}▌"),
        _ => {
            if state.loading {
                "Loading...".to_string()
            } else if state.query.is_empty() {
                "Search: ▌".to_string()
            } else {
                format!("Search: {}▌", state.query)
            }
        }
    };
    let search = Paragraph::new(search_text)
        .block(Block::default().borders(Borders::BOTTOM).border_style(Style::default().fg(THEME.border_muted)));
    frame.render_widget(search, chunks[1]);

    // === Session List ===
    let list_height = chunks[2].height as usize;
    if state.display_nodes.is_empty() {
        let empty_msg = match (&state.scope, &state.name_filter) {
            (Scope::Current, NameFilter::Named) => "No named sessions. Press ^N to show all, or Tab for all folders.",
            (Scope::Current, _) => "No sessions in current folder. Press Tab to view all.",
            (_, NameFilter::Named) => "No named sessions found. Press ^N to show all.",
            _ => "No sessions found.",
        };
        let empty = Paragraph::new(Line::styled(empty_msg, Style::default().fg(THEME.dim)));
        frame.render_widget(empty, chunks[2]);
    } else {
        let start = state.selected.saturating_sub(list_height / 2).min(
            state.display_nodes.len().saturating_sub(list_height),
        );
        let start = start.min(state.display_nodes.len().saturating_sub(1));
        let end = (start + list_height).min(state.display_nodes.len());

        let w = chunks[2].width.saturating_sub(2) as usize;
        let mut lines = Vec::new();

        for row in start..end {
            let node = &state.display_nodes[row];
            let entry = &state.entries[node.entry_index];
            let selected = row == state.selected;
            let is_current = state
                .current_session_path
                .as_deref()
                .is_some_and(|p| p == entry.path);

            // 树形前缀
            let prefix = build_tree_prefix(node);

            // 显示名称
            let display_text = SessionSelectorState::display_name(entry);
            let normalized = display_text
                .chars()
                .map(|c| if c.is_control() { ' ' } else { c })
                .collect::<String>();

            // 右侧信息
            let age = format_relative_time(&entry.last_modified);
            let msg_count = entry.message_count.to_string();
            let mut right_parts = vec![msg_count.as_str(), age.as_str()];
            let cwd_short;
            if state.scope == Scope::All && !entry.cwd.is_empty() {
                cwd_short = shorten_path(&entry.cwd);
                right_parts.insert(0, &cwd_short);
            }
            let path_short;
            if state.show_path {
                path_short = shorten_path(&entry.path);
                right_parts.insert(0, &path_short);
            }
            let right = right_parts.join(" ");

            // 选中标记
            let cursor = if selected { "› " } else { "  " };

            // 计算可用宽度
            let prefix_len = prefix.chars().count();
            let right_len = right.chars().count() + 2;
            let available = w.saturating_sub(2 + prefix_len + right_len);
            let truncated_name = truncate(&normalized, available.max(8));

            // 样式
            let fg_color = if matches!(state.mode, SelectorMode::ConfirmDelete { ref path } if *path == entry.path) {
                THEME.error
            } else if is_current {
                THEME.accent
            } else if entry.name.as_ref().is_some_and(|n| !n.is_empty()) {
                THEME.warning
            } else {
                Color::Reset
            };

            let mut style = Style::default().fg(fg_color);
            if selected {
                style = style.add_modifier(Modifier::BOLD);
            }

            let spacing = w.saturating_sub(2 + prefix_len + truncated_name.chars().count() + right_len);
            let line_text = format!(
                "{cursor}{prefix}{truncated_name}{}{right}",
                " ".repeat(spacing.max(1))
            );
            lines.push(Line::styled(line_text, style));
        }

        let list = Paragraph::new(lines);
        frame.render_widget(list, chunks[2]);

        // 滚动指示
        if state.display_nodes.len() > list_height {
            let scroll_text = format!("  ({}/{})", state.selected + 1, state.display_nodes.len());
            let scroll = Paragraph::new(Line::styled(scroll_text, Style::default().fg(THEME.dim)));
            frame.render_widget(scroll, chunks[3]);
        }
    }

    // Status message (覆盖滚动指示)
    if let Some(ref msg) = state.status_message {
        if msg.created.elapsed().as_secs() < 3 {
            let color = if msg.is_error {
                THEME.error
            } else {
                THEME.accent
            };
            let status = Paragraph::new(Line::styled(&msg.text[..], Style::default().fg(color)));
            frame.render_widget(status, chunks[3]);
        }
    }
}

fn build_tree_prefix(node: &FlatSessionNode) -> String {
    if node.depth == 0 {
        return String::new();
    }
    let mut parts = String::new();
    for &continues in &node.ancestor_continues {
        parts.push_str(if continues { "│  " } else { "   " });
    }
    parts.push_str(if node.is_last { "└─ " } else { "├─ " });
    parts
}

fn shorten_path(path: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        let home_str = home.to_string_lossy();
        if path.starts_with(home_str.as_ref()) {
            return format!("~{}", &path[home_str.len()..]);
        }
    }
    path.to_string()
}
