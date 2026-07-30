use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rozsa_app::agent_session::{AgentSession, AgentSessionConfig, ModelStream};
use rozsa_app::resources::LoadedResources;
use rozsa_app::session::manager::SessionManager;
use rozsa_app::settings::SettingsManager;
use rozsa_model::event_stream::create_event_stream;
use rozsa_model::types::{
    Api, AssistantMessage, ContentBlock, Model, ModelCost, Provider, StopReason, StreamEvent,
    ThinkingEffort, Usage,
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
        thinking_effort_map: None,
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
            .is_some_and(|prompt| prompt.contains("Create a concise session title"));
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
        thinking_effort: ThinkingEffort::Off,
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
async fn abort_discards_pending_steering_and_follow_up_messages() {
    let temp = tempfile::tempdir().unwrap();
    let session = test_session(&temp, scripted_stream(Duration::ZERO));
    session.steer("adjust the current turn");
    session.follow_up("continue after the current turn");
    assert_eq!(session.pending_messages().len(), 2);

    session.abort().await;

    assert!(session.pending_messages().is_empty());
}

#[tokio::test]
async fn generated_name_is_isolated_cleaned_and_persisted() {
    let temp = tempfile::tempdir().unwrap();
    let observation = Arc::new(Mutex::new(None));
    let captured = observation.clone();
    let stream: ModelStream = Arc::new(move |model, context, options| {
        *captured.lock().unwrap() = Some((
            model.id.clone(),
            options.reasoning,
            options.base.max_tokens,
            context.messages.len(),
            context.tools.len(),
            context.system_prompt.clone(),
        ));
        let (sender, stream) = create_event_stream();
        sender.push(done_event("<think>hidden</think>\nFix startup crash"));
        stream
    });
    let session = test_session(&temp, stream);
    assert!(session.is_initial_session_name_candidate().await);

    let generated = session
        .generate_session_name(
            "The desktop application crashes during startup after loading the saved workspace",
            Some(test_model()),
        )
        .await
        .unwrap();

    assert_eq!(generated.as_deref(), Some("Fix startup crash"));
    let observed = observation.lock().unwrap().clone().unwrap();
    assert_eq!(observed.0, "scripted");
    assert_eq!(observed.1, Some(ThinkingEffort::Low));
    assert_eq!(observed.2, Some(32));
    assert_eq!(observed.3, 1);
    assert_eq!(observed.4, 0);
    assert!(
        observed
            .5
            .unwrap()
            .contains("Use the same language as the user")
    );
    assert_eq!(
        session.session_manager().await.current_name().as_deref(),
        Some("Fix startup crash")
    );
}

#[tokio::test]
async fn short_input_is_used_directly_without_a_model_request() {
    let temp = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let captured = calls.clone();
    let stream: ModelStream = Arc::new(move |_, _, _| {
        captured.fetch_add(1, Ordering::SeqCst);
        let (_, stream) = create_event_stream();
        stream
    });
    let session = test_session(&temp, stream);

    assert_eq!(
        session
            .generate_session_name("  Fix   startup crash  ", None)
            .await
            .unwrap()
            .as_deref(),
        Some("Fix startup crash")
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn long_input_without_a_small_model_keeps_the_preview_fallback() {
    let temp = tempfile::tempdir().unwrap();
    let session = test_session(&temp, scripted_stream(Duration::ZERO));

    assert_eq!(
        session
            .generate_session_name(
                "The desktop application crashes during startup after loading the saved workspace",
                None,
            )
            .await
            .unwrap(),
        None
    );
    assert!(session.session_manager().await.current_name().is_none());
}

#[tokio::test]
async fn reasoning_model_uses_fixed_low_for_the_title_request() {
    let temp = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let captured = calls.clone();
    let observed_reasoning = Arc::new(Mutex::new(None));
    let captured_reasoning = observed_reasoning.clone();
    let stream: ModelStream = Arc::new(move |_, _, options| {
        captured.fetch_add(1, Ordering::SeqCst);
        *captured_reasoning.lock().unwrap() = options.reasoning;
        let (sender, stream) = create_event_stream();
        sender.push(done_event("Low reasoning title"));
        stream
    });
    let session = test_session(&temp, stream);
    let mut reasoning_model = test_model();
    reasoning_model.reasoning = true;

    let title = session
        .generate_session_name(
            "The desktop application crashes during startup after loading the saved workspace",
            Some(reasoning_model),
        )
        .await
        .unwrap();

    assert_eq!(title.as_deref(), Some("Low reasoning title"));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        *observed_reasoning.lock().unwrap(),
        Some(ThinkingEffort::Low)
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
            .generate_session_name(
                "The desktop application crashes during startup after loading the saved workspace",
                Some(test_model()),
            )
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
