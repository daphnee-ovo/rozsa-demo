use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use dashmap::DashMap;
use rozsa_app::agent_session::ModelStream;
use rozsa_app::permissions::{ApprovalInfo, PendingApprovals, PermissionResponse, RiskLevel};
use rozsa_app::settings::SettingsManager;
use rozsa_core::config::PreToolUseResult;
use rozsa_core::events::AgentEvent;
use rozsa_gui::state::{
    PermissionRequest, PreToolUseHook, PreToolUseHookFactory, SharedResources,
    permission_pending_key,
};
use rozsa_model::event_stream::create_event_stream;
use rozsa_model::types::{
    Api, AssistantMessage, ContentBlock, Model, ModelCost, Provider, StopReason, StreamEvent,
    ThinkingLevel, ToolCall, Usage,
};
use tokio::sync::{Mutex, mpsc, oneshot};

fn test_model() -> Model {
    Model {
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
        context_window: 8_192,
        max_tokens: 1_024,
        thinking_level_map: None,
        headers: None,
        compat: None,
    }
}

fn assistant(content: Vec<ContentBlock>, reason: StopReason) -> AssistantMessage {
    AssistantMessage {
        content,
        api: Api::OpenAICompletions,
        provider: Provider::OpenAI,
        model: "scripted".to_string(),
        response_model: None,
        response_id: None,
        usage: Usage::default(),
        stop_reason: reason,
        error_message: None,
        timestamp: 0,
    }
}

fn scripted_model(source_path: String) -> ModelStream {
    let step = Arc::new(AtomicUsize::new(0));
    Arc::new(move |_model, _context, _options| {
        let next = step.fetch_add(1, Ordering::SeqCst);
        let message = match next {
            0 => assistant(
                vec![ContentBlock::ToolCall(ToolCall {
                    id: "read-1".to_string(),
                    name: "read".to_string(),
                    arguments: serde_json::json!({"file_path": source_path}),
                })],
                StopReason::ToolUse,
            ),
            1 => assistant(
                vec![ContentBlock::ToolCall(ToolCall {
                    id: "edit-1".to_string(),
                    name: "edit".to_string(),
                    arguments: serde_json::json!({
                        "file_path": source_path,
                        "old_string": "1",
                        "new_string": "2",
                    }),
                })],
                StopReason::ToolUse,
            ),
            2 => assistant(
                vec![ContentBlock::ToolCall(ToolCall {
                    id: "bash-1".to_string(),
                    name: "bash".to_string(),
                    arguments: serde_json::json!({
                        "command": "test \"$(cat src/lib.rs)\" = \"2\"",
                    }),
                })],
                StopReason::ToolUse,
            ),
            _ => assistant(
                vec![ContentBlock::Text {
                    text: "Completed: source changed and verification passed.".to_string(),
                    signature: None,
                }],
                StopReason::Stop,
            ),
        };
        let (sender, stream) = create_event_stream();
        sender.push(StreamEvent::Done {
            reason: message.stop_reason,
            message,
        });
        stream
    })
}

