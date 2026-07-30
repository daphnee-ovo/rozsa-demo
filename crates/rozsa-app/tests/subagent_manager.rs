// subagent::manager 单测 — spawn/list/abort 基本路径
// 不触发 agent_loop（不需要真实 API），仅验证状态机和 cap 限制。

use std::path::PathBuf;
use std::sync::Arc;

use rozsa_app::subagent::SubagentStatus;
use rozsa_app::subagent::manager::{SharedResources, SpawnConfig, SubagentManager};
use rozsa_app::subagent::scope::SubagentScope;
use rozsa_app::tools::create_read_tool;
use rozsa_model::types::{Api, InputModality, Model, ModelCost, Provider, ThinkingEffort};
use tokio::sync::Mutex;

fn dummy_model() -> Model {
    Model {
        id: "test-model".to_string(),
        name: "test".to_string(),
        api: Api::AnthropicMessages,
        provider: Provider::Anthropic,
        base_url: "https://example.invalid".to_string(),
        reasoning: false,
        input_modalities: vec![InputModality::Text],
        cost: ModelCost {
            input: 0.0,
            output: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
        },
        context_window: 1000,
        max_tokens: 100,
        thinking_effort_map: None,
        headers: None,
        compat: None,
    }
}

fn make_manager() -> SubagentManager {
    let shared = SharedResources {
        model_stream: Arc::new(|m, c, o| rozsa_model::stream::stream_simple(m, c, o)),
        convert_to_llm: Arc::new(|msgs| {
            msgs.iter()
                .filter_map(|m| m.as_standard().cloned())
                .collect()
        }),
        main_tools: Arc::new(Mutex::new(Vec::new())),
        tool_settings: Arc::new(std::sync::RwLock::new(Default::default())),
        main_model: dummy_model(),
        main_thinking_effort: ThinkingEffort::Off,
        cwd: PathBuf::from("/tmp"),
        session_dir: None,
        main_session_uuid: "test-session".to_string(),
        main_session_file: None,
        permission_hook: None,
    };
    SubagentManager::new(shared)
}

#[tokio::test]
async fn spawn_and_list() {
    let mut mgr = make_manager();
    let info = mgr
        .spawn(SpawnConfig {
            name: Some("worker-a".to_string()),
            system_prompt: "you are a test subagent".to_string(),
            model: None,
            thinking_effort: None,
            scope: SubagentScope::inherit(),
        })
        .await
        .expect("spawn ok");

    assert_eq!(info.name, "worker-a");
    assert_eq!(info.status, SubagentStatus::Idle);
    assert!(info.id.starts_with("subagent-"));

    let list = mgr.list().await;
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, info.id);
}

#[tokio::test]
async fn spawn_respects_cap() {
    let mut mgr = make_manager();
    // Spawn up to MAX_ACTIVE_SUBAGENTS (10); none are running so they're all Idle —
    // active_count counts only Running, so cap shouldn't trip here.
    for i in 0..10 {
        mgr.spawn(SpawnConfig {
            name: Some(format!("worker-{i}")),
            system_prompt: "x".to_string(),
            model: None,
            thinking_effort: None,
            scope: SubagentScope::inherit(),
        })
        .await
        .unwrap();
    }
    assert_eq!(mgr.list().await.len(), 10);
}

#[tokio::test]
async fn abort_marks_aborted() {
    let mut mgr = make_manager();
    let info = mgr
        .spawn(SpawnConfig {
            name: None,
            system_prompt: "x".to_string(),
            model: None,
            thinking_effort: None,
            scope: SubagentScope::inherit(),
        })
        .await
        .unwrap();
    mgr.abort(&info.id).await.unwrap();
    let list = mgr.list().await;
    assert_eq!(list[0].status, SubagentStatus::Aborted);
}

#[tokio::test]
async fn snapshot_returns_messages() {
    let mut mgr = make_manager();
    let info = mgr
        .spawn(SpawnConfig {
            name: None,
            system_prompt: "x".to_string(),
            model: None,
            thinking_effort: None,
            scope: SubagentScope::inherit(),
        })
        .await
        .unwrap();
    let snap = mgr.snapshot(&info.id).await.expect("snapshot");
    assert_eq!(snap.info.id, info.id);
    assert!(snap.messages.is_empty());
}

#[tokio::test]
async fn disabled_main_tools_are_not_inherited_by_subagents() {
    let shared = SharedResources {
        model_stream: Arc::new(|m, c, o| rozsa_model::stream::stream_simple(m, c, o)),
        convert_to_llm: Arc::new(|messages| {
            messages
                .iter()
                .filter_map(|message| message.as_standard().cloned())
                .collect()
        }),
        main_tools: Arc::new(Mutex::new(vec![Arc::from(create_read_tool())])),
        tool_settings: Arc::new(std::sync::RwLock::new(
            [("read".to_owned(), false)].into_iter().collect(),
        )),
        main_model: dummy_model(),
        main_thinking_effort: ThinkingEffort::Off,
        cwd: PathBuf::from("/tmp"),
        session_dir: None,
        main_session_uuid: "test-session".to_owned(),
        main_session_file: None,
        permission_hook: None,
    };
    let mut manager = SubagentManager::new(shared);
    let info = manager
        .spawn(SpawnConfig {
            name: None,
            system_prompt: "test".to_owned(),
            model: None,
            thinking_effort: None,
            scope: SubagentScope::inherit(),
        })
        .await
        .unwrap();

    assert_eq!(manager.tool_names(&info.id).await, Some(Vec::new()));
}
