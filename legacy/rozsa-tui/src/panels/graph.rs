// components/graph.rs — 会话历史图
//
// Internal Framework:
// graph.rs
// ├── GraphMode         枚举 (List, Detail)
// ├── GraphState        会话图状态
// ├── handle_graph_key()  键盘事件处理
// └── render_graph()      图渲染
//
// Related Docs:
// - [TUI Design](../../../docs/rozsa_framework.md#rozsa-tui--ratatui-终端前端)

// components/graph.rs — Session 历史图查看器：消息列表、预览和详情模式
//
// Internal Framework:
// graph.rs
// ├── GraphMode           enum (List, Detail)
// ├── GraphState          pub struct 图状态（节点、过滤、选中、滚动）
// ├── handle_graph_key()  pub fn 按键处理
// └── render_graph()      pub fn 渲染面板
//
// Related Docs:
// - [TUI Design](../../../docs/rozsa_framework.md#rozsa-tui--ratatui-终端前端)

use std::collections::BTreeMap;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::{
    input::keymap::matches_action,
    protocol::NativeGraphNode,
    theme::THEME,
    util::markdown::parse_markdown,
    widgets::{HintItem, TabBarState, render_hints_bar, render_tab_bar},
};
// truncate is public in sidebar module

#[derive(Clone, Debug)]
pub enum GraphMode {
    List,
    Detail,
}

#[derive(Clone, Debug)]
pub struct GraphState {
    nodes: Vec<NativeGraphNode>,
    filtered: Vec<usize>,
    selected: usize,
    query: String,
    mode: GraphMode,
    detail_scroll: usize,
    fork_mode: bool,
    /// Set when user confirms a selection in fork mode (original node index).
    pub fork_confirmed: Option<usize>,
    searching: bool,
    show_tools: bool,
    /// Tab 列表：第 0 个为 "main"，其余为 subagent id。
    pub tabs: Vec<String>,
    /// 当前激活的 tab 下标。
    pub active_tab: usize,
    /// 选中 agent 节点时，对应 tab 的高亮下标。
    pub tab_highlight: Option<usize>,
}

impl GraphState {
    pub fn new(nodes: Vec<NativeGraphNode>) -> Self {
        let tabs = collect_tabs(&nodes);
        let mut state = Self {
            nodes,
            filtered: Vec::new(),
            selected: 0,
            query: String::new(),
            mode: GraphMode::List,
            detail_scroll: 0,
            fork_mode: false,
            fork_confirmed: None,
            searching: false,
            show_tools: false,
            tabs,
            active_tab: 0,
            tab_highlight: None,
        };
        state.apply_filter();
        state.selected = state.filtered.len().saturating_sub(1);
        state.refresh_tab_highlight();
        state
    }

    pub fn new_fork(nodes: Vec<NativeGraphNode>) -> Self {
        let tabs = collect_tabs(&nodes);
        let mut state = Self {
            nodes,
            filtered: Vec::new(),
            selected: 0,
            query: String::new(),
            mode: GraphMode::List,
            detail_scroll: 0,
            fork_mode: true,
            fork_confirmed: None,
            searching: false,
            show_tools: false,
            tabs,
            active_tab: 0,
            tab_highlight: None,
        };
        state.apply_filter();
        state.selected = state.filtered.len().saturating_sub(1);
        state.refresh_tab_highlight();
        state
    }

    fn selected_node(&self) -> Option<&NativeGraphNode> {
        self.filtered
            .get(self.selected)
            .and_then(|index| self.nodes.get(*index))
    }

    /// 当前激活 tab 对应的 agent id：main → None，subagent → Some(id)。
    fn active_agent_id(&self) -> Option<&str> {
        if self.active_tab == 0 {
            None
        } else {
            self.tabs.get(self.active_tab).map(String::as_str)
        }
    }

