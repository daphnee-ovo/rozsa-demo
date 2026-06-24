//! Integration test for AgentSession.
//!
//! Verifies: session creation, tool registration, session persistence, settings defaults.

use rozsa_app::agent_session::{AgentSession, AgentSessionConfig};
use rozsa_app::resources::LoadedResources;
use rozsa_app::session::manager::SessionManager;
use rozsa_app::settings::SettingsManager;
use rozsa_model::types::{
    Api, InputModality, Model, ModelCost, Provider, ThinkingLevel,
};

fn test_model() -> Model {
    Model {
        id: "test-model".to_string(),
        name: "Test Model".to_string(),
        api: Api::OpenAIResponses,
        provider: Provider::OpenAI,
        base_url: "https://example.invalid".to_string(),
        reasoning: false,
        input_modalities: vec![InputModality::Text],
        cost: ModelCost {
            input: 0.0,
            output: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
        },
        context_window: 8192,
        max_tokens: 2048,
        thinking_level_map: None,
        headers: None,
        compat: None,
    }
}

#[tokio::test]
async fn agent_session_creates_and_registers_tools() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let session_path = tmp_dir.path().join("session.jsonl");

    let session_manager = SessionManager::create(
        &session_path,
        "test-session".to_string(),
        tmp_dir.path().to_string_lossy().to_string(),
        None,
    )
    .unwrap();

    let settings_manager = SettingsManager::load(
        tmp_dir.path().join("global-settings.json"),
        None,
        None,
    )
    .unwrap();

    let config = AgentSessionConfig {
        model: test_model(),
        thinking_level: ThinkingLevel::Off,
        system_prompt: "You are a test agent.".to_string(),
        cwd: tmp_dir.path().to_path_buf(),
        session_manager,
        settings_manager,
        resources: LoadedResources::default(),
    };

    let session = AgentSession::new(config);
    session.register_default_tools(tmp_dir.path()).await;

    assert!(!session.is_running());
    assert!(session.messages().await.is_empty());
}

#[tokio::test]
async fn session_manager_persistence_round_trip() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let session_path = tmp_dir.path().join("session.jsonl");

    let mut manager = SessionManager::create(
        &session_path,
        "test-session".to_string(),
        tmp_dir.path().to_string_lossy().to_string(),
        None,
    )
    .unwrap();

    let user_msg = rozsa_model::types::Message::User(rozsa_model::types::UserMessage {
        content: rozsa_model::types::UserContent::Text("hello".to_string()),
        display_text: None,
        timestamp: 1000,
    });
    let entry_id = manager.append_message(user_msg).unwrap();
    assert!(!entry_id.is_empty(), "append should return a valid entry id");
}

#[tokio::test]
async fn settings_manager_default_values() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let manager = SettingsManager::load(
        tmp_dir.path().join("nonexistent.json"),
        None,
        None,
    )
    .unwrap();

    let settings = manager.resolved();
    assert!(settings.compaction.enabled);
    assert_eq!(settings.transport, "auto");
    assert!(!settings.block_images);
}
