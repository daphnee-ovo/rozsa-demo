// File: ui/mod.rs
//
// Internal Framework:
// ui/
// ├── mod.rs .............. 模块入口, 缓存基础设施, render() 主入口
// │   ├── MSG_CACHE       thread_local LRU 缓存
// │   ├── hash_value()    JSON value → u64 hash
// │   ├── cached_message_lines()  缓存层
// │   └── render()        pub 主渲染入口
// ├── layout.rs ........... 布局计算 (notification_height, pending_height, widget_height)
// └── render.rs ........... 所有 render_* 函数 + message_lines + helpers
//
// Related Docs:
// - [TUI crate](../../../docs/)

mod layout;
mod render;

use std::hash::{Hash, Hasher};

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::Line;
use serde_json::Value;

use crate::{
    app::AppState,
    components::autocomplete::render_autocomplete,
    components::graph::render_graph,
    components::model_selector::render_model_selector,
    components::permission::render_permission,
    components::session_selector::render_session_selector,
    components::sidebar::render_sidebar,
    input::InputState,
};

use layout::{notification_height, pending_height, widget_height};
use render::{
    centered_rect, render_dialog, render_input, render_messages, render_notifications,
    render_pending, render_status, render_widgets,
};

/// 消息渲染缓存 — 避免对未变化的消息重复格式化
/// key: 消息 JSON 的 hash, value: 格式化后的 Lines
/// 使用 LRU 策略淘汰，避免周期性全量重格式化
use std::cell::RefCell;
thread_local! {
    static MSG_CACHE: RefCell<lru::LruCache<u64, Vec<Line<'static>>>> =
        RefCell::new(lru::LruCache::new(std::num::NonZeroUsize::new(500).unwrap()));
}

fn hash_value(v: &Value) -> u64 {
    let mut hasher = std::hash::DefaultHasher::new();
    // 利用 serde_json 的 compact 格式做稳定 hash
    let s = v.to_string();
    s.hash(&mut hasher);
    hasher.finish()
}

pub(crate) fn cached_message_lines(
    message: &Value,
    tools_expanded: bool,
    thinking_visible: bool,
    show_images: bool,
    compaction_collapsed: bool,
    is_last_streaming: bool,
    width: usize,
) -> Vec<Line<'static>> {
    // streaming 中的最后一条消息不缓存（内容持续变化）
    if is_last_streaming {
        return render::message_lines(
            message,
            tools_expanded,
            thinking_visible,
            show_images,
            compaction_collapsed,
            is_last_streaming,
            width,
        );
    }

    let mut hasher = std::hash::DefaultHasher::new();
    hash_value(message).hash(&mut hasher);
    tools_expanded.hash(&mut hasher);
    thinking_visible.hash(&mut hasher);
    show_images.hash(&mut hasher);
    compaction_collapsed.hash(&mut hasher);
    width.hash(&mut hasher);
    let key = hasher.finish();
    MSG_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(lines) = cache.get(&key) {
            return lines.clone();
        }
        let lines = render::message_lines(message, tools_expanded, thinking_visible, show_images, compaction_collapsed, false, width);
        cache.put(key, lines.clone());
        lines
    })
}

pub fn render(frame: &mut ratatui::Frame<'_>, state: &AppState, input: &InputState) {
    let shell = if frame.area().width >= 108 {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Min(70),
                Constraint::Length(2),
                Constraint::Length(24),
            ])
            .split(frame.area())
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(100),
                Constraint::Length(0),
                Constraint::Length(0),
            ])
            .split(frame.area())
    };

    // 输入框动态高度：内容行数 + 2(边框)，最小 3 行，最大 30% 终端高度
    let term_height = shell[0].height;
    let max_input_height = (term_height as f32 * 0.3).ceil() as u16;
    let content_lines = input.lines.len() as u16;
    let input_height = (content_lines + 2).clamp(3, max_input_height.max(5));

    // 状态行高度：streaming/compacting/retry 时 1 行
    let status_height = if state.ui.is_streaming || state.compacting || state.retry.is_some() {
        1u16
    } else {
        0
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(notification_height(state)),
            Constraint::Min(5), // messages
            Constraint::Length(pending_height(&state.ui)),
            Constraint::Length(status_height), // status: spinner
            Constraint::Length(input_height),  // input: 动态高度
            Constraint::Length(widget_height(&state.ui.widgets_below)),
        ])
        .split(shell[0]);

    render_notifications(frame, chunks[0], state);
    render_messages(frame, chunks[1], state);
    render_pending(frame, chunks[2], &state.ui);
    render_status(frame, chunks[3], state);
    render_input(frame, chunks[4], input, state);
    if let Some(autocomplete) = &state.autocomplete {
        let max_height = (chunks[1].height / 2).max(6);
        let height = (autocomplete.items.len() as u16 + 2).min(max_height);
        let y = chunks[4].y.saturating_sub(height);
        let area = Rect {
            x: chunks[4].x,
            y,
            width: chunks[4].width,
            height,
        };
        render_autocomplete(frame, area, autocomplete);
    }
    render_widgets(frame, chunks[5], &state.ui.widgets_below);
    if shell[2].width > 0 {
        render_sidebar(frame, shell[2], state);
    }

    // overlays
    if let Some(dialog) = &state.dialog {
        render_dialog(frame, centered_rect(60, 40, frame.area()), dialog);
    }
    if let Some(graph) = &state.graph {
        render_graph(frame, frame.area(), graph);
    }
    if let Some(permission) = &state.permission {
        if shell[2].width > 0 {
            let perm_area = Rect {
                x: shell[2].x,
                y: shell[2].y + shell[2].height.saturating_sub(14),
                width: shell[2].width,
                height: 14.min(shell[2].height),
            };
            render_permission(frame, perm_area, permission);
        } else {
            render_permission(frame, centered_rect(64, 52, frame.area()), permission);
        }
    }
    if let Some(session_sel) = &state.session_selector {
        render_session_selector(frame, frame.area(), session_sel);
    }
    if let Some(model_sel) = &state.model_selector {
        render_model_selector(frame, frame.area(), model_sel);
    }
}