    fn apply_filter(&mut self) {
        let tokens = self
            .query
            .to_lowercase()
            .split_whitespace()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let active = self.active_agent_id().map(str::to_string);
        self.filtered = self
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(index, node)| {
                let haystack =
                    format!("{} {} {}", node.role, node.summary, node.full_text).to_lowercase();
                if !tokens.iter().all(|token| haystack.contains(token)) {
                    return None;
                }
                // 按 active tab 过滤：main tab 只显示 agent_id == None 的节点；
                // subagent tab 只显示 agent_id == Some(id) 的节点。
                let belongs = match (active.as_deref(), node.agent_id.as_deref()) {
                    (None, None) => true,
                    (Some(want), Some(got)) => want == got,
                    _ => false,
                };
                if !belongs {
                    return None;
                }
                Some(index)
            })
            .collect();
        if !self.show_tools {
            // 只隐藏 tool 节点；agent_spawn 节点不受 show_tools 控制。
            self.filtered.retain(|&idx| self.nodes[idx].role != "tool");
        }
        self.selected = self.selected.min(self.filtered.len().saturating_sub(1));
    }

    /// 当选中的节点是 agent_spawn 时，把对应 tab 标为 highlight。
    fn refresh_tab_highlight(&mut self) {
        self.tab_highlight = self.selected_node().and_then(|node| {
            if node.role != "agent_spawn" {
                return None;
            }
            // summary 形如 "⊕ <name> (<id>)"，匹配 tab 中存的 id。
            self.tabs
                .iter()
                .enumerate()
                .skip(1)
                .find(|(_, id)| node.summary.contains(id.as_str()))
                .map(|(i, _)| i)
        });
    }
}

/// 扫描节点列出现过的 agent_id，生成 ["main", id1, id2, ...] 的 tab 列表。
fn collect_tabs(nodes: &[NativeGraphNode]) -> Vec<String> {
    let mut tabs = vec!["main".to_string()];
    for node in nodes {
        if let Some(id) = &node.agent_id {
            if !tabs.iter().any(|t| t == id) {
                tabs.push(id.clone());
            }
        }
    }
    tabs
}

pub fn handle_graph_key(
    key: KeyEvent,
    graph: GraphState,
    keybindings: &BTreeMap<String, Vec<String>>,
) -> Option<GraphState> {
    match graph.mode {
        GraphMode::List => handle_list_key(key, graph, keybindings),
        GraphMode::Detail => handle_detail_key(key, graph, keybindings),
    }
}

fn handle_list_key(
    key: KeyEvent,
    mut graph: GraphState,
    keybindings: &BTreeMap<String, Vec<String>>,
) -> Option<GraphState> {
    if matches_action(keybindings, key, "tui.select.cancel") {
        if graph.searching {
            if graph.query.is_empty() {
                graph.searching = false;
            } else {
                graph.query.clear();
                graph.apply_filter();
                graph.refresh_tab_highlight();
            }
            Some(graph)
        } else {
            None
        }
    } else if matches_action(keybindings, key, "tui.select.up") {
        graph.selected = graph.selected.saturating_sub(1);
        graph.refresh_tab_highlight();
        Some(graph)
    } else if matches_action(keybindings, key, "tui.select.down") {
        graph.selected = (graph.selected + 1).min(graph.filtered.len().saturating_sub(1));
        graph.refresh_tab_highlight();
        Some(graph)
    } else if matches_action(keybindings, key, "tui.select.pageUp") {
        graph.selected = graph.selected.saturating_sub(5);
        graph.refresh_tab_highlight();
        Some(graph)
    } else if matches_action(keybindings, key, "tui.select.pageDown") {
        graph.selected = (graph.selected + 5).min(graph.filtered.len().saturating_sub(1));
        graph.refresh_tab_highlight();
        Some(graph)
    } else if matches_action(keybindings, key, "tui.select.confirm") {
        if graph.fork_mode {
            graph.fork_confirmed = graph.filtered.get(graph.selected).copied();
            Some(graph)
        } else {
            graph.mode = GraphMode::Detail;
            graph.detail_scroll = 0;
            Some(graph)
        }
    } else if key.code == KeyCode::Backspace {
        if graph.searching {
            graph.query.pop();
            graph.apply_filter();
        }
        Some(graph)
    } else {
        match key.code {
            KeyCode::Char('/') if !graph.searching => {
                graph.searching = true;
                Some(graph)
            }
            KeyCode::Char('j') if !graph.searching => {
                graph.selected = (graph.selected + 1).min(graph.filtered.len().saturating_sub(1));
                graph.refresh_tab_highlight();
                Some(graph)
            }
            KeyCode::Char('k') if !graph.searching => {
                graph.selected = graph.selected.saturating_sub(1);
                graph.refresh_tab_highlight();
                Some(graph)
            }
            KeyCode::Char('o') if !graph.searching => {
                graph.show_tools = !graph.show_tools;
                graph.apply_filter();
                graph.refresh_tab_highlight();
                Some(graph)
            }
            KeyCode::Left | KeyCode::BackTab if !graph.searching => {
                if graph.tabs.len() > 1 {
                    graph.active_tab = (graph.active_tab + graph.tabs.len() - 1) % graph.tabs.len();
                    graph.apply_filter();
                    graph.selected = graph.filtered.len().saturating_sub(1);
                    graph.refresh_tab_highlight();
                }
                Some(graph)
            }
            KeyCode::Right | KeyCode::Tab if !graph.searching => {
                if graph.tabs.len() > 1 {
                    graph.active_tab = (graph.active_tab + 1) % graph.tabs.len();
                    graph.apply_filter();
                    graph.selected = graph.filtered.len().saturating_sub(1);
                    graph.refresh_tab_highlight();
                }
                Some(graph)
            }
            KeyCode::Char(ch) if graph.searching => {
                graph.query.push(ch);
                graph.apply_filter();
                graph.refresh_tab_highlight();
                Some(graph)
            }
            _ => Some(graph),
        }
    }
}

