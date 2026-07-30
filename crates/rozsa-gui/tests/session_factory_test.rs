use std::path::Path;

use rozsa_app::session::manager::SessionManager;
use rozsa_app::settings::SettingsManager;
use rozsa_gui::state::SharedResources;
use rozsa_model::types::{
    Api, Message, Model, ModelCost, Provider, ThinkingEffort, UserContent, UserMessage,
};
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
        thinking_effort_map: None,
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
        thinking_effort: Mutex::new(ThinkingEffort::Off),
        pre_tool_use_factory: None,
        question_request_tx: None,
        model_stream: None,
    }
}

#[tokio::test]
async fn gui_factory_creates_lazy_and_restored_sessions() {
    let temp = tempfile::tempdir().unwrap();
    let sessions = temp.path().join("sessions");
    let shared = shared_resources(temp.path());

    let created = shared.create_new_agent(&sessions, None).await.unwrap();
    assert_eq!(created.agent.cwd(), temp.path());
    assert_eq!(created.agent.model().await.id, "test-model");
    assert!(created.path.ends_with(&format!("{}.jsonl", created.id)));
    assert!(!std::path::Path::new(&created.path).exists());

    let mut persisted = SessionManager::create(
        &created.path,
        created.id.clone(),
        temp.path().to_string_lossy().to_string(),
        None,
    )
    .unwrap();
    persisted
        .append_message(Message::User(UserMessage {
            content: UserContent::Text("restored context".to_string()),
            display_text: None,
            timestamp: 1,
        }))
        .unwrap();
    let restored = shared
        .restore_agent(std::path::Path::new(&created.path))
        .await
        .unwrap();
    assert_eq!(restored.cwd(), temp.path());
    assert_eq!(restored.model().await.id, "test-model");
    assert!(restored.messages().await.iter().any(|message| matches!(
        message.as_standard(),
        Some(Message::User(user)) if user.content.text() == "restored context"
    )));
}

#[tokio::test]
async fn gui_factory_copies_context_for_continued_sessions() {
    let temp = tempfile::tempdir().unwrap();
    let sessions = temp.path().join("sessions");
    let parent_path = sessions.join("parent.jsonl");
    let mut parent = SessionManager::create(
        &parent_path,
        "parent".to_string(),
        temp.path().to_string_lossy().to_string(),
        None,
    )
    .unwrap();
    parent
        .append_message(Message::User(UserMessage {
            content: UserContent::Text("parent context".to_string()),
            display_text: None,
            timestamp: 1,
        }))
        .unwrap();

    let shared = shared_resources(temp.path());
    let continued = shared
        .create_continued_agent(&sessions, &parent_path)
        .await
        .unwrap();

    assert!(
        continued
            .agent
            .messages()
            .await
            .iter()
            .any(|message| matches!(
                message.as_standard(),
                Some(Message::User(user)) if user.content.text() == "parent context"
            ))
    );
}
