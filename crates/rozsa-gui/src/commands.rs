// File: commands.rs
//
// Tauri IPC 命令。多会话架构：操作都针对当前活跃 tab。

use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use rozsa_app::agent_session::AgentSession;
use rozsa_app::permissions::PermissionResponse;
use rozsa_app::session::manager::SessionManager;
use rozsa_app::skills::SkillRegistry;
use rozsa_app::themes::{self, ThemeDefinition, ThemeMode, ThemeStore, ThemeSummary};
use rozsa_core::messages::AgentMessage;
use rozsa_model::types::{
    AssistantMessage, ContentBlock, Message, StopReason, ThinkingLevel, Usage,
};

use crate::state::{
    GuiState, SessionTab, UiSnapshot, deny_pending_approvals, find_tab_index_by_session,
    permission_pending_key, session_id_from_path,
};
use crate::turn_diff::{INTERACTION_STARTED, INTERACTION_SUMMARY};

#[tauri::command]
pub async fn set_gui_scene(
    state: State<'_, GuiState>,
    app: AppHandle,
    invoking_webview: tauri::Webview,
    scene: crate::scene_router::GuiScene,
    selected_pane: Option<crate::scene_router::SettingsPane>,
    expected_revision: u64,
) -> Result<crate::scene_router::GuiSceneSnapshot, String> {
    let requester = crate::scene_router::GuiWebview::from_label(invoking_webview.label())?;
    let update =
        state
            .scene_router
            .lock()
            .await
            .set_scene(scene, selected_pane, expected_revision)?;
    let targets = if update.stale {
        vec![requester]
    } else {
        update.ready_webviews
    };
    crate::events::emit_gui_scene_snapshot(&app, &targets, &update.snapshot)?;
    Ok(update.snapshot)
}

#[tauri::command]
pub async fn gui_webview_ready(
    state: State<'_, GuiState>,
    app: AppHandle,
    invoking_webview: tauri::Webview,
    webview: crate::scene_router::GuiWebview,
    last_revision: u64,
) -> Result<crate::scene_router::GuiSceneSnapshot, String> {
    let requester = crate::scene_router::GuiWebview::from_label(invoking_webview.label())?;
    if requester != webview {
        return Err(format!(
            "GUI WebView payload mismatch: caller is {}, payload is {}",
            requester.label(),
            webview.label()
        ));
    }
    let update = state
        .scene_router
        .lock()
        .await
        .webview_ready(webview, last_revision);
    if update.should_emit {
        crate::events::emit_gui_scene_snapshot(&app, &[webview], &update.snapshot)?;
    }
    if webview == crate::scene_router::GuiWebview::Sidebar {
        crate::events::emit_sidebar_state(&app, state.inner()).await?;
    }
    if update.all_webviews_ready {
        #[cfg(target_os = "macos")]
        reveal_native_split(&app)?;
        app.get_webview_window("main")
            .ok_or_else(|| "main GUI window is unavailable".to_owned())?
            .show()
            .map_err(|error| format!("failed to show ready GUI window: {error}"))?;
    }
    Ok(update.snapshot)
}

#[cfg(target_os = "macos")]
fn reveal_native_split(app: &AppHandle) -> Result<(), String> {
    if objc2::MainThreadMarker::new().is_some() {
        return crate::native_split_view::reveal_content();
    }

    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    app.run_on_main_thread(move || {
        let _ = sender.send(crate::native_split_view::reveal_content());
    })
    .map_err(|error| format!("failed to schedule native split reveal: {error}"))?;
    receiver
        .recv()
        .map_err(|error| format!("native split reveal response was dropped: {error}"))?
}

// --- 对话 ---

#[tauri::command]
pub async fn send_message(
    state: State<'_, GuiState>,
    app: AppHandle,
    message: String,
) -> Result<(), String> {
    let idx = *state.active_tab.lock().await;
    let mut tabs = state.tabs.lock().await;

    // 确保当前 tab 是 Active 状态（懒加载：首次发消息时激活）
    let tab = tabs.get_mut(idx).ok_or("No active tab")?;
    let session_id = tab.session_id();
    let (agent, spawn_forwarder) = match tab {
        SessionTab::Active { agent, .. } => (agent.clone(), false),
        SessionTab::Loaded { path, messages } => {
            // 升级为 Active：创建 AgentSession
            let agent = activate_session(path, &state).await?;
            let path_owned = path.clone();
            let completed_summary = load_session_summary(&path_owned);
            let agent_arc = std::sync::Arc::new(agent);
            *tab = SessionTab::Active {
                path: path_owned,
                agent: agent_arc.clone(),
                live: crate::state::LiveState {
                    messages: std::mem::take(messages),
                    completed_summary,
                    ..Default::default()
                },
            };
            (agent_arc, true)
        }
        SessionTab::Idle { path, .. } => {
            // 从 Idle 直接激活（加载历史 + 创建 agent）
            let path_owned = path.clone();
            let messages = load_session_messages(&path_owned)?;
            let agent = activate_session(&path_owned, &state).await?;
            let completed_summary = load_session_summary(&path_owned);
            let agent_arc = std::sync::Arc::new(agent);
            *tab = SessionTab::Active {
                path: path_owned,
                agent: agent_arc.clone(),
                live: crate::state::LiveState {
                    messages,
                    completed_summary,
                    ..Default::default()
                },
            };
            (agent_arc, true)
        }
    };
    drop(tabs);
    if spawn_forwarder {
        crate::events::spawn_event_forwarder_for_session(
            app.clone(),
            session_id.clone(),
            state.inner().clone(),
        );
    }

    let expansion = crate::file_refs::expand_file_references(&message, &state.shared.cwd);
    for notice in &expansion.notices {
        let _ = crate::events::emit_main(
            &app,
            "notification",
            format!("Skipped @{}: {}.", notice.path, notice.reason),
        );
    }

    {
        let mut manager = agent.session_manager().await;
        manager
            .append_custom(INTERACTION_STARTED.to_string(), None)
            .map_err(|error| error.to_string())?;
    }
    {
        let mut tabs = state.tabs.lock().await;
        if let Some(SessionTab::Active { live, .. }) = tabs.get_mut(idx) {
            live.begin_interaction();
        }
    }

    // 发送消息（后台执行，不阻塞 IPC 返回）
    let shared = state.shared.clone();
    let tabs_ref = state.tabs.clone();
    let active_tab_ref = state.active_tab.clone();
    let pending_approvals = state.pending_approvals.clone();
    let gui_state = state.inner().clone();
    tokio::spawn(async move {
        let prompt_succeeded = if let Err(e) = agent
            .prompt_with_prefix_blocks(&message, expansion.blocks, expansion.display_text)
            .await
        {
            append_prompt_error(idx, &agent, &tabs_ref, &shared, &app, e.to_string()).await;
            false
        } else {
            true
        };
        if prompt_succeeded {
            spawn_session_name_generation(
                gui_state.clone(),
                app.clone(),
                agent.clone(),
                session_id.clone(),
                message.clone(),
            );
        }
        if let Some(approvals) = &pending_approvals {
            deny_pending_approvals(approvals, Some(&session_id));
        }
        advance_interaction(gui_state, app.clone(), session_id.clone());
        // 完成后推送最终状态
        let current_idx = *active_tab_ref.lock().await;
        if current_idx == idx {
            let tabs = tabs_ref.lock().await;
            if let Some(tab) = tabs.get(idx) {
                let snapshot = UiSnapshot::from_tab(tab, &shared);
                let _ = crate::events::emit_main(&app, "ui-state", &snapshot);
            }
        }
    });

    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlashCommandResult {
    pub handled: bool,
    pub action: Option<String>,
    pub value: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutocompleteResponse {
    pub prefix: String,
    pub items: Vec<crate::file_refs::AutocompleteItem>,
    pub valid_match: bool,
    pub highlight_ranges: Vec<InputHighlightRange>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputHighlightRange {
    pub start: usize,
    pub end: usize,
}

#[tauri::command]
pub async fn autocomplete_input(
    state: State<'_, GuiState>,
    text: String,
    cursor: usize,
) -> Result<AutocompleteResponse, String> {
    let cursor = utf16_offset_to_byte_index(&text, cursor);
    let head = &text[..cursor];
    let active_agent = active_agent(&state).await.ok();
    let skill_commands = collect_skill_slash_commands(active_agent.as_deref(), &state.shared.cwd);
    let highlight_ranges = input_highlight_ranges(&text, &state.shared.cwd, &skill_commands);

    if let Some(prefix) = parse_slash_completion_prefix(head) {
        use rozsa_app::slash_commands::{
            AutocompleteEngine, BUILTIN_SLASH_COMMANDS, SlashCommandInfo, SlashCommandSource,
        };

        let prefix_lower = prefix.to_ascii_lowercase();
        let mut dynamic = Vec::new();

        for skill in &skill_commands {
            let builtin_conflict = BUILTIN_SLASH_COMMANDS
                .iter()
                .any(|cmd| cmd.name == skill.name);
            let name = if builtin_conflict {
                format!("skill:{}", skill.name)
            } else {
                skill.name.clone()
            };
            dynamic.push(SlashCommandInfo {
                name,
                description: Some(skill.description.clone()),
                source: SlashCommandSource::Skill,
            });
        }

        let completion_text = format!("/{prefix}");
        let completion_cursor = completion_text.len();
        let items = AutocompleteEngine::with_dynamic(dynamic)
            .complete(&completion_text, completion_cursor)
            .unwrap_or_default()
            .into_iter()
            .map(|item| crate::file_refs::AutocompleteItem {
                value: format!("/{} ", item.value),
                label: item.label,
                description: item.description,
            })
            .collect::<Vec<_>>();
        let valid_match = items
            .iter()
            .any(|item| item.label.trim_start_matches('/').to_ascii_lowercase() == prefix_lower);
        return Ok(AutocompleteResponse {
            prefix: format!("/{prefix}"),
            items,
            valid_match,
            highlight_ranges,
        });
    }

    if let Some(prefix) = parse_at_completion_prefix(head) {
        let items = crate::file_refs::complete_file_reference(&prefix, &state.shared.cwd);
        let path = prefix
            .strip_prefix("@\"")
            .or_else(|| prefix.strip_prefix('@'))
            .unwrap_or(&prefix)
            .trim_end_matches('"');
        let valid_match = resolved_autocomplete_path(path, &state.shared.cwd)
            .as_ref()
            .is_some_and(|path| path.exists());
        return Ok(AutocompleteResponse {
            prefix,
            items,
            valid_match,
            highlight_ranges,
        });
    }

    Ok(AutocompleteResponse {
        prefix: String::new(),
        items: Vec::new(),
        valid_match: false,
        highlight_ranges,
    })
}

#[tauri::command]
pub async fn pick_attachment(app: AppHandle, mode: String) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let path = match AttachmentPickMode::parse(&mode)? {
        AttachmentPickMode::Directory => app.dialog().file().blocking_pick_folder(),
        AttachmentPickMode::File => app.dialog().file().blocking_pick_file(),
        AttachmentPickMode::Any => return pick_any_attachment(app).await,
    };
    Ok(path.map(|path| path.to_string()))
}

async fn pick_any_attachment(app: AppHandle) -> Result<Option<String>, String> {
    #[cfg(target_os = "macos")]
    {
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        app.run_on_main_thread(move || {
            let _ = sender.send(pick_any_attachment_macos());
        })
        .map_err(|error| format!("Could not open the native attachment picker: {error}"))?;

        return tokio::task::spawn_blocking(move || {
            receiver
                .recv()
                .map_err(|_| "Native attachment picker did not return a result".to_owned())?
        })
        .await
        .map_err(|error| format!("Native attachment picker task failed: {error}"))?;
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Err("A combined file-and-directory picker is only available on macOS".to_owned())
    }
}

#[cfg(target_os = "macos")]
fn pick_any_attachment_macos() -> Result<Option<String>, String> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSModalResponseOK, NSOpenPanel};

    let marker = MainThreadMarker::new()
        .ok_or_else(|| "Native attachment picker must run on the macOS main thread".to_owned())?;
    let panel = NSOpenPanel::openPanel(marker);
    panel.setCanChooseFiles(true);
    panel.setCanChooseDirectories(true);
    panel.setAllowsMultipleSelection(false);

    if panel.runModal() != NSModalResponseOK {
        return Ok(None);
    }

    panel
        .URL()
        .and_then(|url| url.path())
        .map(|path| path.to_string())
        .map(Some)
        .ok_or_else(|| "Native attachment picker did not return a filesystem path".to_owned())
}