fn handle_detail_key(
    key: KeyEvent,
    mut graph: GraphState,
    keybindings: &BTreeMap<String, Vec<String>>,
) -> Option<GraphState> {
    let line_count = graph
        .selected_node()
        .map(|node| parse_markdown(&node.full_text).len())
        .unwrap_or(0);
    let max_scroll = line_count.saturating_sub(1);
    if matches_action(keybindings, key, "tui.select.cancel")
        || matches_action(keybindings, key, "tui.select.confirm")
    {
        graph.mode = GraphMode::List;
        graph.detail_scroll = 0;
        Some(graph)
    } else if matches_action(keybindings, key, "tui.select.up") {
        graph.detail_scroll = graph.detail_scroll.saturating_sub(1);
        Some(graph)
    } else if matches_action(keybindings, key, "tui.select.down") {
        graph.detail_scroll = (graph.detail_scroll + 1).min(max_scroll);
        Some(graph)
    } else if matches_action(keybindings, key, "tui.select.pageUp") {
        graph.detail_scroll = graph.detail_scroll.saturating_sub(10);
        Some(graph)
    } else if matches_action(keybindings, key, "tui.select.pageDown") {
        graph.detail_scroll = (graph.detail_scroll + 10).min(max_scroll);
        Some(graph)
    } else if matches_action(keybindings, key, "tui.editor.cursorLineStart") {
        graph.detail_scroll = 0;
        Some(graph)
    } else if matches_action(keybindings, key, "tui.editor.cursorLineEnd") {
        graph.detail_scroll = max_scroll;
        Some(graph)
    } else {
        Some(graph)
    }
}

pub fn render_graph(frame: &mut ratatui::Frame<'_>, area: Rect, graph: &GraphState) {
    frame.render_widget(Clear, area);
    match graph.mode {
        GraphMode::List => render_list(frame, area, graph),
        GraphMode::Detail => render_detail(frame, area, graph),
    }
}

fn render_list(frame: &mut ratatui::Frame<'_>, area: Rect, graph: &GraphState) {
    let has_tab_bar = graph.tabs.len() > 1;
    let outer_constraints = if has_tab_bar {
        vec![
            Constraint::Length(1), // tab bar
            Constraint::Min(3),    // content
            Constraint::Length(1), // hints
        ]
    } else {
        vec![Constraint::Min(3), Constraint::Length(1)]
    };
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints(outer_constraints)
        .split(area);

    let (content_area, hints_area) = if has_tab_bar {
        let tab_state = TabBarState {
            tabs: graph.tabs.clone(),
            active: graph.active_tab,
            highlight: graph.tab_highlight,
        };
        render_tab_bar(frame, outer[0], &tab_state);
        (outer[1], outer[2])
    } else {
        (outer[0], outer[1])
    };

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(content_area);
    let left = render_node_list(chunks[0], graph);
    let right = render_preview(chunks[1], graph);
    frame.render_widget(left, chunks[0]);
    frame.render_widget(right, chunks[1]);

    let hints: Vec<HintItem> = if graph.searching {
        vec![
            HintItem {
                key: "Type".into(),
                action: "filter".into(),
            },
            HintItem {
                key: "Backspace".into(),
                action: "delete".into(),
            },
            HintItem {
                key: "Esc".into(),
                action: "clear".into(),
            },
        ]
    } else if has_tab_bar {
        vec![
            HintItem {
                key: "/".into(),
                action: "search".into(),
            },
            HintItem {
                key: "↑↓".into(),
                action: "navigate".into(),
            },
            HintItem {
                key: "←→/Tab".into(),
                action: "switch agent".into(),
            },
            HintItem {
                key: "Enter".into(),
                action: "expand".into(),
            },
            HintItem {
                key: "o".into(),
                action: "tools".into(),
            },
            HintItem {
                key: "Esc".into(),
                action: "close".into(),
            },
        ]
    } else {
        vec![
            HintItem {
                key: "/".into(),
                action: "search".into(),
            },
            HintItem {
                key: "↑↓".into(),
                action: "navigate".into(),
            },
            HintItem {
                key: "Enter".into(),
                action: "expand".into(),
            },
            HintItem {
                key: "o".into(),
                action: "tools".into(),
            },
            HintItem {
                key: "Esc".into(),
                action: "close".into(),
            },
        ]
    };
    render_hints_bar(frame, hints_area, &hints);
}

