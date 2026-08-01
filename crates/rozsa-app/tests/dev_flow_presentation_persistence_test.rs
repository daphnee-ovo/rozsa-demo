// FrameworkTree
// dev_flow_presentation_persistence_test.rs
// ├── project()
// ├── presentation()
// ├── bash_messages()
// ├── snapshot()
// ├── typed_record_round_trips_without_entering_context_messages()
// ├── rebuild_requires_paired_bash_evidence_and_exact_project_identity()
// └── old_sessions_and_unknown_custom_metadata_remain_loadable()

use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use rozsa_app::dev_flow::{
    BashExecutionEvidence, DevFlowIssueStatus, DevFlowPresentationRecord, DevFlowProjectKey,
    DevFlowRevisionKey, DevFlowSnapshot, DevFlowTask, DevFlowTaskStatus,
    rebuild_dev_flow_presentations, recognize_dow_bash,
};
use rozsa_app::session::manager::SessionManager;
use rozsa_model::types::{
    Api, AssistantMessage, ContentBlock, Message, Provider, StopReason, ToolCall,
    ToolResultMessage, Usage,
};

fn project(root: PathBuf, branch: &str) -> DevFlowProjectKey {
    DevFlowProjectKey {
        root,
        revision: DevFlowRevisionKey::NamedBranch(branch.to_owned()),
    }
}

fn presentation() -> rozsa_app::dev_flow::DevFlowToolPresentation {
    recognize_dow_bash(
        "dow task create",
        None,
        &BashExecutionEvidence {
            success: true,
            exit_code: Some(0),
            truncated: false,
            stdout: "TASK-T001".to_owned(),
        },
    )
    .unwrap()
}

fn bash_messages(tool_call_id: &str) -> [Message; 2] {
    [
        Message::Assistant(AssistantMessage {
            content: vec![ContentBlock::ToolCall(ToolCall {
                id: tool_call_id.to_owned(),
                name: "bash".to_owned(),
                arguments: serde_json::json!({"command": "dow task create"}),
            })],
            api: Api::OpenAICompletions,
            provider: Provider::OpenAI,
            model: "test".to_owned(),
            response_model: None,
            response_id: None,
            usage: Usage::default(),
            stop_reason: StopReason::ToolUse,
            error_message: None,
            timestamp: 1,
        }),
        Message::ToolResult(ToolResultMessage {
            tool_call_id: tool_call_id.to_owned(),
            tool_name: "bash".to_owned(),
            content: vec![ContentBlock::Text {
                text: "TASK-T001".to_owned(),
                signature: None,
            }],
            details: serde_json::json!({
                "success": true,
                "exit_code": 0,
                "truncated": false
            }),
            is_error: false,
            timestamp: 2,
        }),
    ]
}

fn snapshot(title: &str) -> DevFlowSnapshot {
    DevFlowSnapshot {
        revision: 7,
        project: rozsa_app::dev_flow::DevFlowProjectStatus {
            name: Some("test".to_owned()),
            phase: Some("DEV".to_owned()),
            mode: None,
            version: None,
            goals_minor: None,
            updated: None,
        },
        tasks: vec![DevFlowTask {
            id: "TASK-T001".to_owned(),
            title: title.to_owned(),
            status: DevFlowTaskStatus::Pending,
            priority: Some("P1".to_owned()),
            complexity: Some("S".to_owned()),
            task_type: Some("feat".to_owned()),
            refs: None,
            depends_on: Vec::new(),
            done_when: Vec::new(),
            files_create: Vec::new(),
            files_modify: Vec::new(),
            files_test: Vec::new(),
        }],
        issues: vec![rozsa_app::dev_flow::DevFlowIssue {
            id: "ISSUE-I001".to_owned(),
            title: "unused".to_owned(),
            status: DevFlowIssueStatus::Open,
            severity: None,
            description: None,
            files_create: Vec::new(),
            files_modify: Vec::new(),
        }],
        received_at: UNIX_EPOCH,
        stale: false,
    }
}

#[test]
fn typed_record_round_trips_without_entering_context_messages() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("session.jsonl");
    let mut manager = SessionManager::create(
        &path,
        "session".to_owned(),
        directory.path().to_string_lossy().into_owned(),
        None,
    )
    .unwrap();
    let messages = bash_messages("call-1");
    for message in messages.clone() {
        manager.append_message(message).unwrap();
    }
    let key = project(directory.path().to_path_buf(), "main");
    let mut confirmed = presentation();
    confirmed.items[0].title = Some("Persisted title".to_owned());
    confirmed.details_unavailable = false;
    let record = DevFlowPresentationRecord::new("call-1".to_owned(), &key, confirmed, 123);
    manager.append_dev_flow_presentation(&record).unwrap();
    drop(manager);

    let restored = SessionManager::open(&path).unwrap();
    assert_eq!(restored.context_messages().len(), 2);
    assert_eq!(
        restored.dev_flow_presentation_records().unwrap(),
        vec![record]
    );
    let raw = std::fs::read_to_string(path).unwrap();
    assert!(raw.contains("\"type\":\"custom\""));
    assert!(raw.contains("\"customType\":\"dev_flow_presentation\""));
}

#[test]
fn rebuild_requires_paired_bash_evidence_and_exact_project_identity() {
    let root = PathBuf::from("/tmp/project-a");
    let key = project(root.clone(), "main");
    let record = DevFlowPresentationRecord::new("call-1".to_owned(), &key, presentation(), 100);
    let messages = bash_messages("call-1");
    let rebuilt = rebuild_dev_flow_presentations(
        [record.clone()],
        &messages,
        Some((&key, &snapshot("Exact title"))),
    );
    assert_eq!(
        rebuilt["call-1"].items[0].title.as_deref(),
        Some("Exact title")
    );
    assert!(!rebuilt["call-1"].details_unavailable);

    for mismatch in [
        project(PathBuf::from("/tmp/project-b"), "main"),
        project(root, "feature"),
    ] {
        let rebuilt = rebuild_dev_flow_presentations(
            [record.clone()],
            &messages,
            Some((&mismatch, &snapshot("Wrong title"))),
        );
        assert!(rebuilt["call-1"].items[0].title.is_none());
        assert!(rebuilt["call-1"].details_unavailable);
    }

    assert!(
        rebuild_dev_flow_presentations([record.clone()], &messages[..1], None).is_empty(),
        "missing ToolResult rejects orphan metadata"
    );
    assert!(
        rebuild_dev_flow_presentations([record], &messages[1..], None).is_empty(),
        "missing ToolCall rejects orphan metadata"
    );
}

#[test]
fn old_sessions_and_unknown_custom_metadata_remain_loadable() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("old.jsonl");
    let mut manager = SessionManager::create(
        &path,
        "old".to_owned(),
        directory.path().to_string_lossy().into_owned(),
        None,
    )
    .unwrap();
    manager
        .append_custom(
            "future_extension".to_owned(),
            Some(serde_json::json!({"v": 1})),
        )
        .unwrap();
    drop(manager);

    let restored = SessionManager::open(path).unwrap();
    assert!(restored.dev_flow_presentation_records().unwrap().is_empty());
    assert!(restored.context_messages().is_empty());
}
