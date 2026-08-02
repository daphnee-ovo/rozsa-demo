// FrameworkTree
// dev_flow_tool_presentation_test.rs
// ├── frontend_parses_bash_messages_into_structured_summary_and_evidence()
// └── frontend_formats_lowercase_bash_tool_arguments_as_command()

use std::process::Command;

#[test]
fn frontend_parses_bash_messages_into_structured_summary_and_evidence() {
    let source = include_str!("../frontend/app.js");
    let start = source
        .find("function parseDevFlowBashPresentation(")
        .unwrap();
    let end = source[start..].find("function formatToolArgs(").unwrap() + start;
    let script = format!(
        r#"
function escapeHtml(value) {{
  return String(value).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}}
let devFlowSettings = {{ project: {{ dashboardUrl: 'http://127.0.0.1:9800/' }} }};
{}
function check(condition, message) {{ if (!condition) throw new Error(message); }}
const result = {{isError:false,output:'claim output\nstderr evidence',details:{{success:true,exit_code:0,duration_ms:42,timeout_ms:120000,truncated:false,file_deltas:[{{path:'src/lib.rs',patch:'@@'}}]}}}};
const claimed = parseDevFlowBashPresentation(
  {{name:'bash',arguments:{{command:'dow claim TASK-T001 --timeout 300 -H 2>&1'}}}},
  result
);
check(claimed.action === 'claimed', 'claim action');
check(claimed.items[0].id === 'TASK-T001', 'claim id');
check(formatBashDevFlowTitle(claimed).name === 'Claimed', 'claim label');
check(formatBashDevFlowTitle(claimed).arg === 'Task T001 Details unavailable', 'missing title');
const created = parseDevFlowBashPresentation(
  {{name:'Bash',arguments:{{command:'dow task create < request.json'}}}},
  {{...result,output:'TASK-T002\n'}}
);
check(created.action === 'created' && created.items[0].shortId === 'T002', 'create action');
const completed = parseDevFlowBashPresentation(
  {{name:'bash',arguments:{{command:'dow task done T002 -H 2>&1'}}}}, result
);
check(completed.action === 'completed' && completed.items[0].kind === 'task', 'done action');
const closed = parseDevFlowBashPresentation(
  {{name:'bash',arguments:{{command:'dow issue close I003 -H 2>&1'}}}}, result
);
check(closed.action === 'closed' && closed.items[0].kind === 'issue', 'close action');
const updated = parseDevFlowBashPresentation(
  {{name:'bash',arguments:{{command:'dow --human task update --title "renamed" T004 --priority P1 2>&1'}}}}, result
);
check(updated.action === 'updated' && updated.items[0].id === 'TASK-T004', 'update action');
const removed = parseDevFlowBashPresentation(
  {{name:'bash',arguments:{{command:'dow issue remove ISSUE-I005 --confirm IRM-123 -H 2>&1'}}}}, result
);
check(removed.action === 'removed' && removed.items[0].id === 'ISSUE-I005', 'remove action');
const reopened = parseDevFlowBashPresentation(
  {{name:'bash',arguments:{{command:'dow task reopen T006 --confirm TRO-123 --human 2>&1'}}}}, result
);
check(reopened.action === 'reopened' && reopened.items[0].kind === 'task', 'reopen action');
const released = parseDevFlowBashPresentation(
  {{name:'bash',arguments:{{command:'dow claim --revoke TASK-T001 ISSUE-I003 --timeout=300 -H 2>&1'}}}}, result
);
check(released.action === 'released' && released.items.length === 2, 'release action');
const releasedAll = parseDevFlowBashPresentation(
  {{name:'bash',arguments:{{command:'dow claim --revoke -H 2>&1'}}}}, result
);
check(formatBashDevFlowTitle(releasedAll).arg === 'all claims', 'release all action');
const multiDone = parseDevFlowBashPresentation(
  {{name:'bash',arguments:{{command:'dow task done T007 T008 --human 2>&1'}}}}, result
);
check(multiDone.action === 'completed' && multiDone.items.length === 2, 'multi-id action');
check(parseDevFlowBashPresentation(
  {{name:'bash',arguments:{{command:'dow task show TASK-T001 -H 2>&1'}}}}, result
) === null, 'read-only task command');
check(parseDevFlowBashPresentation(
  {{name:'bash',arguments:{{command:'dow status set --phase DEV -H 2>&1'}}}}, result
) === null, 'status command');
check(parseDevFlowBashPresentation(
  {{name:'bash',arguments:{{command:'dow task update T001 2&>1'}}}}, result
) === null, 'file redirect is not captured');
check(parseDevFlowBashPresentation(
  {{name:'bash',arguments:{{command:'dow task update T001 2>&1; echo "==="'}}}}, result
) === null, 'compound command');
check(parseDevFlowBashPresentation(
  {{name:'bash',arguments:{{command:'dow claim TASK-T001'}}}},
  {{...result,isError:true,details:{{...result.details,success:false}}}}
) === null, 'failed command');
check(devFlowTitleEndpoint('task', 'TASK-T001').pathname === '/api/v1/tasks/TASK-T001', 'task detail endpoint');
check(devFlowTitleEndpoint('issue', 'ISSUE-I003').pathname === '/api/v1/issues/ISSUE-I003', 'issue detail endpoint');
check(fetchDevFlowTitle.toString().includes("method: 'GET'"), 'title lookup uses GET');
check(!fetchDevFlowTitle.toString().includes('payload?.items'), 'title lookup is not a list GET');
const evidence = renderBashToolEvidence(
  {{arguments:{{command:'dow task create < request.json'}}}},
  {{...result,output:'TASK-T001\nstderr evidence'}}
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
fn frontend_formats_lowercase_bash_tool_arguments_as_command() {
    let source = include_str!("../frontend/app.js");
    let start = source.find("function formatToolArgs(").unwrap();
    let end = source[start..].find("function renderCodeView(").unwrap() + start;
    let script = format!(
        r#"
{}
function check(condition, message) {{ if (!condition) throw new Error(message); }}
const tool = {{name:'bash', arguments:{{command:'dow task list -H 2>&1'}}}};
check(formatToolArgs(tool) === 'dow task list -H 2>&1', 'lowercase bash args');
check(formatToolTitle(tool).arg === 'dow task list -H 2>&1', 'lowercase bash title');
"#,
        &source[start..end]
    );
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("dev_flow_lowercase_bash_test.js");
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
