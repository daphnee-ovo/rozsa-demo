// File: commands.rs
//
// Tauri IPC 命令。多会话架构：操作都针对当前活跃 tab。

use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter, State};

use rozsa_app::permissions::PermissionResponse;
use rozsa_app::session::manager::SessionManager;
use rozsa_core::messages::AgentMessage;
use rozsa_model::types::{AssistantMessage, Message, StopReason, Usage};

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

    // 发送消息（后台执行，不阻塞 IPC 返回）
    let shared = state.shared.clone();
    let tabs_ref = state.tabs.clone();
    let active_tab_ref = state.active_tab.clone();
    tokio::spawn(async move {
        if let Err(e) = agent.prompt(&message).await {
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
