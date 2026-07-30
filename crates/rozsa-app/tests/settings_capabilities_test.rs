use std::collections::BTreeMap;
use std::fs;
use std::sync::{Arc, Mutex};

use rozsa_app::agent_session::{AgentSession, AgentSessionConfig, ModelStream};
use rozsa_app::resources::LoadedResources;
use rozsa_app::session::manager::SessionManager;
use rozsa_app::settings::{
    CapabilityKind, PermissionRuleKind, SettingsManager, SettingsScope, merge::merge_settings,
    schema::PartialSettings,
};
use rozsa_model::event_stream::create_event_stream;
use rozsa_model::types::{
    Api, AssistantMessage, ContentBlock, Model, ModelCost, Provider, StopReason, StreamEvent,
    ThinkingEffort, Usage,
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

#[test]
fn layered_permission_updates_preserve_settings_and_remove_retired_fields() {
    let temp = tempdir().unwrap();
    let global = temp.path().join("global/settings.json");
    let project = temp.path().join("project/settings.json");
    fs::create_dir_all(global.parent().unwrap()).unwrap();
    fs::write(
        &global,
        r#"{"transport":"sse","permission":{"mode":"on-request","allow":["Read(*)"],"allowed_tools":["Bash"],"blocked_commands":["rm"],"auto_approve_patterns":[".*"]}}"#,
    )
    .unwrap();
    let mut manager = SettingsManager::load(global.clone(), Some(project.clone()), None).unwrap();

    manager
        .set_permission_rule_overrides(
            SettingsScope::Project,
            PermissionRuleKind::Allow,
            Some(vec!["Edit(src/*)".to_owned()]),
        )
        .unwrap();
    assert_eq!(
        manager.resolved().permissions.allow,
        vec!["Edit(src/*)".to_owned()]
    );
    assert_eq!(manager.resolved().transport, "sse");

    manager
        .set_permission_mode_override(SettingsScope::Global, Some("yolo".to_owned()))
        .unwrap();
    let persisted = fs::read_to_string(global).unwrap();
    assert!(!persisted.contains("allowed_tools"));
    assert!(!persisted.contains("blocked_commands"));
    assert!(!persisted.contains("auto_approve_patterns"));

    manager
        .set_permission_rule_overrides(SettingsScope::Project, PermissionRuleKind::Allow, None)
        .unwrap();
    assert_eq!(
        manager.resolved().permissions.allow,
        vec!["Read(*)".to_owned()]
    );
}

#[test]
fn permission_rule_set_moves_are_written_atomically() {
    let temp = tempdir().unwrap();
    let global = temp.path().join("settings.json");
    let mut manager = SettingsManager::load(global.clone(), None, None).unwrap();

    manager
        .set_permission_rule_set(
            SettingsScope::Global,
            vec!["Bash(cargo publish *)".to_owned()],
            vec![],
            vec!["ls(*)".to_owned()],
        )
        .unwrap();

    let persisted: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(global).unwrap()).unwrap();
    assert_eq!(
        persisted["permission"]["deny"],
        serde_json::json!(["Bash(cargo publish *)"])
    );
    assert_eq!(persisted["permission"]["ask"], serde_json::json!([]));
    assert_eq!(
        persisted["permission"]["allow"],
        serde_json::json!(["ls(*)"])
    );
    assert_eq!(
        manager.resolved().permissions.allow,
        vec!["ls(*)".to_owned()]
    );
}

#[test]
fn global_path_and_universal_rules_fail_without_changing_the_file() {
    let temp = tempdir().unwrap();
    let global = temp.path().join("settings.json");
    fs::write(&global, r#"{"transport":"sse"}"#).unwrap();
    let before = fs::read_to_string(&global).unwrap();
    let mut manager = SettingsManager::load(global.clone(), None, None).unwrap();

    assert!(
        manager
            .set_permission_rule_overrides(
                SettingsScope::Global,
                PermissionRuleKind::Allow,
                Some(vec!["Edit(src/**)".to_owned()]),
            )
            .is_err()
    );
    assert!(
        manager
            .set_permission_rule_overrides(
                SettingsScope::Global,
                PermissionRuleKind::Allow,
                Some(vec!["*(*)".to_owned()]),
            )
            .is_err()
    );
    assert_eq!(fs::read_to_string(global).unwrap(), before);
}

#[test]
fn auto_approve_mode_fails_before_persistence() {
    let temp = tempdir().unwrap();
    let global = temp.path().join("settings.json");
    fs::write(&global, r#"{"permission":{"mode":"on-request"}}"#).unwrap();
    let before = fs::read_to_string(&global).unwrap();
    let mut manager = SettingsManager::load(global.clone(), None, None).unwrap();

    let error = manager
        .set_permission_mode_override(SettingsScope::Global, Some("auto-approve".to_owned()))
        .unwrap_err();

    assert!(error.to_string().contains("not implemented"));
    assert_eq!(fs::read_to_string(global).unwrap(), before);
    assert_eq!(manager.resolved().permissions.mode, "on-request");
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
        thinking_effort_map: None,
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
        thinking_effort: ThinkingEffort::Off,
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
