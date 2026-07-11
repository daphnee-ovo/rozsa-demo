// File: events.rs
//
// 事件转发：每个 Active session tab 有独立的事件监听任务。

use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use rozsa_core::events::AgentEvent;

use crate::state::{
    GuiState, PermissionEvent, PermissionRequest, SessionTab, ToolEvent, UiSnapshot,
    find_tab_index_by_session,
};

/// Start an event forwarder addressed to an immutable session id, not a tab index.
pub fn spawn_event_forwarder_for_session<R: Runtime>(
    app: AppHandle<R>,
    session_id: String,
    gui_state: GuiState,
) {
    let tabs = gui_state.tabs.clone();
    let active_tab = gui_state.active_tab.clone();
    let shared = gui_state.shared.clone();

    tokio::spawn(async move {
        // 获取该 tab 的 agent 的 event receiver
        let rx = {
            let tabs_guard = tabs.lock().await;
            match find_tab_index_by_session(&tabs_guard, &session_id)
                .and_then(|index| tabs_guard.get(index))
            {
                Some(SessionTab::Active { agent, .. }) => agent.subscribe(),
                _ => return,
            }
        };
        let mut rx = rx;
        let mut last_stream_emit = Instant::now() - Duration::from_millis(50);

        loop {
            match rx.recv().await {
                Ok(event) => {
                    let stream_update = matches!(
                        event,
                        AgentEvent::MessageStart { .. }
                            | AgentEvent::MessageUpdate { .. }
                            | AgentEvent::MessageEnd { .. }
                    );
                    let throttled_update = matches!(event, AgentEvent::MessageUpdate { .. });
                    let (changed, turn_id) = {
                        let mut tabs_guard = tabs.lock().await;
                        let Some(index) = find_tab_index_by_session(&tabs_guard, &session_id)
                        else {
                            break;
                        };
                        let Some(SessionTab::Active { live, .. }) = tabs_guard.get_mut(index)
                        else {
                            break;
                        };
                        let changed = live.apply(&event);
                        (changed, live.turn_id)
                    };

                    // 工具事件单独推送
                    match &event {
                        AgentEvent::ToolExecutionStart {
                            tool_call_id,
                            tool_name,
                            args,
                        } => {
                            let _ = app.emit(
                                "tool-event",
                                ToolEvent::Start {
                                    session_id: session_id.clone(),
                                    turn_id,
                                    id: tool_call_id.clone(),
                                    name: tool_name.clone(),
                                    args: args.clone(),
                                },
                            );
                        }
                        AgentEvent::ToolExecutionEnd {
                            tool_call_id,
                            tool_name,
                            result,
                        } => {
                            let output = result
                                .content
                                .iter()
                                .filter_map(|b| {
                                    if let rozsa_model::types::ContentBlock::Text { text, .. } = b {
                                        Some(text.as_str())
                                    } else {
                                        None
                                    }
                                })
                                .collect::<Vec<_>>()
                                .join("\n");
                            let _ = app.emit(
                                "tool-event",
                                ToolEvent::End {
                                    session_id: session_id.clone(),
                                    turn_id,
                                    id: tool_call_id.clone(),
                                    name: tool_name.clone(),
                                    success: !result.is_error,
                                    output,
                                    details: result.details.clone(),
                                },
                            );
                        }
                        _ => {}
                    }

                    // Only emit a snapshot when this immutable session is active.
                    if changed {
                        if throttled_update
                            && last_stream_emit.elapsed() < Duration::from_millis(33)
                        {
                            continue;
                        }
                        if throttled_update {
                            last_stream_emit = Instant::now();
                        }
                        let current_active = *active_tab.lock().await;
                        let tabs_guard = tabs.lock().await;
                        if tabs_guard
                            .get(current_active)
                            .is_some_and(|tab| tab.session_id() == session_id)
                        {
                            if let Some(tab) = tabs_guard.get(current_active) {
                                let snapshot = if stream_update {
                                    UiSnapshot::from_stream_update(tab, &shared)
                                } else {
                                    UiSnapshot::from_tab(tab, &shared)
                                };
                                let _ = app.emit("ui-state", &snapshot);
                            }
                        }
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(
                        "GUI event forwarder for session {session_id} lagged by {n} events"
                    );
                }
            }
        }
    });
}

/// 权限请求监听
pub fn spawn_permission_listener(
    app: AppHandle,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<PermissionRequest>,
) {
    tokio::spawn(async move {
        while let Some(request) = rx.recv().await {
            let context_key =
                crate::state::permission_pending_key(&request.session_id, &request.request_id);
            app.state::<GuiState>().pending_permission_contexts.insert(
                context_key,
                crate::state::PendingPermissionContext {
                    tool_name: request.tool_name.clone(),
                    args: request.args.clone(),
                    info: request.info.clone(),
                },
            );
            let event = PermissionEvent {
                session_id: request.session_id,
                turn_id: request.turn_id,
                request_id: request.request_id,
                tool: request.info.tool_name.clone(),
                description: request.description,
                summary: request.info.args_summary.clone(),
                risk: format!("{:?}", request.info.risk),
                trust_key: request.info.trust_key.clone(),
                trust_levels: request.info.trust_levels.clone(),
                trust_groups: request.info.trust_groups.clone(),
            };
            let _ = app.emit("permission-request", &event);
        }
    });
}
