// File: ui/layout.rs
//
// Layout helper functions — compute heights/constraints without rendering.
//
// Internal Framework:
// layout.rs
// ├── notification_height()  顶部通知栏高度
// ├── pending_height()       pending messages 区域高度
// └── widget_height()        底部 widgets 区域高度

use std::collections::BTreeMap;

use crate::app::AppState;
use crate::protocol::NativeUiState;

/// 顶部短通知栏高度（只显示 <=3 行的通知）
pub(super) fn notification_height(state: &AppState) -> u16 {
    // 只显示短通知（<=3行）在顶部
    let short_count = state
        .notifications
        .iter()
        .filter(|n| n.message.lines().count() <= 3)
        .count();
    if short_count == 0 {
        0
    } else {
        (short_count as u16).min(3)
    }
}

/// pending messages 区域高度
pub(super) fn pending_height(ui: &NativeUiState) -> u16 {
    if ui.pending_messages.is_empty() {
        0
    } else {
        (ui.pending_messages.len() as u16 + 1).min(4)
    }
}

/// 底部 widgets 区域高度
pub(super) fn widget_height(widgets: &BTreeMap<String, Vec<String>>) -> u16 {
    widgets
        .values()
        .map(|lines| lines.len() as u16)
        .sum::<u16>()
        .min(12)
}
