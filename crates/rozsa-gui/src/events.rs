// File: events.rs
//
// 事件转发：每个 Active session tab 有独立的事件监听任务。

use std::sync::Arc;

use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;

use rozsa_app::permissions::ApprovalInfo;
use rozsa_core::events::AgentEvent;

use crate::state::{GuiState, PermissionEvent, SessionTab, ToolEvent, UiSnapshot};

/// 为指定 tab index 的 Active session 启动事件转发任务。
/// 只有 Active 状态的 tab 才能调用此函数。
pub fn spawn_event_forwarder_for_tab(
    app: AppHandle,
    tab_idx: usize,
    gui_state: GuiState,
) {
    let tabs = gui_state.tabs.clone();
    let active_tab = gui_state.active_tab.clone();
    let shared = gui_state.shared.clone();

    tokio::spawn(async move {
        // 获取该 tab 的 agent 的 event receiver
        let rx = {
            let tabs_guard = tabs.lock().await;
            match tabs_guard.get(tab_idx) {
                Some(SessionTab::Active { agent, .. }) => agent.subscribe(),
                _ => return,
            }
        };
        let mut rx = rx;

        loop {
            match rx.recv().await {
                Ok(event) => {
                    // 工具事件单独推送
                    match &event {
                        AgentEvent::ToolExecutionStart { tool_call_id, tool_name, args } => {
                            let _ = app.emit("tool-event", ToolEvent::Start {
                                id: tool_call_id.clone(),
                                name: tool_name.clone(),
                                args: args.clone(),
                            });
                        }
                        AgentEvent::ToolExecutionEnd { tool_call_id, tool_name, result } => {
                            let output = result.content.iter()
                                .filter_map(|b| if let rozsa_model::types::ContentBlock::Text { text, .. } = b { Some(text.as_str()) } else { None })
                                .collect::<Vec<_>>().join("\n");
                            let _ = app.emit("tool-event", ToolEvent::End {
                                id: tool_call_id.clone(),
                                name: tool_name.clone(),
                                success: !result.is_error,
                                output,
                            });
                        }
                        _ => {}
                    }

                    // 累积事件到该 tab 的 LiveState
                    let changed = {
                        let mut tabs_guard = tabs.lock().await;
                        if let Some(SessionTab::Active { live, .. }) = tabs_guard.get_mut(tab_idx) {
                            live.apply(&event)
                        } else {
                            false
                        }
                    };

                    // 只有当前正在看这个 tab 时才 emit 给前端
                    if changed {
                        let current_active = *active_tab.lock().await;
                        if current_active == tab_idx {
                            let tabs_guard = tabs.lock().await;
                            if let Some(tab) = tabs_guard.get(tab_idx) {
                                let snapshot = UiSnapshot::from_tab(tab, &shared);
                                let _ = app.emit("ui-state", &snapshot);
                            }
                        }
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("GUI event forwarder for tab {tab_idx} lagged by {n} events");
                }
            }
        }
    });
}

/// 权限请求监听
pub fn spawn_permission_listener(
    app: AppHandle,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<(String, ApprovalInfo)>,
) {
    tokio::spawn(async move {
        while let Some((request_id, info)) = rx.recv().await {
            let event = PermissionEvent {
                id: request_id,
                tool: info.tool_name.clone(),
                summary: info.args_summary.clone(),
                risk: format!("{:?}", info.risk),
                trust_key: info.trust_key.clone(),
            };
            let _ = app.emit("permission-request", &event);
        }
    });
}
