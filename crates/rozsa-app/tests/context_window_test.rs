use std::sync::Arc;

use rozsa_app::agent_session::{AgentSession, AgentSessionConfig, ModelStream};
use rozsa_app::compaction::{CompactionEngine, CompactionTrigger};
use rozsa_app::resources::LoadedResources;
use rozsa_app::session::manager::{SessionEntry, SessionEntryBase, SessionMessageEntry};
use rozsa_app::settings::SettingsManager;
use rozsa_model::event_stream::create_event_stream;
use rozsa_model::types::{
    Api, AssistantMessage, ContentBlock, Message, Model, ModelCost, Provider, StopReason,
    StreamEvent, ThinkingLevel, Usage, UserContent, UserMessage,
};

fn user_entry(id: usize, text: &str) -> SessionEntry {
    SessionEntry::Message(SessionMessageEntry {
        base: SessionEntryBase {
            id: format!("entry-{id}"),
            parent_id: None,
            timestamp: format!("2026-07-12T00:00:{id:02}Z"),
        },
        message: Message::User(UserMessage {
            content: UserContent::Text(text.to_string()),
            display_text: None,
            timestamp: id as i64,
        }),
    })
}

#[test]
fn compaction_threshold_is_inclusive_and_uses_provider_context_usage() {
    let engine = CompactionEngine::new(CompactionTrigger {
        threshold_tokens: 100,
        target_tokens: 20,
    });
    assert!(!engine.should_compact(99));
    assert!(engine.should_compact(100));

    let entries = (0..5)
        .map(|id| user_entry(id, &"x".repeat(32)))
        .collect::<Vec<_>>();
    let plan = engine
        .prepare_with_context(&entries, 100)
        .expect("provider-reported context usage should trigger a plan");
    assert!(plan.cut_point_index > 0);
    assert!(plan.cut_point_index < entries.len());
    assert_eq!(plan.estimated_tokens_before, 125);
}

#[test]
fn default_target_is_clamped_below_threshold_so_compaction_can_cut_history() {
    let engine = CompactionEngine::new(CompactionTrigger {
        threshold_tokens: 100,
        target_tokens: 200,
    });
    let entries = (0..8)
        .map(|id| user_entry(id, &"y".repeat(40)))
        .collect::<Vec<_>>();
    let plan = engine
        .prepare(&entries)
        .expect("target above threshold must not disable compaction");
    assert!(plan.cut_point_index > 0);
    assert!(plan.cut_point_index < entries.len());
}

#[tokio::test]
async fn agent_session_auto_compacts_after_provider_reports_threshold_usage() {
    let temp = tempfile::tempdir().unwrap();
    let mut settings = SettingsManager::load(
        temp.path().join("global.json"),
        Some(temp.path().join("project.json")),
        None,
    )
    .unwrap();
    settings.resolved_mut().compaction.enabled = true;
    settings.resolved_mut().compaction.threshold_tokens = 10;
    settings.resolved_mut().compaction.target_tokens = 1;

    let model = Model {
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
        context_window: 128,
        max_tokens: 64,
        thinking_level_map: None,
        headers: None,
        compat: None,
    };
    let model_stream: ModelStream = Arc::new(|_, _, _| {
        let (sender, stream) = create_event_stream();
        sender.push(StreamEvent::Done {
            reason: StopReason::Stop,
            message: AssistantMessage {
                content: vec![ContentBlock::Text {
                    text: "summary or answer".to_string(),
                    signature: None,
                }],
                api: Api::OpenAICompletions,
                provider: Provider::OpenAI,
                model: "scripted".to_string(),
                response_model: None,
                response_id: None,
                usage: Usage {
                    input: 19,
                    output: 1,
                    total_tokens: 20,
                    ..Usage::default()
                },
                stop_reason: StopReason::Stop,
                error_message: None,
                timestamp: 0,
            },
        });
        stream
    });
    let session_manager = rozsa_app::session::manager::SessionManager::create(
        temp.path().join("session.jsonl"),
        "session".to_string(),
        temp.path().to_string_lossy().to_string(),
        None,
    )
    .unwrap();
    let session = AgentSession::new(AgentSessionConfig {
        model,
        thinking_level: ThinkingLevel::Off,
        system_prompt: String::new(),
        cwd: temp.path().to_path_buf(),
        session_manager,
        settings_manager: settings,
        resources: LoadedResources::default(),
        pre_tool_use: None,
        model_stream: Some(model_stream),
    });

    session.prompt("hello").await.unwrap();
    let messages = session.messages().await;
    assert!(messages.iter().any(|message| matches!(
        message,
        rozsa_core::messages::AgentMessage::Custom { message }
            if message.message_type == "compaction_summary"
    )));
    assert!(!session.is_compacting());
}
