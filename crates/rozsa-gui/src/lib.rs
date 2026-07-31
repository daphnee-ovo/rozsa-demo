// FrameworkTree
// lib.rs
// ├── mod commands
// ├── mod dev_flow
// ├── mod events
// ├── mod file_refs
// ├── mod git_diff
// ├── mod inspector
// ├── mod key_bindings
// ├── mod native_split_view
// ├── mod native_titlebar
// ├── mod notifications
// ├── mod scene_router
// ├── mod state
// ├── mod turn_diff
// ├── struct GuiConfig
// ├── native_sidebar_collapsed()
// ├── set_native_sidebar_overlay_visible()
// ├── native_sidebar_overlay_width()
// └── run()

// File: lib.rs
//
// rozsa-gui 入口。多会话架构：每个 session tab 有独立的 agent backend（懒加载）。

mod commands;
pub mod dev_flow;
pub mod events;
pub mod file_refs;
pub mod git_diff;
mod inspector;
pub mod key_bindings;
#[cfg(target_os = "macos")]
mod native_split_view;
#[cfg(target_os = "macos")]
mod native_titlebar;
pub mod notifications;
pub mod scene_router;
pub mod state;
pub mod turn_diff;

pub use git_diff::read_workspace_diff;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rozsa_app::config_paths::ConfigRoots;
use rozsa_app::model_registry::ModelRegistry;
use rozsa_app::permissions::PendingApprovals;
use rozsa_app::settings::SettingsManager;
use rozsa_app::tools::{AskUserQuestionRequest, AskUserQuestionRequestSender};
use rozsa_model::types::{Model, ThinkingEffort};
use tauri::{Emitter, Manager};

use state::{
    GuiState, PermissionRequest, PreToolUseHookFactory, SessionTab, SharedResources,
    cancel_pending_user_questions, deny_pending_approvals,
};

pub struct GuiConfig {
    /// Bootstrap data only. GUI owns all AgentSession construction.
    pub initial_parent_session: Option<String>,
    pub model: Model,
    pub thinking_effort: ThinkingEffort,
    pub cwd: PathBuf,
    pub config_roots: ConfigRoots,
    pub settings_manager: SettingsManager,
    pub model_registry: Option<Arc<ModelRegistry>>,
    pub session_dir: PathBuf,
    pub session_dirs: Vec<PathBuf>,
    pub global_settings_path: Option<PathBuf>,
    pub pending_approvals: Option<PendingApprovals>,
    pub permission_controller: Arc<rozsa_app::permissions::PermissionController>,
    pub permission_request_rx: Option<tokio::sync::mpsc::UnboundedReceiver<PermissionRequest>>,
    pub question_request_tx: Option<AskUserQuestionRequestSender>,
    pub question_request_rx: Option<tokio::sync::mpsc::UnboundedReceiver<AskUserQuestionRequest>>,
    pub pre_tool_use_factory: Option<PreToolUseHookFactory>,
    pub system_prompt: String,
    pub resources: rozsa_app::resources::LoadedResources,
}

#[tauri::command]
fn native_sidebar_collapsed() -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        native_split_view::is_sidebar_collapsed()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(false)
    }
}

#[tauri::command]
fn set_native_sidebar_overlay_visible(visible: bool) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        native_split_view::set_sidebar_overlay_visible(visible)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = visible;
        Ok(())
    }
}

#[tauri::command]
fn native_sidebar_overlay_width() -> Result<f64, String> {
    #[cfg(target_os = "macos")]
    {
        native_split_view::sidebar_overlay_width()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(0.0)
    }
}

