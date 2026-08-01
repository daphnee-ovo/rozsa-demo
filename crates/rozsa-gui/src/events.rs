// FrameworkTree
// events.rs
// ├── struct ThemeStateSnapshot
// ├── emit_to_webview()
// ├── emit_main()
// ├── emit_sidebar()
// ├── emit_both()
// ├── native_theme_variant()
// ├── apply_native_theme_surface()
// ├── emit_theme_state()
// ├── emit_sidebar_state()
// ├── emit_gui_scene_snapshot()
// ├── spawn_event_forwarder_for_session()
// ├── spawn_dev_flow_presentation_capture()
// ├── spawn_permission_listener()
// └── spawn_user_question_listener()

// File: events.rs
//
// 事件转发：每个 Active session tab 有独立的事件监听任务。

use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use rozsa_app::themes::ThemeDefinition;
use rozsa_app::tools::{AskUserQuestionRequest, AskUserQuestionResponse};
use rozsa_core::events::AgentEvent;

use crate::commands::AppearanceSnapshot;
use crate::state::{
    GuiState, PendingUserQuestion, PermissionEvent, PermissionRequest, SessionTab, ToolEvent,
    UiSnapshot, UserQuestionEvent, find_tab_index_by_session, user_question_pending_key,
};

pub const MAIN_WEBVIEW: &str = "main";
pub const SIDEBAR_WEBVIEW: &str = "sidebar";
static THEME_REVISION: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeStateSnapshot {
    pub revision: u64,
    pub theme_mode: String,
    pub font_size: u8,
    pub translucent_sidebar: bool,
    pub light_theme: ThemeDefinition,
    pub dark_theme: ThemeDefinition,
    pub is_macos: bool,
}

pub fn emit_to_webview<R: Runtime, S: Serialize + Clone>(
    app: &AppHandle<R>,
    webview: &str,
    event: &str,
    payload: S,
) -> Result<(), String> {
    app.emit_to(webview, event, payload)
        .map_err(|error| format!("Failed to emit {event} to {webview}: {error}"))
}

pub fn emit_main<R: Runtime, S: Serialize + Clone>(
    app: &AppHandle<R>,
    event: &str,
    payload: S,
) -> Result<(), String> {
    emit_to_webview(app, MAIN_WEBVIEW, event, payload)
}

pub fn emit_sidebar<R: Runtime, S: Serialize + Clone>(
    app: &AppHandle<R>,
    event: &str,
    payload: S,
) -> Result<(), String> {
    emit_to_webview(app, SIDEBAR_WEBVIEW, event, payload)
}

pub fn emit_both<R: Runtime, S: Serialize + Clone>(
    app: &AppHandle<R>,
    event: &str,
    payload: S,
) -> Result<(), String> {
    emit_main(app, event, payload.clone())?;
    emit_sidebar(app, event, payload)
}

#[cfg(target_os = "macos")]
fn native_theme_variant(theme: &ThemeDefinition) -> crate::native_split_view::NativeThemeVariant {
    crate::native_split_view::NativeThemeVariant {
        opaque_color: theme
            .variables
            .get("--sidebar-bg")
            .cloned()
            .unwrap_or_else(|| theme.background.clone()),
    }
}

#[cfg(target_os = "macos")]
fn apply_native_theme_surface<R: Runtime>(
    app: &AppHandle<R>,
    revision: u64,
    snapshot: &ThemeStateSnapshot,
) -> Result<(), String> {
    let surface = crate::native_split_view::NativeThemeSurface {
        theme_mode: snapshot.theme_mode.clone(),
        translucent_sidebar: snapshot.translucent_sidebar,
        light: native_theme_variant(&snapshot.light_theme),
        dark: native_theme_variant(&snapshot.dark_theme),
    };
    if objc2::MainThreadMarker::new().is_some() {
        return crate::native_split_view::apply_theme_surface(revision, surface);
    }

    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    app.run_on_main_thread(move || {
        let result = crate::native_split_view::apply_theme_surface(revision, surface);
        let _ = sender.send(result);
    })
    .map_err(|error| format!("failed to schedule native theme surface: {error}"))?;
    receiver
        .recv()
        .map_err(|error| format!("native theme surface response was dropped: {error}"))?
}

pub fn emit_theme_state<R: Runtime>(
    app: &AppHandle<R>,
    appearance: &AppearanceSnapshot,
    light_theme: ThemeDefinition,
    dark_theme: ThemeDefinition,
) -> Result<ThemeStateSnapshot, String> {
    let revision = THEME_REVISION.fetch_add(1, Ordering::SeqCst) + 1;
    let snapshot = ThemeStateSnapshot {
        revision,
        theme_mode: appearance.theme_mode.clone(),
        font_size: appearance.font_size,
        translucent_sidebar: appearance.translucent_sidebar,
        light_theme,
        dark_theme,
        is_macos: appearance.is_macos,
    };
    #[cfg(target_os = "macos")]
    apply_native_theme_surface(app, revision, &snapshot)?;
    emit_both(app, "theme-state", snapshot.clone())?;
    Ok(snapshot)
}

pub async fn emit_sidebar_state<R: Runtime>(
    app: &AppHandle<R>,
    state: &GuiState,
) -> Result<(), String> {
    let snapshot = state.sidebar_snapshot().await?;
    emit_sidebar(app, "sidebar-state", snapshot)
}

