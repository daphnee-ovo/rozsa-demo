// File: render/mod.rs
//
// Internal Framework:
// render/
// ├── mod.rs .............. 模块入口, 缓存基础设施, render() 主入口
// │   ├── MSG_CACHE       thread_local LRU 缓存（消息 → Lines）
// │   ├── HEIGHT_CACHE    thread_local LRU 缓存（消息 → 行数，虚拟滚动用）
// │   ├── hash_message()  AgentMessage → u64 hash (基于 JSON 序列化)
// │   ├── cached_message_lines()  缓存层
// │   ├── cached_message_height() 行数缓存层（虚拟滚动用，不 clone Lines）
// │   └── render()        pub 主渲染入口
// ├── layout.rs ........... 布局计算 (notification_height, pending_height, widget_height)
// ├── messages.rs ......... render_messages + message_lines + 消息格式化辅助函数
// ├── input_box.rs ........ 输入框渲染
// ├── status.rs ........... 状态行 / 通知 / pending / widgets 渲染
// ├── dialog.rs ........... Dialog overlay + centered_rect
// └── overlay.rs .......... Overlay 定位与焦点
//
// Related Docs:
// - [TUI Architecture](../../../docs/tui/architecture.md)

mod dialog;
mod input_box;
mod layout;
mod messages;
pub mod overlay;
mod status;

use std::hash::{Hash, Hasher};

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::Line;
use rozsa_core::messages::AgentMessage;
use rozsa_model::types::Message;

use crate::{
    app::AppState, backend::SubagentView, input::InputState,
    panels::autocomplete::render_autocomplete, panels::graph::render_graph,
    panels::model_selector::render_model_selector, panels::permission::render_permission,
    panels::session_selector::render_session_selector, panels::sidebar::render_sidebar,
};

use dialog::{centered_rect, render_dialog};
use input_box::render_input;
use layout::{notification_height, pending_height, widget_height};
use messages::render_messages;
use status::{render_notifications, render_pending, render_status, render_widgets};

/// 消息渲染缓存 — 避免对未变化的消息重复格式化
/// key: 消息内容的 hash, value: 格式化后的 Lines
/// 使用 LRU 策略淘汰，避免周期性全量重格式化
use std::cell::RefCell;
thread_local! {
    static MSG_CACHE: RefCell<lru::LruCache<u64, Vec<Line<'static>>>> =
        RefCell::new(lru::LruCache::new(std::num::NonZeroUsize::new(500).unwrap()));
    // 行高缓存：消息 → 渲染行数。键包含 width，宽度变化自动失效。
    // 用于虚拟滚动定位可见窗口，无需 clone Lines。
    static HEIGHT_CACHE: RefCell<lru::LruCache<u64, usize>> =
        RefCell::new(lru::LruCache::new(std::num::NonZeroUsize::new(2000).unwrap()));
}

/// Hash an AgentMessage by serializing to JSON. AgentMessage and its inner
/// types are not Hash but serialize stably, and that's sufficient for cache
/// keying — fully-formed messages don't mutate after MessageEnd, so the JSON
/// hash matches the displayed content exactly.
fn hash_message(m: &AgentMessage) -> u64 {
    let mut hasher = std::hash::DefaultHasher::new();
    match m {
        AgentMessage::Standard { message } => {
            // Tag the standard variant to avoid collision with a Custom
            // message that happens to serialize to the same bytes.
            "standard".hash(&mut hasher);
            // Standard Message types serialize to flat camelCase JSON.
            if let Ok(s) = serde_json::to_string(message) {
                s.hash(&mut hasher);
            } else {
                // Fall back to discriminant + timestamp.
                match message {
                    Message::User(u) => ("user", u.timestamp).hash(&mut hasher),
                    Message::Assistant(a) => ("assistant", a.timestamp).hash(&mut hasher),
                    Message::ToolResult(t) => ("toolResult", t.timestamp).hash(&mut hasher),
                }
            }
        }
        AgentMessage::Custom { message } => {
            "custom".hash(&mut hasher);
            message.message_type.hash(&mut hasher);
            message.timestamp.hash(&mut hasher);
            message.payload.to_string().hash(&mut hasher);
        }
    }
    hasher.finish()
}

fn message_render_key(
    message: &AgentMessage,
    tools_expanded: bool,
    thinking_visible: bool,
    show_images: bool,
    compaction_collapsed: bool,
    width: usize,
) -> u64 {
    let mut hasher = std::hash::DefaultHasher::new();
    hash_message(message).hash(&mut hasher);
    tools_expanded.hash(&mut hasher);
    thinking_visible.hash(&mut hasher);
    show_images.hash(&mut hasher);
    compaction_collapsed.hash(&mut hasher);
    width.hash(&mut hasher);
    hasher.finish()
}

pub(crate) fn cached_message_lines(
    message: &AgentMessage,
    tools_expanded: bool,
    thinking_visible: bool,
    show_images: bool,
    compaction_collapsed: bool,
    is_last_streaming: bool,
    width: usize,
) -> Vec<Line<'static>> {
    // streaming 中的最后一条消息不缓存（内容持续变化）
    if is_last_streaming {
        return messages::message_lines(
            message,
            tools_expanded,
            thinking_visible,
            show_images,
            compaction_collapsed,
            is_last_streaming,
            width,
        );
    }

    let key = message_render_key(
        message,
        tools_expanded,
        thinking_visible,
        show_images,
        compaction_collapsed,
        width,
    );
    MSG_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(lines) = cache.get(&key) {
            return lines.clone();
        }
        let lines = messages::message_lines(
            message,
            tools_expanded,
            thinking_visible,
            show_images,
            compaction_collapsed,
            false,
            width,
        );
        // 同步更新 height cache（顺手缓存，避免下次单独算）
        HEIGHT_CACHE.with(|hc| {
            hc.borrow_mut().put(key, lines.len());
        });
        cache.put(key, lines.clone());
        lines
    })
}

/// 返回消息渲染后的行数，不构造 Lines。供虚拟滚动定位可见窗口使用。
///
/// is_last_streaming 时不缓存，每帧重算（内容仍在增长）。
pub(crate) fn cached_message_height(
    message: &AgentMessage,
    tools_expanded: bool,
    thinking_visible: bool,
    show_images: bool,
    compaction_collapsed: bool,
    is_last_streaming: bool,
    width: usize,
) -> usize {
    if is_last_streaming {
        return messages::message_lines(
            message,
            tools_expanded,
            thinking_visible,
            show_images,
            compaction_collapsed,
            is_last_streaming,
            width,
        )
        .len();
    }
    let key = message_render_key(
        message,
        tools_expanded,
        thinking_visible,
        show_images,
        compaction_collapsed,
        width,
    );
    if let Some(h) = HEIGHT_CACHE.with(|hc| hc.borrow_mut().get(&key).copied()) {
        return h;
    }
    // miss：构造一次行（同时填充两个 cache）— 走 cached_message_lines 复用逻辑
    cached_message_lines(
        message,
        tools_expanded,
        thinking_visible,
        show_images,
        compaction_collapsed,
        false,
        width,
    )
    .len()
}

pub fn render(
    frame: &mut ratatui::Frame<'_>,
    state: &AppState,
    input: &InputState,
    agents: Option<&dyn SubagentView>,
) {
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
        render_sidebar(frame, shell[2], state, agents);
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