#[tauri::command]
pub async fn dispatch_slash_command(
    state: State<'_, GuiState>,
    app: AppHandle,
    text: String,
) -> Result<SlashCommandResult, String> {
    let active_agent_for_match = active_agent(&state).await.ok();
    let skill_commands =
        collect_skill_slash_commands(active_agent_for_match.as_deref(), &state.shared.cwd);
    let Some((cmd, args)) = first_builtin_slash_command(&text)
        .or_else(|| first_skill_slash_command(&text, &skill_commands))
    else {
        return Ok(SlashCommandResult {
            handled: false,
            action: None,
            value: None,
        });
    };

    match cmd.as_str() {
        "model" => {
            if args.is_empty() {
                return slash_action("modelPicker");
            }
            switch_model_reference(&state, &args).await?;
            emit_info(&app, &format!("Model: {args}"));
        }
        "settings" => return slash_action("settings"),
        "help" => return slash_action_arg("help", args),
        "hotkeys" => return slash_action("hotkeys"),
        "clear" | "new" => {
            create_new_session(&state, &app).await?;
            emit_info(&app, "Started new session");
            return slash_action("refreshSessions");
        }
        "compact" => {
            compact_active_session(&state).await?;
            emit_info(&app, "Compaction started");
        }
        "thinking" => {
            let level = parse_thinking_level(&args)?;
            set_thinking_level(&state, level).await;
            emit_info(
                &app,
                &format!("Thinking: {}", format!("{level:?}").to_lowercase()),
            );
        }
        "login" => {
            let message = auth_login(app.clone()).await?;
            emit_info(&app, &message);
            return slash_action("refreshModels");
        }
        "logout" => {
            let message = auth_logout().await?;
            emit_info(&app, &message);
            return slash_action("refreshModels");
        }
        "usage" => {
            let snapshot = refresh_rate_limits(&state, &app).await?;
            emit_info(
                &app,
                &rozsa_app::rate_limit::format_rate_limit_display(&snapshot),
            );
        }
        "session" => {
            let (path, count) = active_session_summary(&state).await?;
            emit_info(&app, &format!("Session\nFile: {path}\nMessages: {count}"));
        }
        "name" => {
            let agent = active_agent(&state).await?;
            if args.is_empty() {
                let name = agent
                    .session_manager()
                    .await
                    .current_name()
                    .unwrap_or_else(|| "(unnamed)".to_string());
                emit_info(&app, &format!("Session name: {name}"));
            } else {
                let session_id = {
                    let mut manager = agent.session_manager().await;
                    manager
                        .append_session_info(Some(args.clone()))
                        .map_err(|e| e.to_string())?;
                    manager.session_id().to_string()
                };
                emit_info(&app, &format!("Session name set: {args}"));
                emit_session_views(state.inner(), &app, &session_id).await?;
                return slash_action("refreshSessions");
            }
        }
        "permissions" => {
            let mode = &state.shared.settings_manager.resolved().permissions.mode;
            emit_info(&app, &format!("Permission mode: {mode}"));
        }
        "scoped-models" => {
            let registry = state
                .model_registry
                .as_ref()
                .ok_or("No model registry available")?;
            let lines = registry
                .all()
                .iter()
                .map(|m| format!("[{}] {}", m.provider, m.id))
                .collect::<Vec<_>>();
            emit_info(
                &app,
                &format!("Available models ({}):\n{}", lines.len(), lines.join("\n")),
            );
        }
        "copy" => {
            let text = last_assistant_text(&state).await?;
            if text.is_empty() {
                emit_info(&app, "No assistant message to copy");
            } else {
                return Ok(SlashCommandResult {
                    handled: true,
                    action: Some("copy".to_string()),
                    value: Some(text),
                });
            }
        }
        "search" => {
            if args.is_empty() {
                emit_info(&app, "Usage: /search <pattern>");
            } else {
                let results = search_messages(&state, &args).await;
                emit_info(&app, &results);
            }
        }
        "export" => {
            let path = if args.is_empty() {
                "session-export.jsonl".to_string()
            } else {
                args.clone()
            };
            export_active_session(&state, &path).await?;
            emit_info(&app, &format!("Exported current session to {path}"));
        }
        "resume" => return slash_action("refreshSessions"),
        "lsp" => {
            if args.is_empty() {
                let current = state.runtime_settings.lock().await.lsp_mode.clone();
                emit_info(
                    &app,
                    &format!(
                        "LSP auto-diagnostics mode: {current}\nOptions: agent_end | edit_write | disabled"
                    ),
                );
            } else if matches!(args.as_str(), "agent_end" | "edit_write" | "disabled") {
                state.runtime_settings.lock().await.lsp_mode = args.clone();
                persist_settings(&state).await;
                emit_info(&app, &format!("LSP mode set to: {args}"));
            } else {
                emit_info(
                    &app,
                    &format!(
                        "Unknown LSP mode '{args}'. Options: agent_end | edit_write | disabled"
                    ),
                );
            }
        }
        "main" => {
            let agent = active_agent(&state).await?;
            agent.set_viewing_subagent(None).await;
            emit_info(&app, "Switched to main agent");
        }
        "subagent" | "subagents" => return slash_action("subagentPanel"),
        "tree" => {
            emit_info(&app, &session_tree_summary(&state).await?);
        }
        "graph" => {
            emit_info(&app, &conversation_graph_summary(&state).await?);
        }
        "fork" => return slash_action("forkPicker"),
        "clone" => {
            let message = clone_active_session(&state).await?;
            emit_info(&app, &message);
            return slash_action("refreshSessions");
        }
        "import" => {
            let path = if args.is_empty() {
                "session-export.jsonl"
            } else {
                args.as_str()
            };
            emit_info(&app, &import_session_summary(path));
        }
        "share" => {
            let message = share_active_session(&state).await?;
            emit_info(&app, &message);
        }
        "reload" => {
            let agent = active_agent(&state).await?;
            let diagnostics = agent.reload_skills();
            for diagnostic in &diagnostics {
                emit_info(
                    &app,
                    &format!(
                        "Skill load warning: {} - {}",
                        diagnostic.path.display(),
                        diagnostic.message
                    ),
                );
            }
            let count = agent.skill_registry().list().len();
            emit_info(&app, &format!("Reloaded skills ({count} loaded)"));
        }
        "changelog" => {
            emit_info(&app, "No changelog entries available in GUI mode");
        }
        "gc" => {
            let days = args.parse::<u64>().unwrap_or(30);
            let message = gc_old_sessions(&state, days).await?;
            emit_info(&app, &message);
        }
        "quit" => app.exit(0),
        _ => {
            if let Some(prompt) = normalize_skill_commands_in_text(&text, &skill_commands) {
                send_message(state, app, prompt).await?;
            } else {
                return Ok(SlashCommandResult {
                    handled: false,
                    action: None,
                    value: None,
                });
            }
        }
    }

    Ok(SlashCommandResult {
        handled: true,
        action: None,
        value: None,
    })
}

#[tauri::command]
pub async fn abort(state: State<'_, GuiState>) -> Result<(), String> {
    let idx = *state.active_tab.lock().await;
    let tabs = state.tabs.lock().await;
    let active = tabs.get(idx).and_then(|tab| match tab {
        SessionTab::Active { agent, .. } => Some((tab.session_id(), agent.clone())),
        _ => None,
    });
    drop(tabs);
    if let Some((session_id, agent)) = active {
        if let Some(approvals) = &state.pending_approvals {
            deny_pending_approvals(approvals, Some(&session_id));
        }
        agent.abort().await;
    }
    Ok(())
}

#[tauri::command]
pub async fn send_running_message(
    state: State<'_, GuiState>,
    app: AppHandle,
    mode: String,
    message: String,
) -> Result<(), String> {
    let message = message.trim();
    if message.is_empty() {
        return Ok(());
    }
    let idx = *state.active_tab.lock().await;
    let mut tabs = state.tabs.lock().await;
    let Some(SessionTab::Active { agent, live, .. }) = tabs.get_mut(idx) else {
        return Err("No active agent".to_string());
    };
    if !live.is_streaming {
        return Err("Agent is not running".to_string());
    }
    match mode.as_str() {
        "queue" => live.enqueue_message(message.to_string()),
        "steer" => {
            agent.steer(message);
            live.add_steering_message(message.to_string());
        }
        _ => return Err(format!("Unknown running send mode: {mode}")),
    }
    let snapshot = UiSnapshot::from_tab(&tabs[idx], &state.shared);
    let _ = crate::events::emit_main(&app, "ui-state", &snapshot);
    Ok(())
}

