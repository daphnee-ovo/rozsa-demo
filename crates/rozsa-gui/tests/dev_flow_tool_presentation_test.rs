// FrameworkTree
// dev_flow_tool_presentation_test.rs
// ├── presentation()
// ├── ui_snapshot_serializes_presentations_by_tool_call_id()
// ├── frontend_renders_structured_summary_and_complete_raw_evidence()
// └── runtime_capture_is_bounded_persisted_and_never_reexecutes()

use std::collections::HashMap;
use std::process::Command;

use rozsa_app::dev_flow::{
    DevFlowPresentationAction, DevFlowPresentationItem, DevFlowPresentationItemKind,
    DevFlowToolPresentation,
};
use rozsa_gui::state::{ContextUsage, RuntimeState, TurnActivity, UiSnapshot};

fn presentation(action: DevFlowPresentationAction) -> DevFlowToolPresentation {
    DevFlowToolPresentation {
        action,
        items: vec![DevFlowPresentationItem {
            kind: DevFlowPresentationItemKind::Task,
            id: "TASK-T001".to_owned(),
            short_id: "T001".to_owned(),
            title: Some("Implement integration".to_owned()),
        }],
        details_unavailable: false,
    }
}

#[test]
fn ui_snapshot_serializes_presentations_by_tool_call_id() {
    let snapshot = UiSnapshot {
        session_id: "session".to_owned(),
        turn_id: 1,
        messages: Vec::new(),
        dev_flow_presentations: HashMap::from([(
            "call-1".to_owned(),
            presentation(DevFlowPresentationAction::Created),
        )]),
        is_streaming: false,
        model: None,
        thinking_effort: "off".to_owned(),
        session_name: None,
        cwd: "/tmp/project".to_owned(),
        git: None,
        context_usage: ContextUsage {
            percent: 0.0,
            tokens: 0,
            context_window: 0,
            input_tokens: 0,
            uncached_input_tokens: 0,
            cached_input_tokens: 0,
            output_tokens: 0,
        },
        runtime_state: RuntimeState {
            prompt_tokens: 0,
            completion_tokens: 0,
            session_total_tokens: 0,
        },
        turn_activity: TurnActivity::default(),
        turn_summaries: Vec::new(),
        queued_messages: Vec::new(),
        steering_conversation: Vec::new(),
        stream_update: false,
    };
    let value = serde_json::to_value(snapshot).unwrap();
    assert_eq!(value["devFlowPresentations"]["call-1"]["action"], "created");
    assert_eq!(
        value["devFlowPresentations"]["call-1"]["items"][0]["shortId"],
        "T001"
    );
}

#[test]
fn frontend_renders_structured_summary_and_complete_raw_evidence() {
    let source = include_str!("../frontend/app.js");
    let start = source.find("function formatDevFlowToolTitle(").unwrap();
    let end = source[start..].find("function formatToolArgs(").unwrap() + start;
    let script = format!(
        r#"
function escapeHtml(value) {{
  return String(value).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}}
{}
function check(condition, message) {{ if (!condition) throw new Error(message); }}
for (const [action, expected] of Object.entries({{created:'Created',claimed:'Claimed',completed:'Completed',closed:'Closed'}})) {{
  const title = formatDevFlowToolTitle({{ action, items: [
    {{kind:'task',shortId:'T001',title:'Task title'}},
    {{kind:'issue',shortId:'I002',title:null}},
  ] }});
  check(title.name === expected, action + ' label');
  check(title.arg === 'Task T001 Task title · Issue I002 Details unavailable', action + ' items');
}}
const evidence = renderDevFlowToolEvidence(
  {{arguments:{{command:'dow task create < request.json'}}}},
  {{output:'TASK-T001\nstderr evidence',details:{{exit_code:0,duration_ms:42,timeout_ms:120000,truncated:false,file_deltas:[{{path:'src/lib.rs',patch:'@@'}}]}}}}
);
for (const expected of ['$ dow task create &lt; request.json','exit 0','42ms','timeout 120000ms','not truncated','TASK-T001','stderr evidence','src/lib.rs','@@']) {{
  check(evidence.includes(expected), 'missing evidence: ' + expected);
}}
"#,
        &source[start..end]
    );
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("dev_flow_tool_presentation_test.js");
    std::fs::write(&path, script).unwrap();
    let output = Command::new("node")
        .arg(path)
        .output()
        .expect("Node.js is required for frontend behavior tests");
    assert!(
        output.status.success(),
        "frontend behavior failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn runtime_capture_is_bounded_persisted_and_never_reexecutes() {
    let runtime = include_str!("../src/dev_flow.rs");
    let events = include_str!("../src/events.rs");
    let commands = include_str!("../src/commands.rs");
    let frontend = include_str!("../frontend/app.js");
    let html = include_str!("../frontend/index.html");

    assert!(runtime.contains("pub async fn capture_tool_presentation"));
    assert!(runtime.contains("Duration::from_secs(2)"));
    assert!(runtime.contains("recognize_dow_bash(command"));
    assert!(events.contains("append_dev_flow_presentation(&record)"));
    assert!(events.contains("live.dev_flow_presentations"));
    assert!(commands.contains("rebuild_dev_flow_presentations"));
    assert!(commands.contains("manager.context_messages()"));
    assert!(frontend.contains("snap.devFlowPresentations"));
    assert!(frontend.contains("renderDevFlowToolEvidence(tc, result)"));
    assert!(html.contains(".dev-flow-tool-evidence"));
    for forbidden in ["Command::new(\"dow\")", "Command::new(command)"] {
        assert!(
            !runtime.contains(forbidden),
            "capture must not re-execute Bash"
        );
    }
}
