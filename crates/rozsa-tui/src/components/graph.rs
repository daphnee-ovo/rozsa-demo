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

use crate::{keymap::matches_action, markdown::parse_markdown, protocol::NativeGraphNode, theme::THEME};
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
}

impl GraphState {
    pub fn new(nodes: Vec<NativeGraphNode>) -> Self {
        let filtered = (0..nodes.len()).collect::<Vec<_>>();
        let selected = filtered.len().saturating_sub(1);
        Self {
            nodes,
            filtered,
            selected,
            query: String::new(),
            mode: GraphMode::List,
            detail_scroll: 0,
        }
    }

    fn selected_node(&self) -> Option<&NativeGraphNode> {
        self.filtered
            .get(self.selected)
            .and_then(|index| self.nodes.get(*index))
    }

    fn apply_filter(&mut self) {
        let tokens = self
            .query
            .to_lowercase()
            .split_whitespace()
            .map(str::to_string)
            .collect::<Vec<_>>();
        self.filtered = self
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(index, node)| {
                let haystack =
                    format!("{} {} {}", node.role, node.summary, node.full_text).to_lowercase();
                if tokens.iter().all(|token| haystack.contains(token)) {
                    Some(index)
                } else {
                    None
                }
            })
            .collect();
        self.selected = self.selected.min(self.filtered.len().saturating_sub(1));
    }
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
        if graph.query.is_empty() {
            None
        } else {
            graph.query.clear();
            graph.apply_filter();
            Some(graph)
        }
    } else if matches_action(keybindings, key, "tui.select.up") {
        graph.selected = graph.selected.saturating_sub(1);
        Some(graph)
    } else if matches_action(keybindings, key, "tui.select.down") {
        graph.selected = (graph.selected + 1).min(graph.filtered.len().saturating_sub(1));
        Some(graph)
    } else if matches_action(keybindings, key, "tui.select.pageUp") {
        graph.selected = graph.selected.saturating_sub(5);
        Some(graph)
    } else if matches_action(keybindings, key, "tui.select.pageDown") {
        graph.selected = (graph.selected + 5).min(graph.filtered.len().saturating_sub(1));
        Some(graph)
    } else if matches_action(keybindings, key, "tui.select.confirm") {
        graph.mode = GraphMode::Detail;
        graph.detail_scroll = 0;
        Some(graph)
    } else if matches_action(keybindings, key, "tui.editor.deleteCharBackward") {
        graph.query.pop();
        graph.apply_filter();
        Some(graph)
    } else {
        match key.code {
            KeyCode::Char(ch) => {
                graph.query.push(ch);
                graph.apply_filter();
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
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);
    let left = render_node_list(chunks[0], graph);
    let right = render_preview(chunks[1], graph);
    frame.render_widget(left, chunks[0]);
    frame.render_widget(right, chunks[1]);
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
        let (icon, role_color) = if node.role == "user" {
            ("›", THEME.accent)
        } else {
            ("◆", THEME.assistant_msg)
        };
        let cursor = if selected { "▸ " } else { "  " };
        let summary_max = area.width.saturating_sub(14) as usize;
        let summary = super::sidebar::truncate(&node.summary, summary_max);

        let mut spans = vec![
            Span::styled(cursor, Style::default().fg(if selected { THEME.accent } else { THEME.text })),
            Span::styled(format!("{icon} "), Style::default().fg(role_color)),
            Span::styled(format!("{} ", node.timestamp), Style::default().fg(THEME.muted)),
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
    let role_color = if node.role == "user" { THEME.accent } else { THEME.assistant_msg };
    let role_label = format!("▎{}", node.role.to_uppercase());
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                role_label,
                Style::default().fg(role_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {}  {}/{}", node.timestamp, graph.selected + 1, graph.filtered.len()),
                Style::default().fg(THEME.muted),
            ),
        ]),
        Line::styled(
            "╌".repeat(area.width.min(40) as usize),
            Style::default().fg(THEME.border_muted),
        ),
    ];
    let parsed = parse_markdown(&node.full_text);
    for line in parsed.into_iter().take(area.height.saturating_sub(5) as usize) {
        lines.push(line);
    }
    lines.push(Line::styled(
        "Enter expand · type filter · Esc close",
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
    let role_color = if node.role == "user" { THEME.accent } else { THEME.assistant_msg };
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
