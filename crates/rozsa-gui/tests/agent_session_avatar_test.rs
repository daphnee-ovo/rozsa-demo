use std::process::Command;

#[test]
fn frontend_groups_consecutive_agent_messages_without_repeating_identity() {
    let source = include_str!("../frontend/app.js");
    let start = source
        .find("function isAssistantMessageRaw(")
        .expect("assistant grouping helper should exist");
    let render_start = source
        .find("function renderMessage(")
        .expect("renderMessage should exist");
    let end = source[render_start..]
        .find("function parseDevFlowBashPresentation(")
        .map(|offset| render_start + offset)
        .expect("renderMessage should precede tool presentation helpers");
    let script = format!(
        r#"
const document = {{ createElement: () => ({{ className: '', innerHTML: '', textContent: '' }}) }};
function escapeHtml(value) {{ return String(value ?? '').replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;'); }}
function extractText(content) {{ return Array.isArray(content) ? content.filter(block => block.type === 'text').map(block => block.text).join('\\n') : ''; }}
function extractThinking() {{ return null; }}
function renderMarkdown(text) {{ return String(text ?? ''); }}
function trackTool() {{}}
function isToolCallExpanded() {{ return false; }}
function parseDevFlowBashPresentation() {{ return null; }}
function requestDevFlowTitles() {{}}
function resolveToolTitle() {{ return {{ name: 'Tool', arg: '' }}; }}
{}
{}
function check(condition, message) {{ if (!condition) throw new Error(message); }}
const assistant = role => ({{kind:'standard',message:{{role}}}});
const messages = [assistant('assistant'), assistant('assistant'), assistant('user'), assistant('assistant')];
check(assistantMessageGroupPosition(messages, 0) === 'start', 'first assistant starts a group');
check(assistantMessageGroupPosition(messages, 1) === 'continuation', 'consecutive assistant continues a group');
check(assistantMessageGroupPosition(messages, 2) === 'standalone', 'user message is not part of an assistant group');
check(assistantMessageGroupPosition(messages, 3) === 'start', 'assistant after user starts a new group');
const raw = {{kind:'standard',message:{{role:'assistant',content:[{{type:'text',text:'done'}}]}}}};
const initial = renderMessage(raw, {{}}, false, null, null, false, true);
const continuation = renderMessage(raw, {{}}, false, null, null, false, false);
check((initial.innerHTML.match(/msg-avatar/g) || []).length === 1, 'group start renders one avatar');
check(initial.innerHTML.includes('msg-role'), 'group start renders role');
check(!continuation.innerHTML.includes('msg-avatar'), 'continuation omits avatar');
check(!continuation.innerHTML.includes('msg-role'), 'continuation omits role');
check(continuation.className.includes('msg-assistant-continuation'), 'continuation uses group class');
"#,
        &source[start..render_start],
        &source[render_start..end]
    );
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("agent_session_avatar_test.js");
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
fn frontend_styles_continuation_messages_as_one_agent_group() {
    let html = include_str!("../frontend/index.html");
    let continuation = html
        .split(".msg-assistant-continuation {")
        .nth(1)
        .and_then(|tail| tail.split('}').next())
        .expect("continuation style should exist");
    assert!(continuation.contains("padding-left: 40px"));
    assert!(continuation.contains("margin-top: -12px"));
}
