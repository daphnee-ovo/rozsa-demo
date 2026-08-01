use std::sync::{Arc, Mutex};

use rozsa_app::agent_session::{AgentSession, AgentSessionConfig, ModelStream};
use rozsa_app::dev_flow::{
    DevFlowPresentationAction, DevFlowPresentationItem, DevFlowPresentationItemKind,
    DevFlowPresentationRecord, DevFlowProjectKey, DevFlowRevisionKey, DevFlowToolPresentation,
};
use rozsa_app::resources::LoadedResources;
use rozsa_app::session::manager::SessionManager;
use rozsa_app::settings::SettingsManager;
use rozsa_model::event_stream::create_event_stream;
use rozsa_model::types::{
    Api, AssistantMessage, ContentBlock, Message, Model, ModelCost, Provider, StopReason,
    StreamEvent, ThinkingEffort, ToolResultMessage, Usage, UserContent, UserMessage,
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

fn done_event() -> StreamEvent {
    StreamEvent::Done {
        reason: StopReason::Stop,
        message: AssistantMessage {
            content: vec![ContentBlock::Text {
                text: "new answer".to_string(),
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

#[tokio::test]
async fn restored_session_messages_are_sent_to_the_next_model_request() {
    let temp = tempfile::tempdir().unwrap();
    let session_path = temp.path().join("session.jsonl");
    let mut manager = SessionManager::create(
        &session_path,
        "session".to_string(),
        temp.path().to_string_lossy().to_string(),
        None,
    )
    .unwrap();
    manager
        .append_message(Message::User(UserMessage {
            content: UserContent::Text("previous question".to_string()),
            display_text: None,
            timestamp: 1,
        }))
        .unwrap();
    manager
        .append_message(Message::Assistant(AssistantMessage {
            content: vec![ContentBlock::Text {
                text: "previous answer".to_string(),
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
            timestamp: 2,
        }))
        .unwrap();
    manager
        .append_message(Message::ToolResult(ToolResultMessage {
            tool_call_id: "call-1".to_string(),
            tool_name: "askUserQuestion".to_string(),
            content: vec![ContentBlock::Text {
                text: "question result".to_string(),
                signature: None,
            }],
            details: serde_json::Value::Null,
            is_error: false,
            timestamp: 3,
        }))
        .unwrap();
    manager
        .append_dev_flow_presentation(&DevFlowPresentationRecord::new(
            "call-1".to_string(),
            &DevFlowProjectKey {
                root: temp.path().to_path_buf(),
                revision: DevFlowRevisionKey::NamedBranch("main".to_string()),
            },
            DevFlowToolPresentation {
                action: DevFlowPresentationAction::Created,
                items: vec![DevFlowPresentationItem {
                    kind: DevFlowPresentationItemKind::Task,
                    id: "TASK-T001".to_string(),
                    short_id: "T001".to_string(),
                    title: Some("Persisted task".to_string()),
                }],
                details_unavailable: false,
            },
            4,
        ))
        .unwrap();
    drop(manager);

    let captured = Arc::new(Mutex::new(None));
    let captured_messages = captured.clone();
    let model_stream: ModelStream = Arc::new(move |_, context, _| {
        *captured_messages.lock().unwrap() = Some(context.messages.clone());
        let (sender, stream) = create_event_stream();
        sender.push(done_event());
        stream
    });
    let session = AgentSession::new(AgentSessionConfig {
        model: test_model(),
        thinking_effort: ThinkingEffort::Off,
        system_prompt: String::new(),
        cwd: temp.path().to_path_buf(),
        session_manager: SessionManager::open(&session_path).unwrap(),
        settings_manager: SettingsManager::load(temp.path().join("settings.json"), None, None)
            .unwrap(),
        resources: LoadedResources::default(),
        pre_tool_use: None,
        model_stream: Some(model_stream),
    });

    let restored = session.messages().await;
    assert_eq!(restored.len(), 3);

    session.prompt("current question").await.unwrap();

    let messages = captured.lock().unwrap().clone().unwrap();
    assert_eq!(messages.len(), 4);
    assert!(matches!(
        &messages[0],
        Message::User(user) if user.content.text() == "previous question"
    ));
    assert!(matches!(
        &messages[1],
        Message::Assistant(AssistantMessage { content, .. })
            if content.iter().any(|block| matches!(
                block,
                ContentBlock::Text { text, .. } if text == "previous answer"
            ))
    ));
    assert!(matches!(
        &messages[2],
        Message::ToolResult(ToolResultMessage { content, .. })
            if content.iter().any(|block| matches!(
                block,
                ContentBlock::Text { text, .. } if text == "question result"
            ))
    ));
    assert!(matches!(
        &messages[3],
        Message::User(user) if user.content.text() == "current question"
    ));
}