/// Start only the first queued message after the prior prompt has returned.
/// AgentSession deliberately rejects concurrent prompts, so this function is
/// called from each prompt completion rather than the AgentEnd event itself.
fn advance_interaction(gui_state: GuiState, app: AppHandle, session_id: String) {
    tokio::spawn(async move {
        let next = {
            let mut tabs = gui_state.tabs.lock().await;
            let Some(index) = find_tab_index_by_session(&tabs, &session_id) else {
                return;
            };
            let Some(SessionTab::Active { agent, live, .. }) = tabs.get_mut(index) else {
                return;
            };
            live.take_next_queued_message()
                .map(|message| (agent.clone(), message))
        };

        let Some((agent, message)) = next else {
            finish_interaction(&gui_state, &app, &session_id).await;
            return;
        };

        if let Err(error) = agent.prompt(&message).await {
            append_prompt_error_for_session(
                &session_id,
                &agent,
                &gui_state.tabs,
                &gui_state.shared,
                &app,
                error.to_string(),
            )
            .await;
        }
        if let Some(approvals) = &gui_state.pending_approvals {
            deny_pending_approvals(approvals, Some(&session_id));
        }
        advance_interaction(gui_state, app, session_id);
    });
}

async fn finish_interaction(gui_state: &GuiState, app: &AppHandle, session_id: &str) {
    let agent = {
        let tabs = gui_state.tabs.lock().await;
        let Some(index) = find_tab_index_by_session(&tabs, session_id) else {
            return;
        };
        let Some(SessionTab::Active { agent, live, .. }) = tabs.get(index) else {
            return;
        };
        if !live.interaction_active {
            return;
        }
        agent.clone()
    };

    let summary = {
        let mut manager = agent.session_manager().await;
        let summary = crate::turn_diff::persisted_interaction_activity(&manager);
        let payload = serde_json::to_value(&summary).ok();
        if let Err(error) = manager.append_custom(INTERACTION_SUMMARY.to_string(), payload) {
            let _ = crate::events::emit_main(
                app,
                "notification",
                format!("Failed to persist task summary: {error}"),
            );
        }
        summary
    };

    {
        let mut tabs = gui_state.tabs.lock().await;
        let Some(index) = find_tab_index_by_session(&tabs, session_id) else {
            return;
        };
        if let Some(SessionTab::Active { live, .. }) = tabs.get_mut(index) {
            live.finish_interaction(summary);
        }
    }

    if let Err(error) = emit_session_views(gui_state, app, session_id).await {
        eprintln!("[rozsa-gui][session] failed to refresh completed interaction: {error}");
    }
}

fn spawn_session_name_generation(
    gui_state: GuiState,
    app: AppHandle,
    agent: Arc<AgentSession>,
    session_id: String,
    first_user_message: String,
) {
    tokio::spawn(async move {
        if !gui_state.runtime_settings.lock().await.auto_session_naming {
            return;
        }
        match agent.generate_session_name(&first_user_message).await {
            Ok(Some(_)) => {
                if let Err(error) = emit_session_views(&gui_state, &app, &session_id).await {
                    eprintln!("[rozsa-gui][session-name] failed to refresh views: {error}");
                }
            }
            Ok(None) => {}
            Err(error) => {
                // Naming is auxiliary: the deterministic first-message preview
                // remains visible and the conversation must continue normally.
                eprintln!("[rozsa-gui][session-name] generation failed: {error}");
            }
        }
    });
}

async fn emit_session_views(
    gui_state: &GuiState,
    app: &AppHandle,
    session_id: &str,
) -> Result<(), String> {
    let active_index = *gui_state.active_tab.lock().await;
    let snapshot = {
        let tabs = gui_state.tabs.lock().await;
        tabs.get(active_index)
            .filter(|tab| tab.session_id() == session_id)
            .map(|tab| UiSnapshot::from_tab(tab, &gui_state.shared))
    };
    if let Some(snapshot) = snapshot {
        crate::events::emit_main(app, "ui-state", &snapshot)?;
    }
    crate::events::emit_sidebar_state(app, gui_state).await
}

// --- 状态查询 ---

#[tauri::command]
pub async fn get_state(state: State<'_, GuiState>) -> Result<UiSnapshot, String> {
    let idx = *state.active_tab.lock().await;
    let tabs = state.tabs.lock().await;
    let tab = tabs.get(idx).ok_or("No active tab")?;
    Ok(UiSnapshot::from_tab(tab, &state.shared))
}

#[tauri::command]
pub async fn get_subagents(
    state: State<'_, GuiState>,
) -> Result<Vec<rozsa_app::subagent::SubagentInfo>, String> {
    let agent = active_agent(&state).await?;
    let manager = agent.subagent_manager().await;
    Ok(manager.list().await)
}

#[tauri::command]
pub async fn get_file_diff(
    state: State<'_, GuiState>,
    path: String,
) -> Result<crate::git_diff::FileDiff, String> {
    crate::git_diff::read_workspace_diff(&state.shared.cwd, &path)
}

// --- 会话管理 ---

