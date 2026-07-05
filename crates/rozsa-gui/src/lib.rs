// File: lib.rs
//
// rozsa-gui 入口。多会话架构：每个 session tab 有独立的 agent backend（懒加载）。

mod commands;
mod events;
pub mod state;

use std::path::PathBuf;
use std::sync::Arc;

use rozsa_app::agent_session::AgentSession;
use rozsa_app::model_registry::ModelRegistry;
use rozsa_app::permissions::{ApprovalInfo, PendingApprovals};
use tauri::Manager;

use state::{GuiState, SessionTab, SharedResources};

pub struct GuiConfig {
    pub session: AgentSession,
    pub model_registry: Option<Arc<ModelRegistry>>,
    pub session_dir: Option<PathBuf>,
    pub global_settings_path: Option<PathBuf>,
    pub pending_approvals: Option<PendingApprovals>,
    pub permission_request_rx:
        Option<tokio::sync::mpsc::UnboundedReceiver<(String, ApprovalInfo)>>,
    pub system_prompt: String,
    pub resources: rozsa_app::resources::LoadedResources,
}

pub async fn run(config: GuiConfig) -> Result<(), Box<dyn std::error::Error>> {
    let model = config.session.model().await;
    let thinking = config.session.thinking_level().await;
    let cwd = config.session.cwd().to_path_buf();
    let settings_manager = config.session.settings_manager().clone();
    let runtime_settings = settings_manager.resolved().clone();

    // 初始 session 的 path
    let initial_path = config.session.session_manager().await.session_file().to_string_lossy().to_string();

    // 共享资源（创建新 agent backend 时复用）
    let shared = Arc::new(SharedResources {
        cwd: cwd.clone(),
        settings_manager,
        resources: config.resources,
        system_prompt: config.system_prompt,
        model: tokio::sync::Mutex::new(model),
        thinking_level: tokio::sync::Mutex::new(thinking),
        pre_tool_use: None,
    });

    // 初始 tab — 已有一个 active session（CLI 构造的）
    let initial_tab = SessionTab::Active {
        path: initial_path.clone(),
        agent: Arc::new(config.session),
        live: state::LiveState::default(),
    };

    let gui_state = GuiState {
        tabs: Arc::new(tokio::sync::Mutex::new(vec![initial_tab])),
        active_tab: Arc::new(tokio::sync::Mutex::new(0)),
        shared,
        model_registry: config.model_registry,
        session_dir: config.session_dir,
        pending_approvals: config.pending_approvals,
        global_settings_path: config.global_settings_path,
        runtime_settings: Arc::new(tokio::sync::Mutex::new(runtime_settings)),
    };

    let perm_rx = config.permission_request_rx;

    tauri::Builder::default()
        .manage(gui_state)
        .invoke_handler(tauri::generate_handler![
            commands::send_message,
            commands::abort,
            commands::get_state,
            commands::get_sessions,
            commands::switch_session,
            commands::new_session,
            commands::respond_permission,
            commands::get_settings,
            commands::update_setting,
            commands::list_models,
            commands::switch_model,
            commands::compact,
            commands::rename_session,
            commands::delete_session,
            commands::run_bash,
        ])
        .setup(move |app| {
            if let Some(window) = app.get_webview_window("main") {
                window.open_devtools();
            }

            let handle = app.handle().clone();

            // 初始 session 的事件转发
            let gui = app.state::<GuiState>();
            events::spawn_event_forwarder_for_tab(handle.clone(), 0, gui.inner().clone());

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
