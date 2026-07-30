use std::path::Path;
use std::sync::{Arc, Mutex as StdMutex};

use dashmap::DashMap;
use rozsa_app::permissions::{PendingApprovals, PermissionResponse};
use rozsa_app::settings::SettingsManager;
use rozsa_core::config::PreToolUseResult;
use rozsa_gui::state::{
    PreToolUseHook, PreToolUseHookFactory, SharedResources, deny_pending_approvals,
    permission_pending_key,
};
use rozsa_model::types::{Api, Model, ModelCost, Provider, ThinkingEffort};
use tokio::sync::Mutex;

fn test_model() -> Model {
    Model {
        id: "test-model".to_string(),
        name: "Test model".to_string(),
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
        context_window: 8_192,
        max_tokens: 1_024,
        thinking_effort_map: None,
        headers: None,
        compat: None,
    }
}

fn shared_resources(cwd: &Path, factory: PreToolUseHookFactory) -> SharedResources {
    SharedResources {
        cwd: cwd.to_path_buf(),
        settings_manager: SettingsManager::load(cwd.join("settings.json"), None, None).unwrap(),
        resources: rozsa_app::resources::LoadedResources::default(),
        system_prompt: "test system prompt".to_string(),
        model: Mutex::new(test_model()),
        thinking_effort: Mutex::new(ThinkingEffort::Off),
        pre_tool_use_factory: Some(factory),
        question_request_tx: None,
        model_stream: None,
    }
}

#[tokio::test]
async fn gui_factory_binds_a_distinct_permission_hook_to_each_session() {
    let temp = tempfile::tempdir().unwrap();
    let observed_session_ids = Arc::new(StdMutex::new(Vec::new()));
    let factory: PreToolUseHookFactory = {
        let observed_session_ids = observed_session_ids.clone();
        Arc::new(move |session_id| {
            observed_session_ids.lock().unwrap().push(session_id);
            Arc::new(
                |_| -> std::pin::Pin<
                    Box<dyn std::future::Future<Output = Option<PreToolUseResult>> + Send>,
                > { Box::pin(async { None }) },
            ) as PreToolUseHook
        })
    };
    let shared = shared_resources(temp.path(), factory);
    let sessions = temp.path().join("sessions");

    let first = shared.create_new_agent(&sessions, None).await.unwrap();
    let second = shared.create_new_agent(&sessions, None).await.unwrap();

    assert_eq!(
        observed_session_ids.lock().unwrap().as_slice(),
        &[first.id, second.id]
    );
}

#[test]
fn abort_or_close_denies_only_the_matching_session_pending_approvals() {
    let approvals: PendingApprovals = Arc::new(DashMap::new());
    let (a_sender, mut a_receiver) = tokio::sync::oneshot::channel();
    let (b_sender, mut b_receiver) = tokio::sync::oneshot::channel();
    approvals.insert(permission_pending_key("session-a", "call-1"), a_sender);
    approvals.insert(permission_pending_key("session-b", "call-1"), b_sender);

    assert_eq!(deny_pending_approvals(&approvals, Some("session-a")), 1);
    assert_eq!(a_receiver.try_recv(), Ok(PermissionResponse::Deny));
    assert!(b_receiver.try_recv().is_err());
    assert_eq!(deny_pending_approvals(&approvals, None), 1);
    assert_eq!(b_receiver.try_recv(), Ok(PermissionResponse::Deny));
}