#[tauri::command]
pub async fn get_sessions(state: State<'_, GuiState>) -> Result<Vec<SessionListEntry>, String> {
    let session_dir = state
        .session_dir
        .as_ref()
        .ok_or("No session directory configured")?;

    let metas = SessionManager::list_dir(session_dir).map_err(|e| e.to_string())?;

    Ok(metas
        .into_iter()
        .map(|m| SessionListEntry {
            id: m.id,
            path: m.path.to_string_lossy().to_string(),
            name: m.name.unwrap_or_else(|| {
                let mut chars = m.first_message.chars();
                let preview: String = chars.by_ref().take(50).collect();
                if chars.next().is_some() {
                    format!("{preview}...")
                } else {
                    m.first_message
                }
            }),
            modified: m.modified,
            message_count: m.message_count,
        })
        .collect())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForkPoint {
    pub message_index: usize,
    pub label: String,
}

#[tauri::command]
pub async fn get_fork_points(state: State<'_, GuiState>) -> Result<Vec<ForkPoint>, String> {
    let idx = *state.active_tab.lock().await;
    let tabs = state.tabs.lock().await;
    let tab = tabs.get(idx).ok_or("No active tab")?;
    Ok(tab
        .messages()
        .iter()
        .enumerate()
        .filter_map(|(message_index, message)| match message.as_standard()? {
            Message::User(user) => Some(ForkPoint {
                message_index,
                label: truncate_chars(&user.content.text(), 100),
            }),
            _ => None,
        })
        .collect())
}

#[tauri::command]
pub async fn fork_session(
    state: State<'_, GuiState>,
    app: AppHandle,
    message_index: usize,
) -> Result<String, String> {
    let (parent_path, copied_messages) = {
        let idx = *state.active_tab.lock().await;
        let tabs = state.tabs.lock().await;
        let tab = tabs.get(idx).ok_or("No active tab")?;
        let Some(message) = tab.messages().get(message_index) else {
            return Err("Fork point no longer exists".to_string());
        };
        if !matches!(message.as_standard(), Some(Message::User(_))) {
            return Err("A fork point must be a user message".to_string());
        }
        let copied_messages = tab.messages()[..=message_index]
            .iter()
            .filter_map(|message| message.as_standard().cloned())
            .collect::<Vec<_>>();
        (tab.path().to_string(), copied_messages)
    };

    let session_dir = state
        .session_dir
        .as_ref()
        .ok_or("No session directory configured")?;
    let created = state
        .shared
        .create_new_agent(session_dir, Some(parent_path))
        .await?;
    {
        let mut manager = created.agent.session_manager().await;
        for message in copied_messages {
            manager
                .append_message(message)
                .map_err(|error| error.to_string())?;
        }
    }
    let messages = load_session_messages(&created.path)?;
    let session_id = created.id.clone();
    let mut tabs = state.tabs.lock().await;
    tabs.push(SessionTab::Active {
        path: created.path,
        agent: Arc::new(created.agent),
        live: crate::state::LiveState {
            messages,
            ..Default::default()
        },
    });
    let new_index = tabs.len() - 1;
    drop(tabs);
    *state.active_tab.lock().await = new_index;
    crate::events::spawn_event_forwarder_for_session(
        app.clone(),
        session_id.clone(),
        state.inner().clone(),
    );
    let tabs = state.tabs.lock().await;
    let snapshot = UiSnapshot::from_tab(&tabs[new_index], &state.shared);
    let _ = crate::events::emit_main(&app, "ui-state", &snapshot);
    drop(tabs);
    crate::events::emit_sidebar_state(&app, state.inner()).await?;
    Ok(session_id)
}

#[tauri::command]
pub async fn switch_session(
    state: State<'_, GuiState>,
    app: AppHandle,
    path: String,
) -> Result<(), String> {
    let mut tabs = state.tabs.lock().await;

    // 查找是否已经有这个 session 的 tab
    let existing_idx = tabs.iter().position(|t| t.path() == path);

    let target_idx = if let Some(idx) = existing_idx {
        idx
    } else {
        // 创建新的 Loaded tab（加载历史消息用于显示，不启动 agent）
        let messages = load_session_messages(&path)?;
        tabs.push(SessionTab::Loaded {
            path: path.clone(),
            messages,
        });
        tabs.len() - 1
    };

    drop(tabs);
    *state.active_tab.lock().await = target_idx;

    // 推送新 tab 的状态
    let tabs = state.tabs.lock().await;
    if let Some(tab) = tabs.get(target_idx) {
        let snapshot = UiSnapshot::from_tab(tab, &state.shared);
        let _ = crate::events::emit_main(&app, "ui-state", &snapshot);
    }
    drop(tabs);
    crate::events::emit_sidebar_state(&app, state.inner()).await?;

    Ok(())
}

#[tauri::command]
pub async fn new_session(state: State<'_, GuiState>, app: AppHandle) -> Result<String, String> {
    create_new_session(&state, &app).await
}

// --- 权限 ---

#[tauri::command]
pub async fn respond_permission(
    state: State<'_, GuiState>,
    app: AppHandle,
    session_id: String,
    id: String,
    choice: String,
    trust_key: Option<String>,
    trust_keys: Option<Vec<String>>,
    hint: Option<String>,
) -> Result<(), String> {
    let approvals = state
        .pending_approvals
        .as_ref()
        .ok_or("Permission system not initialized")?;

    let pending_key = permission_pending_key(&session_id, &id);
    let context = state
        .pending_permission_contexts
        .get(&pending_key)
        .map(|context| context.clone())
        .ok_or_else(|| format!("No pending approval context: {session_id}:{id}"))?;

    let response = match choice.as_str() {
        "allow" => PermissionResponse::Allow,
        "allow-session" => {
            let keys = trust_keys
                .or_else(|| trust_key.map(|key| vec![key]))
                .unwrap_or_default();
            let valid_keys = context
                .info
                .trust_groups
                .iter()
                .flat_map(|group| group.levels.iter())
                .map(|level| level.key.as_str())
                .collect::<std::collections::HashSet<_>>();
            if keys.iter().any(|key| !valid_keys.contains(key.as_str())) {
                return Err("Selected trust scope is not valid for this request".to_string());
            }
            for key in &keys {
                state.permission_controller.record_project_approval(key)?;
            }
            PermissionResponse::Allow
        }
        "deny-hint" => PermissionResponse::DenyWithHint {
            hint: hint
                .filter(|hint| !hint.trim().is_empty())
                .unwrap_or_else(|| {
                    rozsa_app::permissions::safer_alternative_hint(
                        &context.tool_name,
                        &context.args,
                    )
                }),
        },
        _ => PermissionResponse::Deny,
    };

    let (_, sender) = approvals
        .remove(&pending_key)
        .ok_or_else(|| format!("No pending approval: {session_id}:{id}"))?;
    state.pending_permission_contexts.remove(&pending_key);

    sender
        .send(response)
        .map_err(|_| "Failed to send permission response".to_string())?;
    crate::events::emit_sidebar_state(&app, state.inner()).await
}

/// Re-evaluate a queued request immediately before showing it. A trust granted
/// for an earlier request may have made this one safe without another prompt.
#[tauri::command]
pub async fn prepare_permission(
    state: State<'_, GuiState>,
    session_id: String,
    id: String,
) -> Result<Option<rozsa_app::permissions::ApprovalInfo>, String> {
    let pending_key = permission_pending_key(&session_id, &id);
    let context = state
        .pending_permission_contexts
        .get(&pending_key)
        .map(|context| context.clone())
        .ok_or_else(|| format!("No pending approval context: {session_id}:{id}"))?;
    match state
        .permission_controller
        .evaluate(&session_id, &context.tool_name, &context.args)
    {
        rozsa_app::permissions::PolicyVerdict::NeedApproval { info } => {
            if let Some(mut context) = state.pending_permission_contexts.get_mut(&pending_key) {
                context.info = info.clone();
            }
            Ok(Some(info))
        }
        rozsa_app::permissions::PolicyVerdict::Allow => {
            if let Some(approvals) = &state.pending_approvals
                && let Some((_, sender)) = approvals.remove(&pending_key)
            {
                let _ = sender.send(PermissionResponse::Allow);
            }
            state.pending_permission_contexts.remove(&pending_key);
            Ok(None)
        }
        rozsa_app::permissions::PolicyVerdict::Block { .. } => {
            if let Some(approvals) = &state.pending_approvals
                && let Some((_, sender)) = approvals.remove(&pending_key)
            {
                let _ = sender.send(PermissionResponse::Deny);
            }
            state.pending_permission_contexts.remove(&pending_key);
            Ok(None)
        }
    }
}

// --- 设置 ---

#[tauri::command]
pub async fn get_settings(
    state: State<'_, GuiState>,
    app: AppHandle,
) -> Result<SettingsSnapshot, String> {
    let rt = state.runtime_settings.lock().await;
    let model = state.shared.model.lock().await;
    let thinking = state.shared.thinking_level.lock().await;

    let snapshot = SettingsSnapshot {
        permission_mode: rt.permissions.mode.clone(),
        thinking_level: format!("{:?}", *thinking).to_lowercase(),
        model_id: model.id.clone(),
        model_name: model.name.clone(),
        model_provider: model.provider.as_str().to_string(),
        auto_approve_patterns: rt.permissions.auto_approve_patterns.clone(),
        allowed_tools: rt.permissions.allowed_tools.clone(),
        block_images: rt.block_images,
        hide_thinking: rt.hide_thinking,
        transport: rt.transport.clone(),
        auto_compact: rt.compaction.enabled,
        auto_session_naming: rt.auto_session_naming,
        steering_mode: rt.steering_mode.clone(),
        follow_up_mode: rt.follow_up_mode.clone(),
        running_send_mode: rt.running_send_mode.clone(),
        appearance: AppearanceSnapshot {
            theme_mode: rt.appearance.theme_mode.clone(),
            font_size: rt.appearance.font_size,
            light_theme: rt.appearance.light_theme.clone(),
            dark_theme: rt.appearance.dark_theme.clone(),
            is_macos: cfg!(target_os = "macos"),
        },
    };
    publish_theme_state(&app, &snapshot.appearance)?;
    Ok(snapshot)
}

#[tauri::command]
pub fn list_themes() -> Result<Vec<ThemeSummary>, String> {
    theme_store()?.list().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn get_theme(id: String, mode: String) -> Result<ThemeDefinition, String> {
    let mode = parse_theme_mode(&mode)?;
    theme_store()?
        .load(&id, mode)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn save_theme(theme: ThemeDefinition) -> Result<(), String> {
    theme_store()?
        .save(&theme)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn update_setting(
    state: State<'_, GuiState>,
    app: AppHandle,
    key: String,
    value: String,
) -> Result<(), String> {
    match key.as_str() {
        "thinking" => {
            use rozsa_model::types::ThinkingLevel;
            let level = match value.as_str() {
                "off" => ThinkingLevel::Off,
                "minimal" => ThinkingLevel::Minimal,
                "low" => ThinkingLevel::Low,
                "medium" => ThinkingLevel::Medium,
                "high" => ThinkingLevel::High,
                "xhigh" => ThinkingLevel::XHigh,
                _ => return Err(format!("Invalid thinking level: {value}")),
            };
            *state.shared.thinking_level.lock().await = level;
            // 同步到所有 active sessions
            let tabs = state.tabs.lock().await;
            for tab in tabs.iter() {
                if let SessionTab::Active { agent, .. } = tab {
                    agent.set_thinking_level(level).await;
                }
            }
            drop(tabs);
            {
                let mut s = state.runtime_settings.lock().await;
                s.default_thinking_level = Some(level);
            }
            persist_settings(&state).await;
            Ok(())
        }
        "permission_mode" => {
            let mut s = state.runtime_settings.lock().await;
            s.permissions.mode = value;
            let mode = rozsa_app::permissions::PermissionMode::parse(&s.permissions.mode)
                .ok_or_else(|| format!("Invalid permission mode: {}", s.permissions.mode))?;
            state.permission_controller.update(
                mode,
                s.permissions.auto_approve_patterns.clone(),
                s.permissions.allowed_tools.clone(),
                s.permissions.blocked_commands.clone(),
            );
            drop(s);
            persist_settings(&state).await;
            Ok(())
        }
        "auto_compact" => {
            let mut s = state.runtime_settings.lock().await;
            s.compaction.enabled = value == "true";
            drop(s);
            persist_settings(&state).await;
            Ok(())
        }
        "auto_session_naming" => {
            state.runtime_settings.lock().await.auto_session_naming = value == "true";
            persist_settings(&state).await;
            Ok(())
        }
        "steering_mode" => {
            let mut s = state.runtime_settings.lock().await;
            s.steering_mode = value;
            drop(s);
            persist_settings(&state).await;
            Ok(())
        }
        "follow_up_mode" => {
            let mut s = state.runtime_settings.lock().await;
            s.follow_up_mode = value;
            drop(s);
            persist_settings(&state).await;
            Ok(())
        }
        "running_send_mode" => {
            if value != "queue" && value != "steer" {
                return Err(format!("Invalid running send mode: {value}"));
            }
            state.runtime_settings.lock().await.running_send_mode = value;
            persist_settings(&state).await;
            Ok(())
        }
        "transport" => {
            let mut s = state.runtime_settings.lock().await;
            s.transport = value;
            drop(s);
            persist_settings(&state).await;
            Ok(())
        }
        "block_images" => {
            let mut s = state.runtime_settings.lock().await;
            s.block_images = value == "true";
            drop(s);
            persist_settings(&state).await;
            Ok(())
        }
        "appearance_theme_mode" => {
            if !matches!(value.as_str(), "system" | "light" | "dark") {
                return Err(format!("Invalid theme mode: {value}"));
            }
            let mut s = state.runtime_settings.lock().await;
            s.appearance.theme_mode = value;
            drop(s);
            persist_settings(&state).await;
            emit_theme_state(&state, &app).await?;
            Ok(())
        }
        "appearance_font_size" => {
            let font_size = value
                .parse::<u8>()
                .map_err(|_| format!("Invalid font size: {value}"))?;
            if !(5..=50).contains(&font_size) {
                return Err(format!("Font size must be between 5 and 50: {font_size}"));
            }
            let mut s = state.runtime_settings.lock().await;
            s.appearance.font_size = font_size;
            drop(s);
            persist_settings(&state).await;
            emit_theme_state(&state, &app).await?;
            Ok(())
        }
        "appearance_light_theme" => {
            theme_store()?
                .load(&value, ThemeMode::Light)
                .map_err(|error| error.to_string())?;
            let mut s = state.runtime_settings.lock().await;
            s.appearance.light_theme = value;
            drop(s);
            persist_settings(&state).await;
            emit_theme_state(&state, &app).await?;
            Ok(())
        }
        "appearance_dark_theme" => {
            theme_store()?
                .load(&value, ThemeMode::Dark)
                .map_err(|error| error.to_string())?;
            let mut s = state.runtime_settings.lock().await;
            s.appearance.dark_theme = value;
            drop(s);
            persist_settings(&state).await;
            emit_theme_state(&state, &app).await?;
            Ok(())
        }
        _ => Err(format!("Unknown setting: {key}")),
    }
}

fn parse_theme_mode(value: &str) -> Result<ThemeMode, String> {
    match value {
        "light" => Ok(ThemeMode::Light),
        "dark" => Ok(ThemeMode::Dark),
        _ => Err(format!("Invalid theme mode: {value}")),
    }
}

fn theme_store() -> Result<ThemeStore, String> {
    themes::user_theme_store().map_err(|error| error.to_string())
}

fn publish_theme_state(app: &AppHandle, appearance: &AppearanceSnapshot) -> Result<(), String> {
    let store = theme_store()?;
    let light_theme = store
        .load(&appearance.light_theme, ThemeMode::Light)
        .map_err(|error| error.to_string())?;
    let dark_theme = store
        .load(&appearance.dark_theme, ThemeMode::Dark)
        .map_err(|error| error.to_string())?;
    crate::events::emit_theme_state(app, appearance, light_theme, dark_theme)?;
    Ok(())
}

async fn emit_theme_state(state: &State<'_, GuiState>, app: &AppHandle) -> Result<(), String> {
    let settings = state.runtime_settings.lock().await;
    let appearance = AppearanceSnapshot {
        theme_mode: settings.appearance.theme_mode.clone(),
        font_size: settings.appearance.font_size,
        light_theme: settings.appearance.light_theme.clone(),
        dark_theme: settings.appearance.dark_theme.clone(),
        is_macos: cfg!(target_os = "macos"),
    };
    drop(settings);
    publish_theme_state(app, &appearance)
}

// --- 模型 ---

#[tauri::command]
pub async fn list_models(state: State<'_, GuiState>) -> Result<Vec<ModelListEntry>, String> {
    let registry = state
        .model_registry
        .as_ref()
        .ok_or("No model registry available")?;

    Ok(registry
        .all()
        .iter()
        .map(|m| ModelListEntry {
            id: m.id.clone(),
            name: m.name.clone(),
            provider: format!("{:?}", m.provider),
        })
        .collect())
}

#[tauri::command]
pub async fn switch_model(state: State<'_, GuiState>, model_id: String) -> Result<(), String> {
    let registry = state
        .model_registry
        .as_ref()
        .ok_or("No model registry available")?;

    let model = match registry.find_by_id(&model_id) {
        Some(model) => model,
        None => {
            let mut model = state.shared.model.lock().await.clone();
            model.id = model_id.clone();
            model.name = model_id.clone();
            model
        }
    };

    // 更新共享 model
    *state.shared.model.lock().await = model.clone();
    {
        let mut settings = state.runtime_settings.lock().await;
        settings.default_model = Some(model.id.clone());
        settings.default_provider = Some(model.provider.as_str().to_string());
    }
    persist_settings(&state).await;

    // 同步到所有 active sessions
    let tabs = state.tabs.lock().await;
    for tab in tabs.iter() {
        if let SessionTab::Active { agent, .. } = tab {
            agent.set_model(model.clone()).await;
        }
    }
    Ok(())
}

// --- 认证 ---

#[tauri::command]
pub async fn auth_login(app: AppHandle) -> Result<String, String> {
    use rozsa_model::credentials::store_oauth_credentials;
    use rozsa_model::oauth::openai_codex;
    use rozsa_model::oauth::types::OAuthFlowEvent;
    use tokio::sync::mpsc as tokio_mpsc;
    use tokio_util::sync::CancellationToken;

    let (flow_event_tx, mut flow_event_rx) = tokio_mpsc::unbounded_channel();
    let (_response_tx, response_rx) = tokio_mpsc::unbounded_channel();
    let cancel = CancellationToken::new();
    let login_handle = tokio::spawn(openai_codex::login(
        flow_event_tx,
        response_rx,
        cancel.clone(),
    ));

    while let Some(event) = flow_event_rx.recv().await {
        match event {
            OAuthFlowEvent::AuthUrl { url, .. } => {
                let opened = open_url(&url);
                let message = if opened {
                    "Opened browser for codex-oauth login.".to_string()
                } else {
                    format!("Open this URL to continue codex-oauth login:\n{url}")
                };
                let _ = crate::events::emit_main(&app, "notification", message);
            }
            OAuthFlowEvent::Progress { message } => {
                let _ = crate::events::emit_main(&app, "notification", message);
            }
            _ => {}
        }
    }

    match login_handle.await {
        Ok(Ok(credentials)) => {
            let models_dir = models_dir()?;
            std::fs::create_dir_all(&models_dir)
                .map_err(|e| format!("Failed to create models directory: {e}"))?;
            let auth_path = models_dir.join("auth.json");
            store_oauth_credentials(
                auth_path.to_str().unwrap_or(""),
                "codex-oauth",
                &credentials,
            )?;
            ensure_codex_oauth_models_config(&models_dir)?;
            Ok("codex-oauth login successful.".to_string())
        }
        Ok(Err(e)) => Err(format!("Login failed: {e}")),
        Err(e) => Err(format!("Login task panicked: {e}")),
    }
}

#[tauri::command]
pub async fn auth_logout() -> Result<String, String> {
    let auth_path = models_dir()?.join("auth.json");
    let removed = rozsa_model::credentials::remove_stored_credentials(
        auth_path.to_str().unwrap_or(""),
        "codex-oauth",
    )?;
    if removed {
        Ok("codex-oauth credentials removed.".to_string())
    } else {
        Ok("No codex-oauth credentials found.".to_string())
    }
}

#[tauri::command]
pub async fn get_rate_limits(
    state: State<'_, GuiState>,
    app: AppHandle,
) -> Result<rozsa_model::rate_limit::RateLimitSnapshot, String> {
    refresh_rate_limits(&state, &app).await
}

async fn refresh_rate_limits(
    state: &State<'_, GuiState>,
    app: &AppHandle,
) -> Result<rozsa_model::rate_limit::RateLimitSnapshot, String> {
    let snapshot = rozsa_app::rate_limit::get_rate_limits()
        .await
        .map_err(|e| e.to_string())?;
    *state.quota_summary.lock().await = Some(snapshot.clone());
    crate::events::emit_sidebar_state(app, state.inner()).await?;
    Ok(snapshot)
}

// --- 其他操作 ---

#[tauri::command]
pub async fn compact(state: State<'_, GuiState>) -> Result<(), String> {
    let idx = *state.active_tab.lock().await;
    let tabs = state.tabs.lock().await;
    if let Some(SessionTab::Active { agent, .. }) = tabs.get(idx) {
        agent.compact().await.map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn rename_session(
    state: State<'_, GuiState>,
    app: AppHandle,
    path: String,
    name: String,
) -> Result<(), String> {
    let new_name = (!name.trim().is_empty()).then(|| name.trim().to_string());
    let active_agent = {
        let tabs = state.tabs.lock().await;
        tabs.iter().find_map(|tab| match tab {
            SessionTab::Active {
                path: active_path,
                agent,
                ..
            } if active_path == &path => Some(agent.clone()),
            _ => None,
        })
    };
    if let Some(agent) = active_agent {
        agent
            .session_manager()
            .await
            .append_session_info(new_name)
            .map_err(|error| error.to_string())?;
    } else {
        SessionManager::rename(&path, new_name).map_err(|error| error.to_string())?;
    }
    emit_session_views(state.inner(), &app, &session_id_from_path(&path)).await
}

#[tauri::command]
pub async fn delete_session(
    state: State<'_, GuiState>,
    app: AppHandle,
    path: String,
) -> Result<(), String> {
    let session_id = session_id_from_path(&path);
    if let Some(approvals) = &state.pending_approvals {
        deny_pending_approvals(approvals, Some(&session_id));
    }
    // 从 tabs 移除该 session
    let mut tabs = state.tabs.lock().await;
    tabs.retain(|t| t.path() != path);
    drop(tabs);
    SessionManager::delete(&path).map_err(|e| e.to_string())?;
    crate::events::emit_sidebar_state(&app, state.inner()).await
}

#[tauri::command]
pub async fn run_bash(
    state: State<'_, GuiState>,
    app: AppHandle,
    command: String,
) -> Result<(), String> {
    let idx = *state.active_tab.lock().await;
    let tabs = state.tabs.lock().await;
    let agent = match tabs.get(idx) {
        Some(SessionTab::Active { agent, .. }) => agent.clone(),
        _ => return Err("No active agent for bash execution".to_string()),
    };
    drop(tabs);

    // 通过 agent.prompt 执行 bang command（agent 内部会识别 ! 前缀）
    tokio::spawn(async move {
        let bang_cmd = format!("!{}", command);
        if let Err(e) = agent.prompt(&bang_cmd).await {
            let _ = crate::events::emit_main(&app, "error", e.to_string());
        }
    });
    Ok(())
}

// --- 辅助 ---

fn slash_action(action: &str) -> Result<SlashCommandResult, String> {
    Ok(SlashCommandResult {
        handled: true,
        action: Some(action.to_string()),
        value: None,
    })
}

fn slash_action_arg(action: &str, value: String) -> Result<SlashCommandResult, String> {
    Ok(SlashCommandResult {
        handled: true,
        action: Some(action.to_string()),
        value: Some(value),
    })
}

fn emit_info(app: &AppHandle, message: &str) {
    let _ = crate::events::emit_main(app, "notification", message.to_string());
}

fn parse_slash_completion_prefix(head: &str) -> Option<String> {
    let chars = head.char_indices().collect::<Vec<_>>();
    for (idx, ch) in chars.iter().rev() {
        if *ch == '/' && (*idx == 0 || head[..*idx].ends_with(char::is_whitespace)) {
            return Some(head[*idx + 1..].to_ascii_lowercase());
        }
        if ch.is_whitespace() {
            break;
        }
    }
    None
}

fn utf16_offset_to_byte_index(text: &str, offset: usize) -> usize {
    let mut utf16_offset = 0;
    for (byte_idx, ch) in text.char_indices() {
        if offset <= utf16_offset {
            return byte_idx;
        }
        let next_utf16_offset = utf16_offset + ch.len_utf16();
        if offset < next_utf16_offset {
            return byte_idx;
        }
        utf16_offset = next_utf16_offset;
    }
    text.len()
}

fn parse_at_completion_prefix(head: &str) -> Option<String> {
    if let Some(start) = head.rfind("@\"") {
        let after = &head[start + 2..];
        if (start == 0 || head[..start].ends_with(char::is_whitespace)) && !after.contains('"') {
            return Some(head[start..].to_string());
        }
    }

    let chars = head.char_indices().collect::<Vec<_>>();
    for (idx, ch) in chars.iter().rev() {
        if *ch == '@' && (*idx == 0 || head[..*idx].ends_with(char::is_whitespace)) {
            return Some(head[*idx..].to_string());
        }
        if ch.is_whitespace() {
            break;
        }
    }
    None
}

fn input_highlight_ranges(
    text: &str,
    cwd: &std::path::Path,
    skill_commands: &[SkillSlashCommand],
) -> Vec<InputHighlightRange> {
    let mut ranges = Vec::new();
    for (start, end) in slash_command_ranges(text, skill_commands) {
        ranges.push(InputHighlightRange {
            start: char_offset(text, start),
            end: char_offset(text, end),
        });
    }
    for (start, end) in file_reference_ranges(text, cwd) {
        ranges.push(InputHighlightRange {
            start: char_offset(text, start),
            end: char_offset(text, end),
        });
    }
    ranges
}

#[derive(Clone)]
struct SkillSlashCommand {
    name: String,
    description: String,
}

fn collect_skill_slash_commands(
    agent: Option<&AgentSession>,
    cwd: &std::path::Path,
) -> Vec<SkillSlashCommand> {
    if let Some(agent) = agent {
        return agent
            .skill_registry()
            .list()
            .iter()
            .map(|skill| SkillSlashCommand {
                name: skill.name.clone(),
                description: skill.description.clone(),
            })
            .collect();
    }

    SkillRegistry::load_from_defaults(cwd)
        .list()
        .iter()
        .map(|skill| SkillSlashCommand {
            name: skill.name.clone(),
            description: skill.description.clone(),
        })
        .collect()
}

fn slash_command_ranges(text: &str, skill_commands: &[SkillSlashCommand]) -> Vec<(usize, usize)> {
    slash_command_tokens(text)
        .into_iter()
        .filter_map(|token| {
            valid_slash_command(token.command, skill_commands).then_some((token.start, token.end))
        })
        .collect()
}

fn first_builtin_slash_command(text: &str) -> Option<(String, String)> {
    slash_command_tokens(text)
        .into_iter()
        .filter(|token| is_builtin_slash_command(token.command))
        .map(|token| {
            (
                token.command.to_ascii_lowercase(),
                text[token.end..].trim().to_string(),
            )
        })
        .next()
}

fn first_skill_slash_command(
    text: &str,
    skill_commands: &[SkillSlashCommand],
) -> Option<(String, String)> {
    slash_command_tokens(text)
        .into_iter()
        .filter(|token| skill_name_for_command(token.command, skill_commands).is_some())
        .map(|token| {
            (
                token.command.to_ascii_lowercase(),
                text[token.end..].trim().to_string(),
            )
        })
        .next()
}

struct SlashCommandToken<'a> {
    start: usize,
    end: usize,
    command: &'a str,
}

fn slash_command_tokens(text: &str) -> Vec<SlashCommandToken<'_>> {
    let mut tokens = Vec::new();
    for (start, ch) in text.char_indices() {
        if ch != '/' || (start > 0 && !text[..start].ends_with(char::is_whitespace)) {
            continue;
        }
        let command_start = start + ch.len_utf8();
        let command_end = text[command_start..]
            .char_indices()
            .find_map(|(idx, ch)| ch.is_whitespace().then_some(command_start + idx))
            .unwrap_or(text.len());
        if command_end == command_start {
            continue;
        }
        tokens.push(SlashCommandToken {
            start,
            end: command_end,
            command: &text[command_start..command_end],
        });
    }
    tokens
}

fn valid_slash_command(command: &str, skill_commands: &[SkillSlashCommand]) -> bool {
    is_builtin_slash_command(command) || skill_name_for_command(command, skill_commands).is_some()
}

fn is_builtin_slash_command(command: &str) -> bool {
    let command = command.to_ascii_lowercase();
    rozsa_app::slash_commands::BUILTIN_SLASH_COMMANDS
        .iter()
        .any(|cmd| cmd.name == command)
}

fn skill_name_for_command(command: &str, skill_commands: &[SkillSlashCommand]) -> Option<String> {
    let command = command.to_ascii_lowercase();
    let skill_name = command.strip_prefix("skill:").unwrap_or(&command);
    if command != skill_name {
        return skill_commands
            .iter()
            .any(|skill| skill.name == skill_name)
            .then(|| skill_name.to_string());
    }
    let has_builtin_conflict = rozsa_app::slash_commands::BUILTIN_SLASH_COMMANDS
        .iter()
        .any(|cmd| cmd.name == skill_name);
    (!has_builtin_conflict && skill_commands.iter().any(|skill| skill.name == skill_name))
        .then(|| skill_name.to_string())
}

fn file_reference_ranges(text: &str, cwd: &std::path::Path) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut iter = text.char_indices().peekable();
    while let Some((start, ch)) = iter.next() {
        if ch != '@' || (start > 0 && !text[..start].ends_with(char::is_whitespace)) {
            continue;
        }

        let Some((next_idx, next_ch)) = iter.peek().copied() else {
            continue;
        };
        if next_ch == '"' {
            iter.next();
            let mut end = None;
            while let Some((idx, ch)) = iter.next() {
                if ch == '"' {
                    end = Some(idx + ch.len_utf8());
                    break;
                }
            }
            let Some(end_idx) = end else {
                continue;
            };
            let path = &text[next_idx + 1..end_idx - 1];
            if resolved_autocomplete_path(path, cwd)
                .as_ref()
                .is_some_and(|path| path.exists())
            {
                ranges.push((start, end_idx));
            }
            continue;
        }

        let mut end = text.len();
        while let Some((idx, ch)) = iter.peek().copied() {
            if ch.is_whitespace() {
                end = idx;
                break;
            }
            iter.next();
        }
        let path = &text[next_idx..end];
        if !path.is_empty()
            && resolved_autocomplete_path(path, cwd)
                .as_ref()
                .is_some_and(|path| path.exists())
        {
            ranges.push((start, end));
        }
    }
    ranges
}

fn char_offset(text: &str, byte_idx: usize) -> usize {
    text[..byte_idx].chars().count()
}

fn resolved_autocomplete_path(path: &str, cwd: &std::path::Path) -> Option<std::path::PathBuf> {
    if path.is_empty() {
        return None;
    }
    if path == "~" {
        return dirs::home_dir();
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return dirs::home_dir().map(|home| home.join(rest));
    }
    let parsed = std::path::PathBuf::from(path);
    if parsed.is_absolute() {
        Some(parsed)
    } else {
        Some(cwd.join(parsed))
    }
}

#[cfg(test)]
mod token_tests {
    use super::*;

    #[test]
    fn attachment_pick_modes_accept_file_directory_and_any() {
        assert!(matches!(
            AttachmentPickMode::parse("any"),
            Ok(AttachmentPickMode::Any)
        ));
        assert!(matches!(
            AttachmentPickMode::parse("file"),
            Ok(AttachmentPickMode::File)
        ));
        assert!(matches!(
            AttachmentPickMode::parse("directory"),
            Ok(AttachmentPickMode::Directory)
        ));
        assert!(AttachmentPickMode::parse("unsupported").is_err());
    }

    #[test]
    fn slash_tokens_highlight_anywhere_in_input() {
        let text = "prefix /tree suffix /model";
        let skill_commands = Vec::new();

        assert_eq!(
            slash_command_ranges(text, &skill_commands),
            vec![(7, 12), (20, 26)]
        );
        assert_eq!(
            first_builtin_slash_command(text),
            Some(("tree".to_string(), "suffix /model".to_string()))
        );
    }

    #[test]
    fn skill_tokens_highlight_without_active_agent() {
        let text = "prefix /brainstorm suffix";
        let skill_commands = vec![SkillSlashCommand {
            name: "brainstorm".to_string(),
            description: "Collaborative exploration".to_string(),
        }];

        assert_eq!(slash_command_ranges(text, &skill_commands), vec![(7, 18)]);
        assert_eq!(
            first_skill_slash_command(text, &skill_commands),
            Some(("brainstorm".to_string(), "suffix".to_string()))
        );
    }

    #[test]
    fn skill_tokens_normalize_all_matches() {
        let text = "prefix /brainstorm and /ask suffix";
        let skill_commands = vec![
            SkillSlashCommand {
                name: "brainstorm".to_string(),
                description: "Collaborative exploration".to_string(),
            },
            SkillSlashCommand {
                name: "ask".to_string(),
                description: "Ask".to_string(),
            },
        ];

        assert_eq!(
            normalize_skill_commands_in_text(text, &skill_commands),
            Some("prefix /skill:brainstorm and /skill:ask suffix".to_string())
        );
    }

    #[test]
    fn file_reference_tokens_highlight_anywhere_in_input() {
        let cwd = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let text = "prefix @src/commands.rs suffix";

        assert_eq!(file_reference_ranges(text, cwd), vec![(7, 23)]);
    }

    #[test]
    fn quoted_file_reference_tokens_with_spaces_highlight() {
        let temp_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../temp");
        let temp_dir = tempfile::Builder::new()
            .prefix("file-drag-highlight-")
            .tempdir_in(temp_root)
            .unwrap();
        let file = temp_dir.path().join("source file.rs");
        std::fs::write(&file, "fn main() {}\n").unwrap();
        let text = format!(r#"@"{}""#, file.display());

        assert_eq!(
            file_reference_ranges(&text, temp_dir.path()),
            vec![(0, text.len())]
        );
    }

    #[test]
    fn utf16_cursor_offsets_never_split_multibyte_characters() {
        let text = r#"@"/Users/xinyue/Downloads/希音 ai数据分析一面.txt" "#;
        let cursor = text.encode_utf16().count();

        assert_eq!(utf16_offset_to_byte_index(text, cursor), text.len());
        assert_eq!(utf16_offset_to_byte_index("😀/path", 1), 0);
        assert_eq!(utf16_offset_to_byte_index("😀/path", 2), 4);
    }
}

#[derive(Clone, Copy)]
enum AttachmentPickMode {
    Any,
    File,
    Directory,
}

impl AttachmentPickMode {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "" | "any" => Ok(Self::Any),
            "file" => Ok(Self::File),
            "directory" => Ok(Self::Directory),
            other => Err(format!("Unknown attachment picker mode: {other}")),
        }
    }
}

fn normalize_skill_commands_in_text(
    text: &str,
    skill_commands: &[SkillSlashCommand],
) -> Option<String> {
    let mut normalized = String::new();
    let mut cursor = 0;
    let mut changed = false;
    for token in slash_command_tokens(text) {
        let Some(skill_name) = skill_name_for_command(token.command, skill_commands) else {
            continue;
        };
        normalized.push_str(&text[cursor..token.start]);
        normalized.push_str("/skill:");
        normalized.push_str(&skill_name);
        cursor = token.end;
        changed = true;
    }
    if !changed {
        return None;
    }
    normalized.push_str(&text[cursor..]);
    Some(normalized)
}

async fn active_agent(state: &State<'_, GuiState>) -> Result<Arc<AgentSession>, String> {
    let idx = *state.active_tab.lock().await;
    let tabs = state.tabs.lock().await;
    match tabs.get(idx) {
        Some(SessionTab::Active { agent, .. }) => Ok(agent.clone()),
        _ => Err("No active agent".to_string()),
    }
}

async fn active_session_summary(state: &State<'_, GuiState>) -> Result<(String, usize), String> {
    let idx = *state.active_tab.lock().await;
    let tabs = state.tabs.lock().await;
    let tab = tabs.get(idx).ok_or("No active tab")?;
    Ok((tab.path().to_string(), tab.messages().len()))
}

async fn create_new_session(
    state: &State<'_, GuiState>,
    app: &AppHandle,
) -> Result<String, String> {
    let session_dir = state
        .session_dir
        .as_ref()
        .ok_or("No session directory configured")?
        .clone();
    let created = state.shared.create_new_agent(&session_dir, None).await?;
    let session_id = created.id.clone();
    let agent = Arc::new(created.agent);
    let mut tabs = state.tabs.lock().await;
    tabs.push(SessionTab::Active {
        path: created.path,
        agent,
        live: crate::state::LiveState::default(),
    });
    let new_idx = tabs.len() - 1;
    drop(tabs);

    *state.active_tab.lock().await = new_idx;
    crate::events::spawn_event_forwarder_for_session(
        app.clone(),
        session_id,
        state.inner().clone(),
    );
    let tabs = state.tabs.lock().await;
    if let Some(tab) = tabs.get(new_idx) {
        let snapshot = UiSnapshot::from_tab(tab, &state.shared);
        let _ = crate::events::emit_main(&app, "ui-state", &snapshot);
    }
    drop(tabs);
    crate::events::emit_sidebar_state(app, state.inner()).await?;

    Ok(created.id)
}

async fn switch_model_reference(
    state: &State<'_, GuiState>,
    reference: &str,
) -> Result<(), String> {
    let registry = state
        .model_registry
        .as_ref()
        .ok_or("No model registry available")?;

    let model = if let Some((provider, id)) = reference.split_once('/') {
        registry
            .resolve(provider, id)
            .ok_or_else(|| format!("Model {provider}/{id} not found"))?
    } else {
        registry
            .all()
            .iter()
            .find(|m| m.id == reference || m.id.contains(reference) || m.id.ends_with(reference))
            .cloned()
            .ok_or_else(|| format!("Model not found: {reference}"))?
    };

    *state.shared.model.lock().await = model.clone();
    {
        let mut settings = state.runtime_settings.lock().await;
        settings.default_model = Some(model.id.clone());
        settings.default_provider = Some(model.provider.as_str().to_string());
    }
    persist_settings(state).await;

    let tabs = state.tabs.lock().await;
    for tab in tabs.iter() {
        if let SessionTab::Active { agent, .. } = tab {
            agent.set_model(model.clone()).await;
        }
    }
    Ok(())
}

fn parse_thinking_level(value: &str) -> Result<ThinkingLevel, String> {
    match value.to_ascii_lowercase().as_str() {
        "" | "off" => Ok(ThinkingLevel::Off),
        "minimal" | "min" => Ok(ThinkingLevel::Minimal),
        "low" | "l" => Ok(ThinkingLevel::Low),
        "medium" | "med" | "m" => Ok(ThinkingLevel::Medium),
        "high" | "h" => Ok(ThinkingLevel::High),
        "xhigh" | "x" => Ok(ThinkingLevel::XHigh),
        other => Err(format!(
            "Unknown thinking level: {other}. Use: off/minimal/low/medium/high/xhigh"
        )),
    }
}

async fn set_thinking_level(state: &State<'_, GuiState>, level: ThinkingLevel) {
    *state.shared.thinking_level.lock().await = level;
    let tabs = state.tabs.lock().await;
    for tab in tabs.iter() {
        if let SessionTab::Active { agent, .. } = tab {
            agent.set_thinking_level(level).await;
        }
    }
    drop(tabs);
    {
        let mut settings = state.runtime_settings.lock().await;
        settings.default_thinking_level = Some(level);
    }
    persist_settings(state).await;
}

async fn compact_active_session(state: &State<'_, GuiState>) -> Result<(), String> {
    let idx = *state.active_tab.lock().await;
    let tabs = state.tabs.lock().await;
    if let Some(SessionTab::Active { agent, .. }) = tabs.get(idx) {
        agent.compact().await.map_err(|e| e.to_string())?;
    }
    Ok(())
}

async fn last_assistant_text(state: &State<'_, GuiState>) -> Result<String, String> {
    let idx = *state.active_tab.lock().await;
    let tabs = state.tabs.lock().await;
    let tab = tabs.get(idx).ok_or("No active tab")?;
    Ok(tab
        .messages()
        .iter()
        .rev()
        .find_map(|msg| match msg.as_standard()? {
            Message::Assistant(a) => Some(text_from_blocks(&a.content)),
            _ => None,
        })
        .unwrap_or_default())
}

async fn search_messages(state: &State<'_, GuiState>, pattern: &str) -> String {
    let idx = *state.active_tab.lock().await;
    let tabs = state.tabs.lock().await;
    let Some(tab) = tabs.get(idx) else {
        return "No active tab".to_string();
    };
    let needle = pattern.to_ascii_lowercase();
    let mut results = Vec::new();
    for msg in tab.messages() {
        let Some(standard) = msg.as_standard() else {
            continue;
        };
        let text = match standard {
            Message::Assistant(a) => text_from_blocks(&a.content),
            Message::ToolResult(tr) => text_from_blocks(&tr.content),
            Message::User(u) => u.content.text(),
        };
        for line in text.lines() {
            if line.to_ascii_lowercase().contains(&needle) {
                results.push(line.to_string());
                if results.len() >= 50 {
                    break;
                }
            }
        }
        if results.len() >= 50 {
            break;
        }
    }
    if results.is_empty() {
        format!("No matches for '{pattern}'")
    } else {
        format!(
            "Search results for '{}' ({} matches):\n{}",
            pattern,
            results.len(),
            results.join("\n")
        )
    }
}

async fn session_tree_summary(state: &State<'_, GuiState>) -> Result<String, String> {
    let agent = active_agent(state).await?;
    let manager = agent.session_manager().await;
    let entries = manager.entries();
    drop(manager);
    if entries.is_empty() {
        return Ok("Session tree is empty".to_string());
    }
    Ok(format!(
        "Session tree ({} entries):\n{}",
        entries.len(),
        entries
            .iter()
            .enumerate()
            .map(|(idx, entry)| format!("{:>3}. {}", idx + 1, session_entry_summary(entry)))
            .collect::<Vec<_>>()
            .join("\n")
    ))
}

async fn conversation_graph_summary(state: &State<'_, GuiState>) -> Result<String, String> {
    let idx = *state.active_tab.lock().await;
    let tabs = state.tabs.lock().await;
    let tab = tabs.get(idx).ok_or("No active tab")?;
    if tab.messages().is_empty() {
        return Ok("Conversation graph is empty".to_string());
    }
    Ok(format!(
        "Conversation graph ({} messages):\n{}",
        tab.messages().len(),
        tab.messages()
            .iter()
            .enumerate()
            .filter_map(|(idx, message)| {
                let standard = message.as_standard()?;
                Some(format!("{:>3}. {}", idx + 1, message_summary(standard)))
            })
            .collect::<Vec<_>>()
            .join("\n")
    ))
}

async fn clone_active_session(state: &State<'_, GuiState>) -> Result<String, String> {
    let agent = active_agent(state).await?;
    let manager = agent.session_manager().await;
    let entries = manager.entries();
    let cwd = agent.current_cwd().await.to_string_lossy().to_string();
    drop(manager);

    let session_dir = state
        .session_dir
        .as_ref()
        .ok_or("No session directory configured")?;
    let new_id = uuid::Uuid::new_v4().to_string();
    let new_path = session_dir.join(format!("{new_id}.jsonl"));
    let mut new_manager =
        SessionManager::create(&new_path, new_id, cwd, None).map_err(|e| e.to_string())?;
    let mut count = 0usize;
    for entry in &entries {
        if let rozsa_app::session::manager::SessionEntry::Message(message_entry) = entry {
            new_manager
                .append_message(message_entry.message.clone())
                .map_err(|e| e.to_string())?;
            count += 1;
        }
    }
    Ok(format!(
        "Cloned {count} messages to new session: {}",
        new_path.display()
    ))
}

fn import_session_summary(path: &str) -> String {
    match std::fs::read_to_string(path) {
        Ok(content) => {
            let count = content
                .lines()
                .filter(|line| !line.trim().is_empty())
                .filter(|line| serde_json::from_str::<serde_json::Value>(line).is_ok())
                .count();
            format!("Imported {count} entries from {path}")
        }
        Err(e) => format!("Import failed: {e}"),
    }
}

async fn share_active_session(state: &State<'_, GuiState>) -> Result<String, String> {
    let agent = active_agent(state).await?;
    let manager = agent.session_manager().await;
    let entries = manager.entries();
    drop(manager);

    let temp_dir = state.shared.cwd.join("temp");
    std::fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;
    let export_path = temp_dir.join("rozsa-share-export.jsonl");
    let mut lines = Vec::with_capacity(entries.len());
    for entry in &entries {
        lines.push(serde_json::to_string(entry).map_err(|e| e.to_string())?);
    }
    std::fs::write(&export_path, lines.join("\n") + "\n").map_err(|e| e.to_string())?;

    let output = Command::new("gh")
        .args([
            "gist",
            "create",
            "--public=false",
            &export_path.to_string_lossy(),
        ])
        .output()
        .map_err(|e| format!("Failed to run gh: {e}"))?;
    if output.status.success() {
        Ok(format!(
            "Shared as gist: {}",
            String::from_utf8_lossy(&output.stdout).trim()
        ))
    } else {
        Ok(format!(
            "gh gist create failed: {}\nExport kept at {}",
            String::from_utf8_lossy(&output.stderr).trim(),
            export_path.display()
        ))
    }
}

async fn gc_old_sessions(state: &State<'_, GuiState>, days: u64) -> Result<String, String> {
    let session_dir = state
        .session_dir
        .as_ref()
        .ok_or("No session directory configured")?;
    let cutoff = SystemTime::now() - Duration::from_secs(days * 86_400);
    let mut removed = 0usize;
    for entry in std::fs::read_dir(session_dir)
        .map_err(|e| e.to_string())?
        .flatten()
    {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        if modified < cutoff && move_to_trash(&path).is_ok() {
            removed += 1;
        }
    }
    Ok(format!(
        "GC: moved {removed} session files older than {days} days to trash"
    ))
}

async fn export_active_session(state: &State<'_, GuiState>, path: &str) -> Result<(), String> {
    let agent = active_agent(state).await?;
    let manager = agent.session_manager().await;
    let entries = manager.entries();
    drop(manager);

    let mut lines = Vec::with_capacity(entries.len());
    for entry in &entries {
        lines.push(serde_json::to_string(entry).map_err(|e| e.to_string())?);
    }
    std::fs::write(path, lines.join("\n") + "\n").map_err(|e| e.to_string())
}

fn session_entry_summary(entry: &rozsa_app::session::manager::SessionEntry) -> String {
    match entry {
        rozsa_app::session::manager::SessionEntry::Message(message_entry) => {
            message_summary(&message_entry.message)
        }
        rozsa_app::session::manager::SessionEntry::ThinkingLevelChange(entry) => {
            format!("thinking_change {}", entry.thinking_level)
        }
        rozsa_app::session::manager::SessionEntry::ModelChange(entry) => {
            format!("model_change {}/{}", entry.provider, entry.model_id)
        }
        rozsa_app::session::manager::SessionEntry::Compaction(entry) => {
            format!("compaction {}", truncate_chars(&entry.summary, 80))
        }
        rozsa_app::session::manager::SessionEntry::Custom(entry) => {
            format!("custom {}", entry.custom_type)
        }
        rozsa_app::session::manager::SessionEntry::Label(entry) => {
            format!("label {}", entry.label.clone().unwrap_or_default())
        }
        rozsa_app::session::manager::SessionEntry::SessionInfo(entry) => {
            format!("session_info {}", entry.name.clone().unwrap_or_default())
        }
    }
}

fn message_summary(message: &Message) -> String {
    match message {
        Message::User(user) => format!("user {}", truncate_chars(&user.content.text(), 80)),
        Message::Assistant(assistant) => {
            let text = text_from_blocks(&assistant.content);
            if text.is_empty() {
                "assistant (tool calls)".to_string()
            } else {
                format!("assistant {}", truncate_chars(&text, 80))
            }
        }
        Message::ToolResult(result) => format!(
            "tool_result [{}] {}",
            result.tool_name,
            truncate_chars(&text_from_blocks(&result.content), 80)
        ),
    }
}

fn text_from_blocks(blocks: &[ContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn truncate_chars(text: &str, max: usize) -> String {
    let mut out = String::new();
    for (idx, ch) in text.chars().enumerate() {
        if idx >= max {
            out.push_str("...");
            break;
        }
        out.push(ch);
    }
    out
}

fn move_to_trash(path: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "tell application \"Finder\" to delete POSIX file \"{}\"",
            path.to_string_lossy().replace('"', "\\\"")
        );
        let status = Command::new("osascript")
            .args(["-e", &script])
            .status()
            .map_err(|e| e.to_string())?;
        return status
            .success()
            .then_some(())
            .ok_or_else(|| format!("Failed to move {} to trash", path.display()));
    }
    #[cfg(target_os = "windows")]
    {
        let script = format!(
            "Add-Type -AssemblyName Microsoft.VisualBasic; [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile('{}', 'OnlyErrorDialogs', 'SendToRecycleBin')",
            path.to_string_lossy().replace('\'', "''")
        );
        let status = Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .status()
            .map_err(|e| e.to_string())?;
        return status
            .success()
            .then_some(())
            .ok_or_else(|| format!("Failed to move {} to recycle bin", path.display()));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        for program in ["gio", "trash-put"] {
            let status = if program == "gio" {
                Command::new(program)
                    .args(["trash", &path.to_string_lossy()])
                    .status()
            } else {
                Command::new(program).arg(path).status()
            };
            if status.map(|status| status.success()).unwrap_or(false) {
                return Ok(());
            }
        }
        Err(format!(
            "No supported trash command found for {}. Install gio or trash-cli.",
            path.display()
        ))
    }
}

async fn persist_settings(state: &State<'_, GuiState>) {
    if let Some(ref path) = state.global_settings_path {
        let s = state.runtime_settings.lock().await;
        if let Ok(mut updated) = serde_json::to_value(&*s) {
            let existing = std::fs::read_to_string(path)
                .ok()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
                .unwrap_or_else(|| serde_json::json!({}));
            if let (Some(updated), Some(mut existing)) =
                (updated.as_object_mut(), existing.as_object().cloned())
            {
                // permission is deliberately excluded: global permission rules
                // are manual-only and project Trust is persisted separately.
                updated.remove("permission");
                updated.remove("permissions");
                for (key, value) in updated {
                    existing.insert(key.clone(), value.clone());
                }
                if let Ok(json) = serde_json::to_string_pretty(&existing) {
                    let _ = std::fs::write(path, json);
                }
            }
        }
    }
}

async fn append_prompt_error(
    tab_idx: usize,
    agent: &rozsa_app::agent_session::AgentSession,
    tabs_ref: &std::sync::Arc<tokio::sync::Mutex<Vec<SessionTab>>>,
    shared: &std::sync::Arc<crate::state::SharedResources>,
    app: &AppHandle,
    error: String,
) {
    let model = agent.model().await;
    let error_message = Message::Assistant(AssistantMessage {
        content: Vec::new(),
        api: model.api.clone(),
        provider: model.provider.clone(),
        model: model.id.clone(),
        response_model: None,
        response_id: None,
        usage: Usage::default(),
        stop_reason: StopReason::Error,
        error_message: Some(error),
        timestamp: current_timestamp_ms(),
    });

    let mut tabs = tabs_ref.lock().await;
    if let Some(SessionTab::Active { path, live, .. }) = tabs.get_mut(tab_idx) {
        if let Ok(messages) = load_session_messages(path) {
            live.messages = messages;
        }
        live.messages
            .push(AgentMessage::standard(error_message.clone()));
        live.is_streaming = !live.queued_messages.is_empty();
        live.turn_base = live.messages.len();

        if let Ok(mut manager) = SessionManager::open(path) {
            let _ = manager.append_message(error_message);
        }

        let snapshot = UiSnapshot::from_tab(&tabs[tab_idx], shared);
        let _ = crate::events::emit_main(&app, "ui-state", &snapshot);
    }
}

async fn append_prompt_error_for_session(
    session_id: &str,
    agent: &rozsa_app::agent_session::AgentSession,
    tabs_ref: &std::sync::Arc<tokio::sync::Mutex<Vec<SessionTab>>>,
    shared: &std::sync::Arc<crate::state::SharedResources>,
    app: &AppHandle,
    error: String,
) {
    let tab_index = {
        let tabs = tabs_ref.lock().await;
        find_tab_index_by_session(&tabs, session_id)
    };
    if let Some(tab_index) = tab_index {
        append_prompt_error(tab_index, agent, tabs_ref, shared, app, error).await;
    }
}

fn current_timestamp_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_millis(0))
        .as_millis() as i64
}

