// FrameworkTree
// context_window_test.rs
// ├── user_entry()
// ├── compaction_threshold_is_inclusive_and_uses_provider_context_usage()
// ├── default_target_is_clamped_below_threshold_so_compaction_can_cut_history()
// ├── compaction_ratios_resolve_against_the_model_context_window()
// ├── agent_session_auto_compacts_after_provider_reports_threshold_usage()
// ├── compaction_rebuild_and_restore_keep_tool_call_result_pairs()
// └── assert_tool_pair()

use std::sync::Arc;

use rozsa_app::agent_session::{AgentSession, AgentSessionConfig, ModelStream};
use rozsa_app::compaction::{CompactionEngine, CompactionTrigger};
use rozsa_app::resources::LoadedResources;
use rozsa_app::session::manager::{SessionEntry, SessionEntryBase, SessionMessageEntry};
use rozsa_app::settings::{CompactionSettings, SettingsManager};
use rozsa_model::event_stream::create_event_stream;
use rozsa_model::types::{
    Api, AssistantMessage, ContentBlock, Message, Model, ModelCost, Provider, StopReason,
    StreamEvent, ThinkingEffort, ToolCall, ToolResultMessage, Usage, UserContent, UserMessage,
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

#[test]
fn compaction_ratios_resolve_against_the_model_context_window() {
    let limits = CompactionSettings::default()
        .resolve_token_limits(1_000)
        .expect("default compaction ratios should be valid");

    assert_eq!(limits.threshold_tokens, 850);
    assert_eq!(limits.target_tokens, 300);
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
    settings.resolved_mut().compaction.trigger_ratio = 0.1;
    settings.resolved_mut().compaction.target_ratio = 0.01;

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
        thinking_effort_map: None,
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
        thinking_effort: ThinkingEffort::Off,
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

#[tokio::test]
async fn compaction_rebuild_and_restore_keep_tool_call_result_pairs() {
    let temp = tempfile::tempdir().unwrap();
    let mut settings = SettingsManager::load(
        temp.path().join("global.json"),
        Some(temp.path().join("project.json")),
        None,
    )
    .unwrap();
    settings.resolved_mut().compaction.trigger_ratio = 0.1;
    settings.resolved_mut().compaction.target_ratio = 0.01;

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
        thinking_effort_map: None,
        headers: None,
        compat: None,
    };
    let model_stream: ModelStream = Arc::new(|_, _, _| {
        let (sender, stream) = create_event_stream();
        sender.push(StreamEvent::Done {
            reason: StopReason::Stop,
            message: AssistantMessage {
                content: vec![ContentBlock::Text {
                    text: "compaction summary".to_string(),
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
        });
        stream
    });

    let session_file = temp.path().join("session.jsonl");
    let mut session_manager = rozsa_app::session::manager::SessionManager::create(
        &session_file,
        "session".to_string(),
        temp.path().to_string_lossy().to_string(),
        None,
    )
    .unwrap();
    session_manager
        .append_message(Message::User(UserMessage {
            content: UserContent::Text("history before metadata".to_string()),
            display_text: None,
            timestamp: 1,
        }))
        .unwrap();
    session_manager
        .append_custom(
            "test_metadata".to_string(),
            Some(serde_json::json!({"ignored": true})),
        )
        .unwrap();
    session_manager
        .append_session_info(Some("session".to_string()))
        .unwrap();
    session_manager
        .append_message(Message::Assistant(AssistantMessage {
            content: vec![ContentBlock::ToolCall(ToolCall {
                id: "call-1".to_string(),
                name: "ls".to_string(),
                arguments: serde_json::json!({"path": "."}),
            })],
            api: Api::OpenAICompletions,
            provider: Provider::OpenAI,
            model: "scripted".to_string(),
            response_model: None,
            response_id: None,
            usage: Usage::default(),
            stop_reason: StopReason::ToolUse,
            error_message: None,
            timestamp: 2,
        }))
        .unwrap();
    session_manager
        .append_message(Message::ToolResult(ToolResultMessage {
            tool_call_id: "call-1".to_string(),
            tool_name: "ls".to_string(),
            content: vec![ContentBlock::Text {
                text: "file.txt".to_string(),
                signature: None,
            }],
            details: serde_json::Value::Null,
            is_error: false,
            timestamp: 3,
        }))
        .unwrap();

    let session = AgentSession::new(AgentSessionConfig {
        model,
        thinking_effort: ThinkingEffort::Off,
        system_prompt: String::new(),
        cwd: temp.path().to_path_buf(),
        session_manager,
        settings_manager: settings,
        resources: LoadedResources::default(),
        pre_tool_use: None,
        model_stream: Some(model_stream),
    });

    session.compact().await.unwrap();

    let in_memory = session
        .messages()
        .await
        .into_iter()
        .filter_map(|message| message.as_standard().cloned())
        .collect::<Vec<_>>();
    assert_tool_pair(&in_memory);

    let persisted = session.session_manager().await.context_messages();
    assert_tool_pair(&persisted);

    let reopened = rozsa_app::session::manager::SessionManager::open(&session_file).unwrap();
    assert_tool_pair(&reopened.context_messages());
}

fn assert_tool_pair(messages: &[Message]) {
    assert_eq!(
        messages.len(),
        2,
        "compaction should keep exactly the tool pair"
    );
    assert!(matches!(
        &messages[0],
        Message::Assistant(assistant)
            if assistant.content.iter().any(|block| matches!(
                block,
                ContentBlock::ToolCall(call) if call.id == "call-1"
            ))
    ));
    assert!(matches!(
        &messages[1],
        Message::ToolResult(result) if result.tool_call_id == "call-1"
    ));
}