fn render_node_list(area: Rect, graph: &GraphState) -> Paragraph<'static> {
    let height = area.height.saturating_sub(2) as usize;
    let start = graph.selected.saturating_sub(height / 2);
    let end = (start + height).min(graph.filtered.len());
    let mut lines = Vec::new();
    for row in start..end {
        let Some(index) = graph.filtered.get(row) else {
            continue;
        };
        let node = &graph.nodes[*index];
        let selected = row == graph.selected;
        let (icon, role_color) = match node.role.as_str() {
            "user" => ("›", THEME.accent),
            "tool" => ("⚡", THEME.muted),
            "agent_spawn" => ("⊕", THEME.accent),
            _ => ("◆", THEME.assistant_msg),
        };
        let cursor = if selected { "▸ " } else { "  " };
        let summary_max = area.width.saturating_sub(14) as usize;
        let summary = super::sidebar::truncate(&node.summary, summary_max);

        let mut spans = vec![
            Span::styled(
                cursor,
                Style::default().fg(if selected { THEME.accent } else { THEME.text }),
            ),
            Span::styled(format!("{icon} "), Style::default().fg(role_color)),
            Span::styled(
                format!("{} ", node.timestamp),
                Style::default().fg(THEME.muted),
            ),
            Span::styled("· ", Style::default().fg(THEME.border_muted)),
        ];
        let text_color = if selected { THEME.text } else { THEME.muted };
        spans.push(Span::styled(summary, Style::default().fg(text_color)));

        let mut line = Line::from(spans);
        if selected {
            line = line.style(Style::default().bg(THEME.selected_bg));
        }
        lines.push(line);
    }
    let title = if graph.query.is_empty() {
        "Session Graph".to_string()
    } else {
        format!("Session Graph filter: {}", graph.query)
    };
    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(Block::default().borders(Borders::ALL).title(title))
}

fn render_preview(area: Rect, graph: &GraphState) -> Paragraph<'static> {
    let Some(node) = graph.selected_node() else {
        return Paragraph::new("(empty)")
            .block(Block::default().borders(Borders::ALL).title("Preview"));
    };
    let role_color = match node.role.as_str() {
        "user" => THEME.accent,
        "tool" => THEME.muted,
        _ => THEME.assistant_msg,
    };
    let role_label = format!("▎{}", node.role.to_uppercase());
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                role_label,
                Style::default().fg(role_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    "  {}  {}/{}",
                    node.timestamp,
                    graph.selected + 1,
                    graph.filtered.len()
                ),
                Style::default().fg(THEME.muted),
            ),
        ]),
        Line::styled(
            "╌".repeat(area.width.min(40) as usize),
            Style::default().fg(THEME.border_muted),
        ),
    ];
    let parsed = parse_markdown(&node.full_text);
    for line in parsed
        .into_iter()
        .take(area.height.saturating_sub(5) as usize)
    {
        lines.push(line);
    }
    lines.push(Line::styled(
        "Enter expand · / search · Esc close",
        Style::default().fg(THEME.muted),
    ));
    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(Block::default().borders(Borders::ALL).title("Preview"))
}

fn render_detail(frame: &mut ratatui::Frame<'_>, area: Rect, graph: &GraphState) {
    let Some(node) = graph.selected_node() else {
        return;
    };
    let role_color = match node.role.as_str() {
        "user" => THEME.accent,
        "tool" => THEME.muted,
        _ => THEME.assistant_msg,
    };
    let role_label = format!("▎{}", node.role.to_uppercase());
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                role_label,
                Style::default().fg(role_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {}  Esc back", node.timestamp),
                Style::default().fg(THEME.muted),
            ),
        ]),
        Line::styled(
            "╌".repeat(area.width.min(50) as usize),
            Style::default().fg(THEME.border_muted),
        ),
    ];
    let parsed = parse_markdown(&node.full_text);
    for line in parsed
        .into_iter()
        .skip(graph.detail_scroll)
        .take(area.height.saturating_sub(4) as usize)
    {
        lines.push(line);
    }
    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(Block::default().borders(Borders::ALL).title("Graph Detail"));
    frame.render_widget(paragraph, area);
}