fn models_dir() -> Result<std::path::PathBuf, String> {
    dirs::home_dir()
        .map(|home| home.join(".rozsa").join("models"))
        .ok_or_else(|| "Cannot determine home directory".to_string())
}

fn open_url(url: &str) -> bool {
    #[cfg(target_os = "macos")]
    let result = Command::new("open").arg(url).spawn();

    #[cfg(target_os = "windows")]
    let result = Command::new("cmd").args(["/C", "start", "", url]).spawn();

    #[cfg(all(unix, not(target_os = "macos")))]
    let result = Command::new("xdg-open").arg(url).spawn();

    result.is_ok()
}

fn ensure_codex_oauth_models_config(models_dir: &std::path::Path) -> Result<(), String> {
    let config_path = models_dir.join("codex-oauth.json");
    if config_path.exists() {
        return Ok(());
    }

    let default_config = serde_json::json!({
        "_fallback_source": "codex-rs/models-manager/models.json",
        "_fallback_version": 2,
        "providers": {
            "codex-oauth": {
                "baseUrl": "https://api.openai.com/v1",
                "api": "openai-responses",
                "authHeader": true,
                "models": [
                    codex_oauth_model("gpt-5.5", "GPT-5.5"),
                    codex_oauth_model("gpt-5.4", "gpt-5.4"),
                    codex_oauth_model("gpt-5.4-mini", "GPT-5.4-Mini"),
                    codex_oauth_model("gpt-5.3-codex", "gpt-5.3-codex"),
                    codex_oauth_model("gpt-5.2", "gpt-5.2")
                ]
            }
        }
    });
    let content = serde_json::to_string_pretty(&default_config)
        .map_err(|e| format!("Failed to serialize codex-oauth models config: {e}"))?;
    std::fs::write(&config_path, content)
        .map_err(|e| format!("Failed to write {}: {e}", config_path.display()))
}

