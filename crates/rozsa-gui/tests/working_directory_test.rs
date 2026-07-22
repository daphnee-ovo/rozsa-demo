use std::path::Path;

use rozsa_app::session::manager::SessionManager;
use rozsa_app::settings::SettingsManager;
use rozsa_gui::state::SharedResources;
use rozsa_model::types::{Api, Model, ModelCost, Provider, ThinkingLevel};
use tokio::sync::Mutex;

fn test_model() -> Model {
    Model {
        id: "test-model".to_string(),
        name: "Test model".to_string(),
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
        thinking_level_map: None,
        headers: None,
        compat: None,
    }
}

fn shared_resources(cwd: &Path) -> SharedResources {
    SharedResources {
        cwd: cwd.to_path_buf(),
        settings_manager: SettingsManager::load(cwd.join("settings.json"), None, None).unwrap(),
        resources: rozsa_app::resources::LoadedResources::default(),
        system_prompt: "test system prompt".to_string(),
        model: Mutex::new(test_model()),
        thinking_level: Mutex::new(ThinkingLevel::Off),
        pre_tool_use_factory: None,
        question_request_tx: None,
        model_stream: None,
    }
}

#[tokio::test]
async fn restored_gui_agent_uses_the_session_header_cwd() {
    let temp = tempfile::tempdir().unwrap();
    let child = temp.path().join("child");
    std::fs::create_dir(&child).unwrap();
    let session_path = temp.path().join("session.jsonl");
    let mut manager = SessionManager::create(
        &session_path,
        "session".to_string(),
        temp.path().to_string_lossy().to_string(),
        None,
    )
    .unwrap();
    manager
        .set_cwd(child.to_string_lossy().to_string())
        .unwrap();

    let shared = shared_resources(temp.path());
    let agent = shared.restore_agent(&session_path).await.unwrap();

    assert_eq!(agent.cwd(), child);
    assert_eq!(agent.current_cwd().await, child);
}
