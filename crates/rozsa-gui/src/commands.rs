// File: commands.rs
//
// Tauri IPC 命令。多会话架构：操作都针对当前活跃 tab。

use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use rozsa_app::agent_session::AgentSession;
use rozsa_app::permissions::PermissionResponse;
use rozsa_app::session::manager::SessionManager;
use rozsa_core::messages::AgentMessage;
use rozsa_model::types::{
    AssistantMessage, ContentBlock, Message, StopReason, ThinkingLevel, Usage,
};

use crate::state::{GuiState, SessionTab, UiSnapshot};

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
    let agent = match tab {
        SessionTab::Active { agent, .. } => agent.clone(),
        SessionTab::Loaded { path, messages } => {
            // 升级为 Active：创建 AgentSession
            let agent = activate_session(path, messages.clone(), &state).await?;
            let path_owned = path.clone();
            let agent_arc = std::sync::Arc::new(agent);
            *tab = SessionTab::Active {
                path: path_owned,
                agent: agent_arc.clone(),
                live: crate::state::LiveState {
                    messages: std::mem::take(messages),
                    ..Default::default()
                },
            };
            // 启动事件转发
            drop(tabs);
            crate::events::spawn_event_forwarder_for_tab(app.clone(), idx, state.inner().clone());
            agent_arc
        }
        SessionTab::Idle { path, .. } => {
            // 从 Idle 直接激活（加载历史 + 创建 agent）
            let path_owned = path.clone();
            let messages = load_session_messages(&path_owned)?;
            let agent = activate_session(&path_owned, messages.clone(), &state).await?;
            let agent_arc = std::sync::Arc::new(agent);
            *tab = SessionTab::Active {
                path: path_owned,
                agent: agent_arc.clone(),
                live: crate::state::LiveState {
                    messages,
                    ..Default::default()
                },
            };
            drop(tabs);
            crate::events::spawn_event_forwarder_for_tab(app.clone(), idx, state.inner().clone());
            agent_arc
        }
    };

    let expansion = crate::file_refs::expand_file_references(&message, &state.shared.cwd);

    // 发送消息（后台执行，不阻塞 IPC 返回）
    let shared = state.shared.clone();
    let tabs_ref = state.tabs.clone();
    let active_tab_ref = state.active_tab.clone();
    tokio::spawn(async move {
        if let Err(e) = agent
            .prompt_with_prefix_blocks(&message, expansion.blocks, expansion.display_text)
            .await
        {
            append_prompt_error(idx, &agent, &tabs_ref, &shared, &app, e.to_string()).await;
        }
        // 完成后推送最终状态
        let current_idx = *active_tab_ref.lock().await;
        if current_idx == idx {
            let tabs = tabs_ref.lock().await;
            if let Some(tab) = tabs.get(idx) {
                let snapshot = UiSnapshot::from_tab(tab, &shared);
                let _ = app.emit("ui-state", &snapshot);
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
}

#[tauri::command]
pub async fn autocomplete_input(
    state: State<'_, GuiState>,
    text: String,
    cursor: usize,
) -> Result<AutocompleteResponse, String> {
    let cursor = cursor.min(text.len());
    let head = &text[..cursor];

    if let Some(prefix) = parse_slash_completion_prefix(head) {
        use rozsa_app::slash_commands::{
            AutocompleteEngine, SlashCommandInfo, SlashCommandSource, BUILTIN_SLASH_COMMANDS,
        };

        let prefix_lower = prefix.to_ascii_lowercase();
        let mut dynamic = Vec::new();

        if let Ok(agent) = active_agent(&state).await {
            for skill in agent.skill_registry().list() {
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
        }

        let items = AutocompleteEngine::with_dynamic(dynamic)
            .complete(head, cursor)
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
        });
    }

    Ok(AutocompleteResponse {
        prefix: String::new(),
        items: Vec::new(),
        valid_match: false,
    })
}

#[tauri::command]
pub async fn pick_attachment(mode: String) -> Result<Option<String>, String> {
    pick_attachment_path(AttachmentPickMode::parse(&mode)?)
}

#[tauri::command]
pub async fn dispatch_slash_command(
    state: State<'_, GuiState>,
    app: AppHandle,
    text: String,
) -> Result<SlashCommandResult, String> {
    let trimmed = text.trim();
    let Some(rest) = trimmed.strip_prefix('/') else {
        return Ok(SlashCommandResult {
            handled: false,
            action: None,
            value: None,
        });
    };
    let rest = rest.trim();
    if rest.is_empty() {
        return Ok(SlashCommandResult {
            handled: false,
            action: None,
            value: None,
        });
    }

    let (cmd, args) = match rest.split_once(char::is_whitespace) {
        Some((cmd, args)) => (cmd.to_ascii_lowercase(), args.trim().to_string()),
        None => (rest.to_ascii_lowercase(), String::new()),
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
            emit_info(&app, &format!("Thinking: {}", format!("{level:?}").to_lowercase()));
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
            let snapshot = get_rate_limits().await?;
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
                agent
                    .session_manager()
                    .await
                    .append_session_info(Some(args.clone()))
                    .map_err(|e| e.to_string())?;
                emit_info(&app, &format!("Session name set: {args}"));
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
                    &format!("Unknown LSP mode '{args}'. Options: agent_end | edit_write | disabled"),
                );
            }
        }
        "main" | "subagents" | "tree" | "graph" | "fork" | "clone" | "import" | "share"
        | "reload" | "changelog" | "gc" => {
            emit_info(
                &app,
                &format!("/{cmd} is recognized but not supported by the GUI yet"),
            );
        }
        "quit" => app.exit(0),
        _ => {
            let agent = active_agent(&state).await?;
            if let Some(prompt) = normalize_skill_command(&agent, &cmd, &args) {
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
    if let Some(SessionTab::Active { agent, .. }) = tabs.get(idx) {
        agent.abort().await;
    }
    Ok(())
}

// --- 状态查询 ---

#[tauri::command]
pub async fn get_state(state: State<'_, GuiState>) -> Result<UiSnapshot, String> {
    let idx = *state.active_tab.lock().await;
    let tabs = state.tabs.lock().await;
    let tab = tabs.get(idx).ok_or("No active tab")?;
    Ok(UiSnapshot::from_tab(tab, &state.shared))
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
        let _ = app.emit("ui-state", &snapshot);
    }

    Ok(())
}

#[tauri::command]
pub async fn new_session(state: State<'_, GuiState>, app: AppHandle) -> Result<String, String> {
    let session_dir = state
        .session_dir
        .as_ref()
        .ok_or("No session directory configured")?;

    let new_id = uuid::Uuid::new_v4().to_string();
    let cwd = state.shared.cwd.to_string_lossy().to_string();
    let path = session_dir.join(format!("{new_id}.jsonl"));

    let _manager =
        SessionManager::create(&path, new_id.clone(), cwd, None).map_err(|e| e.to_string())?;

    let path_str = path.to_string_lossy().to_string();

    // 新建一个 Loaded tab（空消息，等用户发消息时才激活 agent）
    let mut tabs = state.tabs.lock().await;
    tabs.push(SessionTab::Loaded {
        path: path_str.clone(),
        messages: vec![],
    });
    let new_idx = tabs.len() - 1;
    drop(tabs);

    *state.active_tab.lock().await = new_idx;

    // 推送空状态
    let tabs = state.tabs.lock().await;
    if let Some(tab) = tabs.get(new_idx) {
        let snapshot = UiSnapshot::from_tab(tab, &state.shared);
        let _ = app.emit("ui-state", &snapshot);
    }

    Ok(new_id)
}

// --- 权限 ---

#[tauri::command]
pub async fn respond_permission(
    state: State<'_, GuiState>,
    id: String,
    choice: String,
    trust_key: Option<String>,
) -> Result<(), String> {
    let approvals = state
        .pending_approvals
        .as_ref()
        .ok_or("Permission system not initialized")?;

    let (_, sender) = approvals
        .remove(&id)
        .ok_or_else(|| format!("No pending approval: {id}"))?;

    let response = match choice.as_str() {
        "allow" => PermissionResponse::Allow,
        "allow-session" => {
            if let Some(key) = trust_key {
                PermissionResponse::AllowSession { trust_key: key }
            } else {
                PermissionResponse::Allow
            }
        }
        _ => PermissionResponse::Deny,
    };

    sender
        .send(response)
        .map_err(|_| "Failed to send permission response".to_string())
}

// --- 设置 ---

#[tauri::command]
pub async fn get_settings(state: State<'_, GuiState>) -> Result<SettingsSnapshot, String> {
    let rt = state.runtime_settings.lock().await;
    let model = state.shared.model.lock().await;
    let thinking = state.shared.thinking_level.lock().await;

    Ok(SettingsSnapshot {
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
        steering_mode: rt.steering_mode.clone(),
        follow_up_mode: rt.follow_up_mode.clone(),
    })
}

#[tauri::command]
pub async fn update_setting(
    state: State<'_, GuiState>,
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
        _ => Err(format!("Unknown setting: {key}")),
    }
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
                let _ = app.emit("notification", message);
            }
            OAuthFlowEvent::Progress { message } => {
                let _ = app.emit("notification", message);
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
pub async fn get_rate_limits() -> Result<rozsa_model::rate_limit::RateLimitSnapshot, String> {
    rozsa_app::rate_limit::get_rate_limits()
        .await
        .map_err(|e| e.to_string())
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
    _state: State<'_, GuiState>,
    path: String,
    name: String,
) -> Result<(), String> {
    SessionManager::rename(&path, if name.is_empty() { None } else { Some(name) })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_session(state: State<'_, GuiState>, path: String) -> Result<(), String> {
    // 从 tabs 移除该 session
    let mut tabs = state.tabs.lock().await;
    tabs.retain(|t| t.path() != path);
    drop(tabs);
    SessionManager::delete(&path).map_err(|e| e.to_string())
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
            let _ = app.emit("error", e.to_string());
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
    let _ = app.emit("notification", message.to_string());
}

fn parse_slash_completion_prefix(head: &str) -> Option<String> {
    let trimmed = head.trim_start();
    if !trimmed.starts_with('/') || trimmed[1..].contains(char::is_whitespace) {
        return None;
    }
    Some(trimmed[1..].to_ascii_lowercase())
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

fn normalize_skill_command(agent: &AgentSession, cmd: &str, args: &str) -> Option<String> {
    let skill_name = cmd.strip_prefix("skill:").unwrap_or(cmd);
    let exists = agent.skill_registry().find_by_name(skill_name).is_some();
    if !exists {
        return None;
    }
    if args.is_empty() {
        Some(format!("/skill:{skill_name}"))
    } else {
        Some(format!("/skill:{skill_name} {args}"))
    }
}

#[cfg(target_os = "macos")]
fn pick_attachment_path(mode: AttachmentPickMode) -> Result<Option<String>, String> {
    let choose_files = !matches!(mode, AttachmentPickMode::Directory);
    let choose_dirs = !matches!(mode, AttachmentPickMode::File);
    let script = format!(
        r#"
ObjC.import('AppKit');
const panel = $.NSOpenPanel.openPanel;
panel.canChooseFiles = {};
panel.canChooseDirectories = {};
panel.allowsMultipleSelection = false;
panel.resolvesAliases = true;
const result = panel.runModal();
if (result == $.NSModalResponseOK) {{
  ObjC.unwrap(panel.URLs.objectAtIndex(0).path);
}} else {{
  '';
}}
"#,
        choose_files, choose_dirs
    );
    let output = Command::new("osascript")
        .args(["-l", "JavaScript", "-e", &script])
        .output()
        .map_err(|e| format!("Failed to open attachment picker: {e}"))?;
    read_picker_output(output)
}

#[cfg(target_os = "windows")]
fn pick_attachment_path(mode: AttachmentPickMode) -> Result<Option<String>, String> {
    let script = match mode {
        AttachmentPickMode::Any | AttachmentPickMode::File => {
            r#"
Add-Type -AssemblyName System.Windows.Forms
$dialog = New-Object System.Windows.Forms.OpenFileDialog
$dialog.CheckFileExists = $true
$dialog.Multiselect = $false
$dialog.Title = 'Attach file'
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
  [Console]::OutputEncoding = [System.Text.Encoding]::UTF8
  Write-Output $dialog.FileName
}
"#
        }
        AttachmentPickMode::Directory => {
            r#"
Add-Type -AssemblyName System.Windows.Forms
$dialog = New-Object System.Windows.Forms.FolderBrowserDialog
$dialog.Description = 'Attach directory'
$dialog.ShowNewFolderButton = $false
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
  [Console]::OutputEncoding = [System.Text.Encoding]::UTF8
  Write-Output $dialog.SelectedPath
}
"#
        }
    };
    let output = Command::new("powershell")
        .args(["-NoProfile", "-STA", "-Command", script])
        .output()
        .map_err(|e| format!("Failed to open attachment picker: {e}"))?;
    read_picker_output(output)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn pick_attachment_path(mode: AttachmentPickMode) -> Result<Option<String>, String> {
    let directory = matches!(mode, AttachmentPickMode::Directory);
    let mut candidates: Vec<(&str, Vec<&str>)> = Vec::new();
    if directory {
        candidates.push(("zenity", vec!["--file-selection", "--directory"]));
        candidates.push(("kdialog", vec!["--getexistingdirectory"]));
    } else {
        candidates.push(("zenity", vec!["--file-selection"]));
        candidates.push(("kdialog", vec!["--getopenfilename"]));
    }

    let mut last_error = String::new();
    for (program, args) in candidates {
        match Command::new(program).args(args).output() {
            Ok(output) if output.status.success() => return read_picker_output(output),
            Ok(output) if output.status.code() == Some(1) => return Ok(None),
            Ok(output) => {
                last_error = String::from_utf8_lossy(&output.stderr).trim().to_string();
            }
            Err(e) => {
                last_error = e.to_string();
            }
        }
    }
    Err(if last_error.is_empty() {
        "No supported Linux attachment picker found. Install zenity or kdialog.".to_string()
    } else {
        format!("Attachment picker failed: {last_error}")
    })
}

fn read_picker_output(output: std::process::Output) -> Result<Option<String>, String> {
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            "Attachment picker failed.".to_string()
        } else {
            format!("Attachment picker failed: {stderr}")
        });
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok((!path.is_empty()).then_some(path))
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

async fn create_new_session(state: &State<'_, GuiState>, app: &AppHandle) -> Result<String, String> {
    let session_dir = state
        .session_dir
        .as_ref()
        .ok_or("No session directory configured")?;

    let new_id = uuid::Uuid::new_v4().to_string();
    let cwd = state.shared.cwd.to_string_lossy().to_string();
    let path = session_dir.join(format!("{new_id}.jsonl"));

    let _manager =
        SessionManager::create(&path, new_id.clone(), cwd, None).map_err(|e| e.to_string())?;

    let path_str = path.to_string_lossy().to_string();
    let mut tabs = state.tabs.lock().await;
    tabs.push(SessionTab::Loaded {
        path: path_str,
        messages: vec![],
    });
    let new_idx = tabs.len() - 1;
    drop(tabs);

    *state.active_tab.lock().await = new_idx;
    let tabs = state.tabs.lock().await;
    if let Some(tab) = tabs.get(new_idx) {
        let snapshot = UiSnapshot::from_tab(tab, &state.shared);
        let _ = app.emit("ui-state", &snapshot);
    }

    Ok(new_id)
}

async fn switch_model_reference(state: &State<'_, GuiState>, reference: &str) -> Result<(), String> {
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
            .find(|m| {
                m.id == reference || m.id.contains(reference) || m.id.ends_with(reference)
            })
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

async fn persist_settings(state: &State<'_, GuiState>) {
    if let Some(ref path) = state.global_settings_path {
        let s = state.runtime_settings.lock().await;
        if let Ok(json) = serde_json::to_string_pretty(&*s) {
            let _ = std::fs::write(path, json);
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
        live.is_streaming = false;
        live.turn_base = live.messages.len();

        if let Ok(mut manager) = SessionManager::open(path) {
            let _ = manager.append_message(error_message);
        }

        let snapshot = UiSnapshot::from_tab(&tabs[tab_idx], shared);
        let _ = app.emit("ui-state", &snapshot);
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

async fn activate_session(
    path: &str,
    _messages: Vec<AgentMessage>,
    state: &State<'_, GuiState>,
) -> Result<rozsa_app::agent_session::AgentSession, String> {
    use rozsa_app::agent_session::AgentSessionConfig;

    let model = state.shared.model.lock().await.clone();
    let thinking = *state.shared.thinking_level.lock().await;
    let session_manager = SessionManager::open(path).map_err(|e| e.to_string())?;

    let config = AgentSessionConfig {
        model,
        thinking_level: thinking,
        system_prompt: state.shared.system_prompt.clone(),
        cwd: state.shared.cwd.clone(),
        session_manager,
        settings_manager: state.shared.settings_manager.clone(),
        resources: state.shared.resources.clone(),
        pre_tool_use: None, // TODO: 注入权限 hook
    };

    let session = rozsa_app::agent_session::AgentSession::new(config);
    session.register_default_tools(&state.shared.cwd).await;
    Ok(session)
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
    pub steering_mode: String,
    pub follow_up_mode: String,
}