pub fn emit_gui_scene_snapshot<R: Runtime>(
    app: &AppHandle<R>,
    targets: &[crate::scene_router::GuiWebview],
    snapshot: &crate::scene_router::GuiSceneSnapshot,
) -> Result<(), String> {
    for target in targets {
        app.emit_to(
            target.label(),
            crate::scene_router::GUI_SCENE_SNAPSHOT_EVENT,
            snapshot,
        )
        .map_err(|error| {
            format!(
                "Failed to emit GUI scene revision {} to {}: {error}",
                snapshot.revision,
                target.label()
            )
        })?;
    }
    Ok(())
}

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
                            let _ = emit_main(
                                &app,
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
                            let _ = emit_main(
                                &app,
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

                    // Dev-flow activity wiring: the session is active while the
                    // agent runs, finishes at AgentEnd, and successful Bash
                    // completion rescans the whole worktree.
                    match &event {
                        AgentEvent::AgentStart => {
                            let cwd = {
                                let tabs_guard = tabs.lock().await;
                                match find_tab_index_by_session(&tabs_guard, &session_id)
                                    .and_then(|index| tabs_guard.get(index))
                                {
                                    Some(SessionTab::Active { agent, .. }) => {
                                        Some(agent.current_cwd().await)
                                    }
                                    _ => None,
                                }
                            };
                            if let Some(cwd) = cwd {
                                gui_state.dev_flow.session_started(&session_id, cwd).await;
                            }
                        }
                        AgentEvent::AgentEnd { .. } => {
                            gui_state
                                .dev_flow
                                .session_finished(&session_id, std::time::SystemTime::now())
                                .await;
                        }
                        AgentEvent::ToolExecutionEnd {
                            tool_call_id,
                            tool_name,
                            result,
                        } if tool_name == "bash" && !result.is_error => {
                            let cwd = {
                                let tabs_guard = tabs.lock().await;
                                match find_tab_index_by_session(&tabs_guard, &session_id)
                                    .and_then(|index| tabs_guard.get(index))
                                {
                                    Some(SessionTab::Active { agent, .. }) => {
                                        Some(agent.current_cwd().await)
                                    }
                                    _ => None,
                                }
                            };
                            if let Some(cwd) = cwd {
                                gui_state
                                    .dev_flow
                                    .on_successful_bash(&session_id, cwd)
                                    .await;
                            }
                            spawn_dev_flow_presentation_capture(
                                app.clone(),
                                gui_state.clone(),
                                session_id.clone(),
                                tool_call_id.clone(),
                                result.clone(),
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
                                let _ = emit_main(&app, "ui-state", &snapshot);
                            }
                        }
                    }
                    if matches!(
                        event,
                        AgentEvent::AgentStart
                            | AgentEvent::AgentEnd { .. }
                            | AgentEvent::ToolExecutionEnd { .. }
                    ) {
                        let _ = emit_sidebar_state(&app, &gui_state).await;
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

fn spawn_dev_flow_presentation_capture<R: Runtime>(
    app: AppHandle<R>,
    state: GuiState,
    session_id: String,
    tool_call_id: String,
    result: rozsa_model::types::ToolResultMessage,
) {
    tokio::spawn(async move {
        let Some(record) = state
            .dev_flow
            .capture_tool_presentation(&session_id, &tool_call_id, &result)
            .await
        else {
            return;
        };
        let presentation = record.presentation.clone();
        let agent = {
            let tabs = state.tabs.lock().await;
            find_tab_index_by_session(&tabs, &session_id)
                .and_then(|index| tabs.get(index))
                .and_then(|tab| match tab {
                    SessionTab::Active { agent, .. } => Some(agent.clone()),
                    _ => None,
                })
        };
        let Some(agent) = agent else {
            return;
        };
        if let Err(error) = agent
            .session_manager()
            .await
            .append_dev_flow_presentation(&record)
        {
            tracing::warn!(%error, %tool_call_id, "failed to persist Dev-flow presentation");
            return;
        }
        let active_snapshot = {
            let active = *state.active_tab.lock().await;
            let mut tabs = state.tabs.lock().await;
            let Some(index) = find_tab_index_by_session(&tabs, &session_id) else {
                return;
            };
            let Some(SessionTab::Active { live, .. }) = tabs.get_mut(index) else {
                return;
            };
            live.dev_flow_presentations
                .insert(tool_call_id, presentation);
            (index == active).then(|| UiSnapshot::from_tab(&tabs[index], &state.shared))
        };
        if let Some(snapshot) = active_snapshot {
            let _ = emit_main(&app, "ui-state", snapshot);
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
            let _ = emit_main(&app, "permission-request", &event);
            let state = app.state::<GuiState>();
            let _ = emit_sidebar_state(&app, state.inner()).await;
        }
    });
}

/// Listen for app-runtime askUserQuestion requests, retain the response
/// channel in GUI state, and publish only serializable question data to main.
pub fn spawn_user_question_listener(
    app: AppHandle,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<AskUserQuestionRequest>,
) {
    tokio::spawn(async move {
        while let Some(request) = rx.recv().await {
            let key = user_question_pending_key(&request.session_id, &request.request_id);
            let state = app.state::<GuiState>();
            if let Some(previous) = state.pending_user_questions.insert(
                key.clone(),
                PendingUserQuestion {
                    questions: request.questions.clone(),
                    response_tx: request.response_tx,
                },
            ) {
                let _ = previous
                    .response_tx
                    .send(AskUserQuestionResponse::Cancelled);
            }

            let event = UserQuestionEvent {
                session_id: request.session_id,
                request_id: request.request_id,
                questions: request.questions,
            };
            if let Err(error) = emit_main(&app, "question-request", &event) {
                eprintln!("[rozsa-gui][question] failed to emit question request: {error}");
                if let Some((_, pending)) = state.pending_user_questions.remove(&key) {
                    let _ = pending.response_tx.send(AskUserQuestionResponse::Cancelled);
                }
            }
        }
    });
}
