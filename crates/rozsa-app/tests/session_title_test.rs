use std::sync::Arc;
use std::time::Duration;

use rozsa_app::agent_session::{AgentSession, AgentSessionConfig, ModelStream};
use rozsa_app::resources::LoadedResources;
use rozsa_app::session::manager::SessionManager;
use rozsa_app::settings::SettingsManager;
use rozsa_model::event_stream::create_event_stream;
use rozsa_model::types::{
    Api, AssistantMessage, ContentBlock, Model, ModelCost, Provider, StopReason, StreamEvent,
    ThinkingLevel, Usage,
};

fn test_model() -> Model {
    Model {
        id: "scripted".to_string(),
        name: "Scripted".to_string(),
        api: Api::OpenAICompletions,
        provider: Provider::OpenAI,
        base_url: "http://127.0.0.1".to_string(),
        reasoning: false,
        input_modalities: Vec::new(),
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

fn done_event(text: &str) -> StreamEvent {
    StreamEvent::Done {
        reason: StopReason::Stop,
        message: AssistantMessage {
            content: vec![ContentBlock::Text {
                text: text.to_string(),
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
        },
    }
}

fn scripted_stream(title_delay: Duration) -> ModelStream {
    Arc::new(move |_, context, _| {
        let is_title = context
            .system_prompt
            .as_deref()
            .is_some_and(|prompt| prompt.contains("Generate a concise title"));
        let (sender, stream) = create_event_stream();
        if is_title {
            let delay = title_delay;
            tokio::spawn(async move {
                tokio::time::sleep(delay).await;
                sender.push(done_event("<think>hidden</think>\nFix startup crash"));
            });
        } else {
            sender.push(done_event("done"));
        }
        stream
    })
}

fn test_session(temp: &tempfile::TempDir, model_stream: ModelStream) -> AgentSession {
    let session_manager = SessionManager::create(
        temp.path().join("session.jsonl"),
        "session".to_string(),
        temp.path().to_string_lossy().to_string(),
        None,
    )
    .unwrap();
    AgentSession::new(AgentSessionConfig {
        model: test_model(),
        thinking_level: ThinkingLevel::Off,
        system_prompt: String::new(),
        cwd: temp.path().to_path_buf(),
        session_manager,
        settings_manager: SettingsManager::load(temp.path().join("settings.json"), None, None)
            .unwrap(),
        resources: LoadedResources::default(),
        pre_tool_use: None,
        model_stream: Some(model_stream),
    })
}

#[tokio::test]
async fn generated_name_is_isolated_cleaned_and_persisted() {
    let temp = tempfile::tempdir().unwrap();
    let session = test_session(&temp, scripted_stream(Duration::ZERO));
    session
        .prompt("The app crashes during startup")
        .await
        .unwrap();

    let generated = session
        .generate_session_name("The app crashes during startup")
        .await
        .unwrap();

    assert_eq!(generated.as_deref(), Some("Fix startup crash"));
    assert_eq!(
        session.session_manager().await.current_name().as_deref(),
        Some("Fix startup crash")
    );
}

#[tokio::test]
async fn manual_rename_wins_over_in_flight_generation() {
    let temp = tempfile::tempdir().unwrap();
    let session = Arc::new(test_session(
        &temp,
        scripted_stream(Duration::from_millis(50)),
    ));
    session
        .prompt("The app crashes during startup")
        .await
        .unwrap();

    let naming_session = session.clone();
    let generation = tokio::spawn(async move {
        naming_session
            .generate_session_name("The app crashes during startup")
            .await
            .unwrap()
    });
    tokio::time::sleep(Duration::from_millis(10)).await;
    session
        .session_manager()
        .await
        .append_session_info(Some("Manual name".to_string()))
        .unwrap();

    assert_eq!(generation.await.unwrap(), None);
    assert_eq!(
        session.session_manager().await.current_name().as_deref(),
        Some("Manual name")
    );
}
