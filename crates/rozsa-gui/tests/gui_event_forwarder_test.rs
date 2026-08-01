use std::sync::Arc;

use dashmap::DashMap;
use rozsa_app::agent_session::ModelStream;
use rozsa_app::permissions::{PermissionController, PermissionMode};
use rozsa_app::settings::SettingsManager;
use rozsa_gui::dev_flow::DevFlowRuntime;
use rozsa_gui::events::spawn_event_forwarder_for_session;
use rozsa_gui::state::{GuiState, LiveState, SessionTab, SharedResources};
use rozsa_model::event_stream::create_event_stream;
use rozsa_model::types::{
    Api, AssistantMessage, ContentBlock, Model, ModelCost, Provider, StopReason, StreamEvent,
    ThinkingEffort, ToolCall, Usage,
};
use tauri::{Event, Listener};
use tokio::sync::Mutex;

fn model() -> Model {
    Model {
        id: "scripted".to_string(),
        name: "Scripted".to_string(),
        api: Api::OpenAICompletions,
        provider: Provider::OpenAI,
        base_url: "http://127.0.0.1".to_string(),
        reasoning: false,
        input_modalities: vec![],
        cost: ModelCost {
            input: 0.0,
            output: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
        },
        context_window: 8_192,
        max_tokens: 1_024,
        thinking_effort_map: None,
        headers: None,
        compat: None,
    }
}

fn scripted_stream(path: String) -> ModelStream {
    let step = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    Arc::new(move |_model, _context, _options| {
        let message = if step.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
            AssistantMessage {
                content: vec![ContentBlock::ToolCall(ToolCall {
                    id: "read-1".to_string(),
                    name: "read".to_string(),
                    arguments: serde_json::json!({"file_path": path}),
                })],
                api: Api::OpenAICompletions,
                provider: Provider::OpenAI,
                model: "scripted".to_string(),
                response_model: None,
                response_id: None,
                usage: Usage::default(),
                stop_reason: StopReason::ToolUse,
                error_message: None,
                timestamp: 0,
            }
        } else {
            AssistantMessage {
                content: vec![ContentBlock::Text {
                    text: "done".to_string(),
                    signature: None,
                }],
                api: Api::OpenAICompletions,
                provider: Provider::OpenAI,
                model: "scripted".to_string(),
                response_model: None,
                response_id: None,
                usage: Usage::default(),
                stop_reason: StopReason::Stop,
                error_message: None,
                timestamp: 0,
            }
        };
        let (sender, stream) = create_event_stream();
        sender.push(StreamEvent::Done {
            reason: message.stop_reason,
            message,
        });
        stream
    })
}

#[tokio::test]
async fn forwards_ui_state_and_tool_events_with_the_source_session_id() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("src/lib.rs");
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(&source, "pub fn answer() -> u8 { 2 }").unwrap();
    let settings_manager =
        SettingsManager::load(temp.path().join("settings.json"), None, None).unwrap();
    let shared = Arc::new(SharedResources {
        cwd: temp.path().to_path_buf(),
        settings_manager: settings_manager.clone(),
        resources: rozsa_app::resources::LoadedResources::default(),
        system_prompt: "test".to_string(),
        model: Mutex::new(model()),
        thinking_effort: Mutex::new(ThinkingEffort::Off),
        pre_tool_use_factory: None,
        question_request_tx: None,
        model_stream: Some(scripted_stream(source.to_string_lossy().to_string())),
    });
    let created = shared
        .create_new_agent(&temp.path().join("sessions"), None)
        .await
        .unwrap();
    let session_id = created.id.clone();
    let agent = Arc::new(created.agent);
    let gui_state = GuiState {
        scene_router: Arc::new(Mutex::new(Default::default())),
        tabs: Arc::new(Mutex::new(vec![SessionTab::Active {
            path: created.path,
            agent: agent.clone(),
            live: LiveState::default(),
        }])),
        active_tab: Arc::new(Mutex::new(0)),
        shared,
        dev_flow: DevFlowRuntime::new(
            Arc::new(std::sync::Mutex::new(None)),
            Arc::new(rozsa_app::dev_flow::SystemProjectCommandRunner),
            Arc::new(rozsa_app::dev_flow::SystemCommandRunner),
            rozsa_app::dev_flow::DiscoveryEnvironment::from_process(),
            rozsa_gui::dev_flow::real_factory_provider(
                Arc::new(std::sync::Mutex::new(None)),
                Arc::new(std::sync::Mutex::new(None)),
            ),
        ),
        model_registry: None,
        session_dir: None,
        session_dirs: vec![],
        config_roots: rozsa_app::config_paths::ConfigRoots::from_roots(
            temp.path().join("global"),
            temp.path().join("project"),
        ),
        pending_approvals: None,
        pending_permission_contexts: Arc::new(DashMap::new()),
        pending_user_questions: Arc::new(DashMap::new()),
        permission_controller: Arc::new(PermissionController::new(PermissionMode::OnRequest)),
        global_settings_path: None,
        runtime_settings: Arc::new(Mutex::new(settings_manager.resolved().clone())),
        dev_flow_settings_update: Arc::new(Mutex::new(())),
        quota_summary: Arc::new(Mutex::new(None)),
    };
    let app = tauri::test::mock_app();
    let main = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let (ui_tx, mut ui_rx) = tokio::sync::mpsc::unbounded_channel();
    let (tool_tx, mut tool_rx) = tokio::sync::mpsc::unbounded_channel();
    main.listen("ui-state", move |event: Event| {
        let _ = ui_tx.send(event.payload().to_string());
    });
    main.listen("tool-event", move |event: Event| {
        let _ = tool_tx.send(event.payload().to_string());
    });
    spawn_event_forwarder_for_session(app.handle().clone(), session_id.clone(), gui_state);
    tokio::task::yield_now().await;
    agent.prompt("read the source").await.unwrap();

    let tool_event = tokio::time::timeout(std::time::Duration::from_secs(2), tool_rx.recv())
        .await
        .unwrap()
        .unwrap();
    let ui_state = tokio::time::timeout(std::time::Duration::from_secs(2), ui_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(tool_event.contains(&session_id));
    assert!(tool_event.contains("read"));
    assert!(ui_state.contains(&session_id));
}
