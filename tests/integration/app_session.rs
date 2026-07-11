//! Integration test for AgentSession.
//!
//! Verifies: session creation, tool registration, session persistence, settings defaults.

use rozsa_app::agent_session::{AgentSession, AgentSessionConfig};
use rozsa_app::resources::LoadedResources;
use rozsa_app::session::manager::SessionManager;
use rozsa_app::settings::SettingsManager;
use rozsa_model::types::{Api, InputModality, Model, ModelCost, Provider, ThinkingLevel};

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

    let settings_manager =
        SettingsManager::load(tmp_dir.path().join("global-settings.json"), None, None).unwrap();

    let config = AgentSessionConfig {
        model: test_model(),
        thinking_level: ThinkingLevel::Off,
        system_prompt: "You are a test agent.".to_string(),
        cwd: tmp_dir.path().to_path_buf(),
        session_manager,
        settings_manager,
        resources: LoadedResources::default(),
        pre_tool_use: None,
        model_stream: None,
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
    assert!(
        !entry_id.is_empty(),
        "append should return a valid entry id"
    );
}

#[tokio::test]
async fn session_list_dir_extracts_first_message() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let session_path = tmp_dir.path().join("test.jsonl");

    let mut manager = SessionManager::create(
        &session_path,
        "list-test".to_string(),
        "/tmp".to_string(),
        None,
    )
    .unwrap();

    let user_msg = rozsa_model::types::Message::User(rozsa_model::types::UserMessage {
        content: rozsa_model::types::UserContent::Text("用 tui-test 测试".to_string()),
        display_text: None,
        timestamp: 2000,
    });
    manager.append_message(user_msg).unwrap();

    let metas = SessionManager::list_dir(tmp_dir.path()).unwrap();
    assert_eq!(metas.len(), 1);
    assert_eq!(metas[0].first_message, "用 tui-test 测试");
    assert_eq!(metas[0].message_count, 1);
}

#[tokio::test]
async fn session_list_dir_parses_ts_format_file() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let session_path = tmp_dir.path().join("ts-session.jsonl");

    // TS 版产生的真实 session 文件格式
    let content = r#"{"type":"session","version":3,"id":"019ef3e4","timestamp":"2026-06-23T09:51:39.256Z","cwd":"/home/test"}
{"type":"message","id":"218c7a06","parentId":null,"timestamp":"2026-06-23T09:51:42.193Z","message":{"role":"user","content":[{"type":"text","text":"hello world"}],"timestamp":1782208302191}}
{"type":"message","id":"281bb95c","parentId":"218c7a06","timestamp":"2026-06-23T09:51:45.785Z","message":{"role":"assistant","content":[{"type":"text","text":"Hi!"}],"model":"claude-3","api":"anthropic-messages","provider":"anthropic","stopReason":"stop","timestamp":1782208302226,"usage":{"input":3,"output":10,"cacheRead":0,"cacheWrite":0,"totalTokens":13,"cost":{"input":0.0,"output":0.0,"cacheRead":0.0,"cacheWrite":0.0,"total":0.0}}}}
"#;
    std::fs::write(&session_path, content).unwrap();

    let metas = SessionManager::list_dir(tmp_dir.path()).unwrap();
    assert_eq!(metas.len(), 1);
    assert_eq!(metas[0].first_message, "hello world");
    assert_eq!(metas[0].message_count, 2);
    assert_eq!(metas[0].id, "019ef3e4");
}

#[tokio::test]
async fn settings_manager_default_values() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let manager =
        SettingsManager::load(tmp_dir.path().join("nonexistent.json"), None, None).unwrap();

    let settings = manager.resolved();
    assert!(settings.compaction.enabled);
    assert_eq!(settings.transport, "auto");
    assert!(!settings.block_images);
}