fn approval_factory(
    pending: PendingApprovals,
    request_tx: mpsc::UnboundedSender<PermissionRequest>,
) -> PreToolUseHookFactory {
    Arc::new(move |session_id| {
        let pending = pending.clone();
        let request_tx = request_tx.clone();
        Arc::new(
            move |ctx: rozsa_core::config::PreToolUseContext| -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Option<PreToolUseResult>> + Send>,
            > {
                let pending = pending.clone();
                let request_tx = request_tx.clone();
                let session_id = session_id.clone();
                Box::pin(async move {
                    if ctx.tool_name != "edit" {
                        return None;
                    }
                    let request_id = ctx.tool_call_id.clone();
                    let turn_id = ctx
                        .assistant_message
                        .response_id
                        .clone()
                        .unwrap_or_else(|| request_id.clone());
                    let (sender, receiver) = oneshot::channel();
                    pending.insert(permission_pending_key(&session_id, &request_id), sender);
                    let _ = request_tx.send(PermissionRequest {
                        session_id: session_id.clone(),
                        turn_id,
                        request_id,
                        tool_name: ctx.tool_name.clone(),
                        description: "Edit a file".to_string(),
                        args: ctx.args.clone(),
                        info: ApprovalInfo {
                            tool_name: "edit".to_string(),
                            args_summary: "replace 1 with 2".to_string(),
                            risk: RiskLevel::Write,
                            trust_key: "edit:src/lib.rs".to_string(),
                            trust_levels: vec![],
                            trust_groups: vec![],
                        },
                    });
                    match receiver.await {
                        Ok(PermissionResponse::Allow | PermissionResponse::AllowSession { .. }) => {
                            None
                        }
                        Ok(PermissionResponse::Deny | PermissionResponse::DenyWithHint { .. })
                        | Err(_) => Some(PreToolUseResult {
                            block: true,
                            reason: Some("edit approval denied".to_string()),
                        }),
                    }
                })
            },
        ) as PreToolUseHook
    })
}

#[tokio::test]
async fn gui_runtime_completes_scripted_coding_turn_with_session_bound_approval() {
    let temp = tempfile::tempdir().unwrap();
    let source_dir = temp.path().join("src");
    std::fs::create_dir_all(&source_dir).unwrap();
    let source_path = source_dir.join("lib.rs");
    std::fs::write(&source_path, "1").unwrap();

    let sessions = temp.path().join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let pending: PendingApprovals = Arc::new(DashMap::new());
    let (request_tx, mut request_rx) = mpsc::unbounded_channel();
    let shared = SharedResources {
        cwd: temp.path().to_path_buf(),
        settings_manager: SettingsManager::load(temp.path().join("settings.json"), None, None)
            .unwrap(),
        resources: rozsa_app::resources::LoadedResources::default(),
        system_prompt: "test".to_string(),
        model: Mutex::new(test_model()),
        thinking_level: Mutex::new(ThinkingLevel::Off),
        pre_tool_use_factory: Some(approval_factory(pending.clone(), request_tx)),
        model_stream: Some(scripted_model(source_path.to_string_lossy().to_string())),
    };
    let created = shared.create_new_agent(&sessions, None).await.unwrap();
    let session_id = created.id.clone();
    let agent = Arc::new(created.agent);
    let prompt = {
        let agent = agent.clone();
        tokio::spawn(async move { agent.prompt("change src/lib.rs from 1 to 2").await.unwrap() })
    };

    let request = tokio::time::timeout(std::time::Duration::from_secs(2), request_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(request.session_id, session_id);
    assert_eq!(request.request_id, "edit-1");
    assert!(
        pending
            .remove(&permission_pending_key(
                "other-session",
                &request.request_id
            ))
            .is_none()
    );
    let (_, sender) = pending
        .remove(&permission_pending_key(
            &request.session_id,
            &request.request_id,
        ))
        .unwrap();
    sender.send(PermissionResponse::Allow).unwrap();

    let events = prompt.await.unwrap();
    assert_eq!(std::fs::read_to_string(&source_path).unwrap(), "2");
    assert!(
        events
            .iter()
            .any(|event| matches!(event, AgentEvent::AgentEnd { .. }))
    );
    assert!(events.iter().any(|event| matches!(
        event,
        AgentEvent::ToolExecutionEnd { tool_name, result, .. }
            if tool_name == "bash"
                && result.details["success"] == true
                && result.details["exit_code"] == 0
    )));
    assert!(agent.messages().await.iter().any(|message| {
        serde_json::to_string(message)
            .unwrap()
            .contains("Completed: source changed and verification passed.")
    }));
    let persisted = std::fs::read_to_string(Path::new(&created.path)).unwrap();
    assert!(persisted.contains("Completed: source changed and verification passed."));
}
