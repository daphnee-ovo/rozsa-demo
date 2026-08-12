// FrameworkTree
// dev_flow_tool_presentation_test.rs
// ├── frontend_parses_bash_messages_into_structured_summary_and_evidence()
// ├── frontend_formats_lowercase_bash_tool_arguments_as_command()
// ├── frontend_formats_known_tool_titles_with_semantic_arguments()
// ├── frontend_normalizes_tool_icons_and_status_styles()
// └── frontend_renders_structured_tool_evidence_without_losing_output()

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
for (const expected of ['$ dow task create &lt; request.json','exit 0','42ms','timeout 120000ms','not truncated','TASK-T001','stderr evidence']) {{
  check(evidence.includes(expected), 'missing evidence: ' + expected);
}}
check(!evidence.includes('File delta') && !evidence.includes('file_deltas'), 'bash file delta should be hidden');
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
    let start = source.find("function normalizeToolName(").unwrap();
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

#[test]
fn frontend_formats_known_tool_titles_with_semantic_arguments() {
    let source = include_str!("../frontend/app.js");
    let start = source.find("function normalizeToolName(").unwrap();
    let end = source[start..].find("function renderCodeView(").unwrap() + start;
    let script = format!(
        r#"
function formatBashDevFlowTitle() {{ return {{name:'Created',arg:'Task T001'}}; }}
{}
function check(condition, message) {{ if (!condition) throw new Error(message); }}
check(formatToolTitle({{name:'READ', arguments:{{file_path:'src/main.rs',offset:20,limit:61}}}}).name === 'Read', 'read label');
check(formatToolTitle({{name:'READ', arguments:{{file_path:'src/main.rs',offset:20,limit:61}}}}).arg === 'src/main.rs · lines 20–80', 'read range');
check(formatToolTitle({{name:'bash', arguments:{{command:'pwd',description:'Inspect cwd'}}}}).arg === 'Inspect cwd', 'bash description priority');
check(formatToolTitle({{name:'bash', arguments:{{command:'pwd'}}}}).arg === 'pwd', 'bash command fallback');
check(resolveToolTitle({{name:'bash',arguments:{{command:'dow task create',description:'description'}}}}, {{action:'created'}}).name === 'Created', 'dev-flow title priority');
const bashRead = formatToolTitle({{name:'BASH', arguments:{{command:'grep AgentEvent crates'}}}});
check(bashRead.name === 'Bash' && bashRead.arg === 'grep AgentEvent crates', 'bash read summary');
const subagent = formatToolTitle({{name:'subagent', arguments:{{action:'spawn',name:'reviewer'}}}});
check(subagent.name === 'Spawn' && subagent.arg === 'reviewer', 'subagent summary');
check(formatToolTitle({{name:'ask_user_question', arguments:{{questions:[{{question:'Choose model'}}]}}}}).arg === 'Choose model', 'question summary');
check(formatToolTitle({{name:'unknown', arguments:{{a:1}}}}).arg === '{{"a":1}}', 'unknown fallback');
"#,
        &source[start..end]
    );
    let directory = tempfile::tempdir().unwrap();
    let path = directory
        .path()
        .join("dev_flow_semantic_tool_titles_test.js");
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
fn frontend_normalizes_tool_icons_and_status_styles() {
    let source = include_str!("../frontend/app.js");
    let css = include_str!("../frontend/styles/layout/app-shell.css");
    assert!(
        source.contains("const toolName = normalizeToolName(name);"),
        "tool icons should normalize tool names"
    );
    assert!(
        css.contains(".tool-call-status.s-success")
            && css.contains(".tool-call-status.s-error")
            && css.contains(".tool-call-status.s-running"),
        "tool-call status classes should have visual styles"
    );
}

#[test]
fn frontend_renders_structured_tool_evidence_without_losing_output() {
    let source = include_str!("../frontend/app.js");
    assert!(!source.contains("renderSearchToolEvidence"));
    assert!(!source.contains("renderLsToolEvidence"));
    let start = source.find("function renderBashToolEvidence(").unwrap();
    let end = source[start..].find("function extractText(").unwrap() + start;
    let script = format!(
        r#"
function escapeHtml(value) {{
  return String(value ?? '').replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}}
{}
function check(condition, message) {{ if (!condition) throw new Error(message); }}
const read = renderToolEvidence(
  {{name:'read', arguments:{{file_path:'src/main.rs',offset:20,limit:61}}}},
  {{isError:false,output:'  42 | let answer = 1;\\n  43 | answer += 1;',details:{{total_lines:100,output_lines:2,truncated:true,truncated_by:'lines'}}}},
  null
);
for (const expected of ['File src/main.rs','Requested lines 20–80','truncated by lines','line numbers preserved from tool output','  42 | let answer = 1;']) check(read.html.includes(expected), 'read evidence: ' + expected);

const write = renderToolEvidence(
  {{name:'WRITE', arguments:{{file_path:'src/new.rs',content:'fn main() {{}}'}}}},
  {{isError:false,output:'Wrote file',details:{{file_path:'src/new.rs',bytes_written:13,line_count:1,file_deltas:[{{path:'src/new.rs',status:'added',after:'fn main() {{}}',added:1,deleted:0}}]}}}},
  null
);
for (const expected of ['File src/new.rs','created','13 bytes','code-view','code-line','Wrote file']) check(write.html.includes(expected), 'write evidence: ' + expected);

const edit = renderToolEvidence(
  {{name:'edit', arguments:{{file_path:'src/lib.rs'}}}},
  {{isError:false,output:'Edited file',details:{{file_path:'src/lib.rs',replacements:2,file_deltas:[{{path:'src/lib.rs',status:'modified',patch:'@@ -1 +1 @@\n-old\n+new',added:1,deleted:1}}]}}}},
  null
);
for (const expected of ['File src/lib.rs','2 replacements','diff-view','diff-add','diff-del','Edited file']) check(edit.html.includes(expected), 'edit evidence: ' + expected);

const bash = renderToolEvidence(
  {{name:'bash', arguments:{{command:'ls src'}}}},
  {{isError:false,output:'main.rs\\nlib.rs',details:{{exit_code:0,duration_ms:12,timeout_ms:120000,truncated:false}}}},
  null
);
for (const expected of ['$ ls src','exit 0','main.rs']) check(bash.html.includes(expected), 'bash evidence: ' + expected);

const subagent = renderToolEvidence(
  {{name:'subagent', arguments:{{action:'spawn',name:'reviewer',system_prompt:'do not render this prompt'}}}},
  {{isError:false,output:'review complete',details:{{action:'spawn',id:'subagent-1',name:'reviewer',status:'completed',model_id:'gpt-test',model_provider:'openai',thinking_effort:'medium'}}}},
  null
);
for (const expected of ['Spawn','reviewer','status completed','model openai/gpt-test','review complete']) check(subagent.html.includes(expected), 'subagent evidence: ' + expected);
check(!subagent.html.includes('do not render this prompt'), 'subagent prompt leaked');

const questionPending = renderToolEvidence(
  {{name:'ASK_USER_QUESTION', arguments:{{questions:[{{header:'Model',question:'Which model? ',options:[]}}]}}}},
  null,
  null
);
for (const expected of ['1 question','waiting for answer','Which model?']) check(questionPending.html.includes(expected), 'question pending: ' + expected);
check(!questionPending.html.includes('options'), 'question raw JSON leaked');
const questionDone = renderToolEvidence(
  {{name:'ask_user_question', arguments:{{questions:[{{header:'Model',question:'Which model?',options:[]}}]}}}},
  {{isError:false,output:'{{"answers":{{"Model":"GPT-5"}}}}',details:{{answers:{{Model:'GPT-5'}}}}}},
  null
);
for (const expected of ['answered','Model','GPT-5']) check(questionDone.html.includes(expected), 'question answer: ' + expected);
check(!questionDone.html.includes('{{"answers"'), 'question result JSON duplicated');

const error = renderToolEvidence(
  {{name:'unknown_tool',arguments:{{value:'x'}}}},
  {{isError:true,output:'permission denied',details:{{}}}},
  null
);
check(error.html.includes('error') && error.html.includes('permission denied'), 'error fallback');
"#,
        &source[start..end]
    );
    let directory = tempfile::tempdir().unwrap();
    let path = directory
        .path()
        .join("dev_flow_structured_tool_evidence_test.js");
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
