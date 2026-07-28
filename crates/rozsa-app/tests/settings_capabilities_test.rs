use std::collections::BTreeMap;
use std::fs;
use std::sync::{Arc, Mutex};

use rozsa_app::agent_session::{AgentSession, AgentSessionConfig, ModelStream};
use rozsa_app::resources::LoadedResources;
use rozsa_app::session::manager::SessionManager;
use rozsa_app::settings::{
    CapabilityKind, SettingsManager, SettingsScope, merge::merge_settings, schema::PartialSettings,
};
use rozsa_model::event_stream::create_event_stream;
use rozsa_model::types::{
    Api, AssistantMessage, ContentBlock, Model, ModelCost, Provider, StopReason, StreamEvent,
    ThinkingLevel, Usage,
};
use tempfile::tempdir;

#[test]
fn project_capability_entries_override_global_per_name() {
    let base = rozsa_app::settings::Settings {
        tools: BTreeMap::from([("read".to_owned(), false), ("write".to_owned(), false)]),
        ..Default::default()
    };
    let overlay: PartialSettings =
        serde_json::from_value(serde_json::json!({"tools": {"read": true}})).unwrap();

    let merged = merge_settings(&base, &overlay);

    assert_eq!(merged.tools.get("read"), Some(&true));
    assert_eq!(merged.tools.get("write"), Some(&false));
}

#[test]
fn layer_updates_preserve_unrelated_settings_and_support_inheritance() {
    let temp = tempdir().unwrap();
    let global = temp.path().join("global/settings.json");
    let project = temp.path().join("project/settings.json");
    fs::create_dir_all(global.parent().unwrap()).unwrap();
    fs::write(&global, r#"{"transport":"sse","tools":{"bash":false}}"#).unwrap();
    let mut manager = SettingsManager::load(global.clone(), Some(project.clone()), None).unwrap();

    assert!(!manager.capability_enabled(CapabilityKind::Tools, "bash"));
    assert!(manager.capability_enabled(CapabilityKind::Tools, "read"));

    manager
        .set_capability_override(
            SettingsScope::Project,
            CapabilityKind::Tools,
            "bash",
            Some(true),
        )
        .unwrap();
    assert!(manager.capability_enabled(CapabilityKind::Tools, "bash"));
    assert_eq!(manager.resolved().transport, "sse");

    manager
        .set_capability_override(SettingsScope::Project, CapabilityKind::Tools, "bash", None)
        .unwrap();
    assert!(!manager.capability_enabled(CapabilityKind::Tools, "bash"));
    assert!(
        manager
            .capability_overrides(SettingsScope::Project, CapabilityKind::Tools)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn malformed_capability_shapes_fail_loudly() {
    let temp = tempdir().unwrap();
    let global = temp.path().join("settings.json");
    fs::write(&global, r#"{"tools":[]}"#).unwrap();

    assert!(SettingsManager::load(global, None, None).is_err());
}

fn test_model() -> Model {
    Model {
        id: "scripted".to_owned(),
        name: "Scripted".to_owned(),
        api: Api::OpenAICompletions,
        provider: Provider::OpenAI,
        base_url: "http://127.0.0.1".to_owned(),
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
    }
}

#[tokio::test]
async fn tool_filtering_reaches_model_context_and_reload_updates_existing_sessions() {
    let temp = tempdir().unwrap();
    let settings_path = temp.path().join("settings.json");
    fs::write(&settings_path, r#"{"tools":{"read":false}}"#).unwrap();
    let captured = Arc::new(Mutex::new(Vec::<Vec<String>>::new()));
    let captured_contexts = Arc::clone(&captured);
    let model_stream: ModelStream = Arc::new(move |_, context, _| {
        captured_contexts
            .lock()
            .unwrap()
            .push(context.tools.iter().map(|tool| tool.name.clone()).collect());
        let (sender, stream) = create_event_stream();
        sender.push(StreamEvent::Done {
            reason: StopReason::Stop,
            message: AssistantMessage {
                content: vec![ContentBlock::Text {
                    text: "ok".to_owned(),
                    signature: None,
                }],
                api: Api::OpenAICompletions,
                provider: Provider::OpenAI,
                model: "scripted".to_owned(),
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
    let session = AgentSession::new(AgentSessionConfig {
        model: test_model(),
        thinking_level: ThinkingLevel::Off,
        system_prompt: String::new(),
        cwd: temp.path().to_path_buf(),
        session_manager: SessionManager::create(
            temp.path().join("session.jsonl"),
            "session".to_owned(),
            temp.path().to_string_lossy().to_string(),
            None,
        )
        .unwrap(),
        settings_manager: SettingsManager::load(settings_path.clone(), None, None).unwrap(),
        resources: LoadedResources::default(),
        pre_tool_use: None,
        model_stream: Some(model_stream),
    });
    session.register_default_tools(temp.path()).await;

    session.prompt("before reload").await.unwrap();
    fs::write(&settings_path, r#"{"tools":{"read":true}}"#).unwrap();
    let reload = session.reload_configuration().await.unwrap();
    assert!(reload.tool_count >= 1);
    session.prompt("after reload").await.unwrap();

    let captured = captured.lock().unwrap();
    assert!(!captured[0].contains(&"read".to_owned()));
    assert!(captured[1].contains(&"read".to_owned()));
}