pub async fn run(config: GuiConfig) -> Result<(), Box<dyn std::error::Error>> {
    let runtime_settings = config.settings_manager.resolved().clone();
    let initial_cwd = config.cwd.clone();
    // 共享资源（创建新 agent backend 时复用）
    let shared = Arc::new(SharedResources {
        cwd: config.cwd,
        settings_manager: config.settings_manager,
        resources: config.resources,
        system_prompt: config.system_prompt,
        model: tokio::sync::Mutex::new(config.model),
        thinking_effort: tokio::sync::Mutex::new(config.thinking_effort),
        pre_tool_use_factory: config.pre_tool_use_factory,
        question_request_tx: config.question_request_tx,
        model_stream: None,
    });
    let initial_session_dir = config.session_dir.clone();
    let initial = match config.initial_parent_session.as_deref() {
        Some(parent_session) => {
            shared
                .create_continued_agent(&initial_session_dir, Path::new(parent_session))
                .await
        }
        None => shared.create_new_agent(&initial_session_dir, None).await,
    }
    .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;
    let initial_session_id = initial.id.clone();
    let initial_messages = initial.agent.messages().await;
    let initial_dev_flow_settings = runtime_settings.dev_flow.clone();

    // 初始 tab 与后续新建 tab 都由同一个 GUI factory 创建。
    let initial_tab = SessionTab::Active {
        path: initial.path,
        agent: Arc::new(initial.agent),
        live: state::LiveState {
            messages: initial_messages,
            ..state::LiveState::default()
        },
    };

    let dev_flow = dev_flow::system_runtime(
        Arc::new(std::sync::Mutex::new(None)),
        Arc::new(rozsa_app::dev_flow::SystemProjectCommandRunner),
    );

    let gui_state = GuiState {
        scene_router: Arc::new(tokio::sync::Mutex::new(scene_router::SceneRouter::default())),
        tabs: Arc::new(tokio::sync::Mutex::new(vec![initial_tab])),
        active_tab: Arc::new(tokio::sync::Mutex::new(0)),
        shared,
        dev_flow,
        model_registry: config
            .model_registry
            .map(|registry| Arc::new(std::sync::RwLock::new((*registry).clone()))),
        session_dir: Some(config.session_dir),
        session_dirs: config.session_dirs,
        config_roots: config.config_roots,
        pending_approvals: config.pending_approvals,
        pending_permission_contexts: Arc::new(dashmap::DashMap::new()),
        pending_user_questions: Arc::new(dashmap::DashMap::new()),
        permission_controller: config.permission_controller,
        global_settings_path: config.global_settings_path,
        runtime_settings: Arc::new(tokio::sync::Mutex::new(runtime_settings)),
        quota_summary: Arc::new(tokio::sync::Mutex::new(None)),
    };

    let perm_rx = config.permission_request_rx;
    let question_rx = config.question_request_rx;

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(gui_state)
        .invoke_handler(tauri::generate_handler![
            commands::set_gui_scene,
            commands::gui_webview_ready,
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
            commands::respond_user_question,
            commands::prepare_permission,
            commands::get_settings,
            commands::update_setting,
            commands::get_dev_flow_settings,
            commands::set_dev_flow_enabled,
            commands::set_dev_flow_sidebar_status,
            commands::set_dev_flow_executable_path,
            commands::rescan_dev_flow,
            commands::get_capability_settings,
            commands::update_capability_setting,
            commands::get_permission_settings,
            commands::update_permission_mode,
            commands::update_permission_rules,
            commands::update_permission_rule_set,
            commands::get_key_bindings,
            commands::update_key_binding,
            commands::reset_key_binding,
            commands::list_themes,
            commands::get_theme,
            commands::save_theme,
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
            native_sidebar_collapsed,
            set_native_sidebar_overlay_visible,
            native_sidebar_overlay_width,
        ])
        .setup(move |app| {
            if let Some(window) = app.get_webview_window("main") {
                #[cfg(target_os = "macos")]
                {
                    let sidebar_url =
                        tauri::WebviewUrl::App(std::path::PathBuf::from("sidebar.html"));
                    let titlebar_window = window.clone();
                    let sidebar_event_handle = app.handle().clone();
                    let fullscreen_event_handle = app.handle().clone();
                    native_split_view::install(&window, sidebar_url, move |main_webview_raw| {
                        native_titlebar::install(
                            &titlebar_window,
                            move || {
                                match native_split_view::toggle_sidebar() {
                                    Ok(collapsed) => {
                                        if let Err(error) = sidebar_event_handle
                                            .emit("native-sidebar-state", collapsed)
                                        {
                                            eprintln!(
                                                "[rozsa-gui][native-titlebar] failed to emit native-sidebar-state={collapsed}: {error}"
                                            );
                                        }
                                        Some(collapsed)
                                    }
                                    Err(error) => {
                                        eprintln!(
                                            "[rozsa-gui][native-titlebar] sidebar toggle failed: {error}"
                                        );
                                        None
                                    }
                                }
                            },
                            || native_split_view::is_sidebar_collapsed().ok(),
                            move |fullscreen, transitioning| {
                                let payload = serde_json::json!({
                                    "fullscreen": fullscreen,
                                    "transitioning": transitioning,
                                });
                                match fullscreen_event_handle.emit("native-fullscreen", payload) {
                                    Ok(()) => eprintln!(
                                        "[rozsa-gui][native-titlebar] emitted native-fullscreen={fullscreen} transitioning={transitioning}"
                                    ),
                                    Err(error) => eprintln!(
                                        "[rozsa-gui][native-titlebar] failed to emit native-fullscreen={fullscreen} transitioning={transitioning}: {error}"
                                    ),
                                }
                            },
                        )?;
                        titlebar_window
                            .show()
                            .map_err(|error| format!("failed to show installed GUI window: {error}"))?;
                        if inspector::enabled() {
                            inspector::open_from_webview_raw(main_webview_raw)?;
                        }
                        Ok(())
                    })
                    .map_err(std::io::Error::other)?;
                }

                #[cfg(not(target_os = "macos"))]
                {
                    if inspector::enabled() {
                        inspector::open_in_separate_window(&window);
                    }
                    window.show().map_err(std::io::Error::other)?;
                }

                let pending_approvals = app.state::<GuiState>().pending_approvals.clone();
                let pending_user_questions =
                    app.state::<GuiState>().pending_user_questions.clone();
                window.on_window_event(move |event| {
                    if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                        #[cfg(target_os = "macos")]
                        if let Err(error) = inspector::teardown() {
                            eprintln!("[rozsa-gui][inspector] teardown failed: {error}");
                        }
                        #[cfg(target_os = "macos")]
                        if let Err(error) = native_titlebar::teardown() {
                            eprintln!("[rozsa-gui][native-titlebar] teardown failed: {error}");
                        }
                        #[cfg(target_os = "macos")]
                        if let Err(error) = native_split_view::teardown() {
                            eprintln!("[rozsa-gui][native-split] teardown failed: {error}");
                        }
                        if let Some(approvals) = &pending_approvals {
                            deny_pending_approvals(approvals, None);
                        }
                        cancel_pending_user_questions(&pending_user_questions, None);
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
            commands::spawn_codex_oauth_model_refresh(handle.clone(), gui.inner().clone());

            // 权限请求监听
            if let Some(rx) = perm_rx {
                events::spawn_permission_listener(handle, rx);
            }
            if let Some(rx) = question_rx {
                events::spawn_user_question_listener(app.handle().clone(), rx);
            }

            let dev_flow = app.state::<GuiState>().dev_flow.clone();
            dev_flow.attach_notifier(dev_flow::real_notifier(app.handle().clone()));
            tokio::spawn(async move {
                dev_flow
                    .switch_to_session(&initial_session_id, initial_cwd)
                    .await;
                if let Err(error) = dev_flow.reconfigure(&initial_dev_flow_settings).await {
                    tracing::warn!("dev-flow initial reconfigure failed: {error}");
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run tauri application");

    Ok(())
}