fn codex_oauth_model(id: &str, name: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "name": name,
        "contextWindow": 272000,
        "maxTokens": 136000,
        "reasoning": true,
        "input": ["text", "image"],
        "cost": { "input": 0.0, "output": 0.0, "cacheRead": 0.0, "cacheWrite": 0.0 }
    })
}

fn load_session_messages(path: &str) -> Result<Vec<AgentMessage>, String> {
    let mgr = SessionManager::open(path).map_err(|e| e.to_string())?;
    let entries = mgr.entries();
    Ok(entries
        .into_iter()
        .filter_map(|entry| {
            if let rozsa_app::session::manager::SessionEntry::Message(msg_entry) = entry {
                Some(AgentMessage::standard(msg_entry.message))
            } else {
                None
            }
        })
        .collect())
}

fn load_session_summary(path: &str) -> Option<crate::turn_diff::TurnActivity> {
    SessionManager::open(path)
        .ok()
        .and_then(|manager| crate::turn_diff::latest_persisted_summary(&manager))
}

async fn activate_session(
    path: &str,
    state: &State<'_, GuiState>,
) -> Result<rozsa_app::agent_session::AgentSession, String> {
    state.shared.restore_agent(Path::new(path)).await
}

// --- 序列化类型 ---

#[derive(serde::Serialize)]
pub struct SessionListEntry {
    pub id: String,
    pub path: String,
    pub name: String,
    pub modified: String,
    pub message_count: u32,
}

#[derive(serde::Serialize)]
pub struct ModelListEntry {
    pub id: String,
    pub name: String,
    pub provider: String,
}

#[derive(serde::Serialize)]
pub struct SettingsSnapshot {
    pub permission_mode: String,
    pub thinking_level: String,
    pub model_id: String,
    pub model_name: String,
    pub model_provider: String,
    pub auto_approve_patterns: Vec<String>,
    pub allowed_tools: Vec<String>,
    pub block_images: bool,
    pub hide_thinking: bool,
    pub transport: String,
    pub auto_compact: bool,
    pub auto_session_naming: bool,
    pub steering_mode: String,
    pub follow_up_mode: String,
    pub running_send_mode: String,
    pub appearance: AppearanceSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppearanceSnapshot {
    pub theme_mode: String,
    pub font_size: u8,
    pub light_theme: String,
    pub dark_theme: String,
    pub is_macos: bool,
}
