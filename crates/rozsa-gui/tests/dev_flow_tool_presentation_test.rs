// FrameworkTree
// dev_flow_tool_presentation_test.rs
// └── frontend_renders_structured_summary_and_complete_raw_evidence()

use std::process::Command;

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
