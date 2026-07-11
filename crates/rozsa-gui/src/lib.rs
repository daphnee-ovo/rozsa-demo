// File: lib.rs
//
// rozsa-gui 入口。多会话架构：每个 session tab 有独立的 agent backend（懒加载）。

mod commands;
pub mod events;
pub mod file_refs;
pub mod git_diff;
pub mod state;
pub mod turn_diff;

pub use git_diff::read_workspace_diff;

use std::path::PathBuf;
use std::sync::Arc;

use rozsa_app::model_registry::ModelRegistry;
use rozsa_app::permissions::PendingApprovals;
use rozsa_app::settings::SettingsManager;
use rozsa_model::types::{Model, ThinkingLevel};
use tauri::Manager;

use state::{
    GuiState, PermissionRequest, PreToolUseHookFactory, SessionTab, SharedResources,
    deny_pending_approvals,
};

pub struct GuiConfig {
    /// Bootstrap data only. GUI owns all AgentSession construction.
    pub initial_parent_session: Option<String>,
    pub model: Model,
    pub thinking_level: ThinkingLevel,
    pub cwd: PathBuf,
    pub settings_manager: SettingsManager,
    pub model_registry: Option<Arc<ModelRegistry>>,
    pub session_dir: PathBuf,
    pub global_settings_path: Option<PathBuf>,
    pub pending_approvals: Option<PendingApprovals>,
    pub permission_controller: Arc<rozsa_app::permissions::PermissionController>,
    pub permission_request_rx: Option<tokio::sync::mpsc::UnboundedReceiver<PermissionRequest>>,
    pub pre_tool_use_factory: Option<PreToolUseHookFactory>,
    pub system_prompt: String,
    pub resources: rozsa_app::resources::LoadedResources,
}

pub async fn run(config: GuiConfig) -> Result<(), Box<dyn std::error::Error>> {
    let runtime_settings = config.settings_manager.resolved().clone();
    // 共享资源（创建新 agent backend 时复用）
    let shared = Arc::new(SharedResources {
        cwd: config.cwd,
        settings_manager: config.settings_manager,
        resources: config.resources,
        system_prompt: config.system_prompt,
        model: tokio::sync::Mutex::new(config.model),
        thinking_level: tokio::sync::Mutex::new(config.thinking_level),
        pre_tool_use_factory: config.pre_tool_use_factory,
        model_stream: None,
    });
    let initial_session_dir = config.session_dir.clone();
    let initial = shared
        .create_new_agent(&initial_session_dir, config.initial_parent_session)
        .await
        .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;
    let initial_session_id = initial.id.clone();

    // 初始 tab 与后续新建 tab 都由同一个 GUI factory 创建。
    let initial_tab = SessionTab::Active {
        path: initial.path,
        agent: Arc::new(initial.agent),
        live: state::LiveState::default(),
    };

    let gui_state = GuiState {
        tabs: Arc::new(tokio::sync::Mutex::new(vec![initial_tab])),
        active_tab: Arc::new(tokio::sync::Mutex::new(0)),
        shared,
        model_registry: config.model_registry,
        session_dir: Some(config.session_dir),
        pending_approvals: config.pending_approvals,
        pending_permission_contexts: Arc::new(dashmap::DashMap::new()),
        permission_controller: config.permission_controller,
        global_settings_path: config.global_settings_path,
        runtime_settings: Arc::new(tokio::sync::Mutex::new(runtime_settings)),
    };

    let perm_rx = config.permission_request_rx;

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(gui_state)
        .invoke_handler(tauri::generate_handler![
            commands::send_message,
            commands::abort,
            commands::send_running_message,
            commands::get_state,
            commands::get_file_diff,
            commands::get_fork_points,
            commands::fork_session,
            commands::get_subagents,
            commands::get_sessions,
            commands::switch_session,
            commands::new_session,
            commands::respond_permission,
            commands::prepare_permission,
            commands::get_settings,
            commands::update_setting,
            commands::list_models,
            commands::switch_model,
            commands::auth_login,
            commands::auth_logout,
            commands::get_rate_limits,
            commands::dispatch_slash_command,
            commands::compact,
            commands::rename_session,
            commands::delete_session,
            commands::run_bash,
            commands::autocomplete_input,
            commands::pick_attachment,
        ])
        .setup(move |app| {
            if let Some(window) = app.get_webview_window("main") {
                window.open_devtools();
                let pending_approvals = app.state::<GuiState>().pending_approvals.clone();
                window.on_window_event(move |event| {
                    if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                        if let Some(approvals) = &pending_approvals {
                            deny_pending_approvals(approvals, None);
                        }
                    }
                });
            }

            let handle = app.handle().clone();

            let gui = app.state::<GuiState>();
            events::spawn_event_forwarder_for_session(
                handle.clone(),
                initial_session_id.clone(),
                gui.inner().clone(),
            );

            // 权限请求监听
            if let Some(rx) = perm_rx {
                events::spawn_permission_listener(handle, rx);
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run tauri application");

    Ok(())
}
