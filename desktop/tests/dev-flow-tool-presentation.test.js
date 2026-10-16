import test from 'node:test';
import assert from 'node:assert/strict';
import { loadApp, readFrontend } from './helpers/browser.js';

function appWithSettings() {
  const app = loadApp();
  app.evaluate("devFlowSettings = { project: { dashboardUrl: 'http://127.0.0.1:9800/' } }");
  return app;
}

test('successful Dev Flow bash mutations get semantic presentations', () => {
  const app = appWithSettings();
  try {
    const parse = (command, output = 'claim output\nstderr evidence') => app.evaluate(`parseDevFlowBashPresentation(${JSON.stringify({ name: 'bash', arguments: { command } })}, ${JSON.stringify({ isError: false, output, details: { success: true, exit_code: 0, duration_ms: 42, timeout_ms: 120000, truncated: false } })})`);
    const expected = [
      ['dow claim TASK-T001 --timeout 300 -H 2>&1', 'claimed', 'TASK-T001'],
      ['dow task create < request.json', 'created', 'TASK-T002', 'TASK-T002\n'],
      ['dow task done T002 -H 2>&1', 'completed', 'TASK-T002'],
      ['dow issue close ISSUE-I003 -H 2>&1', 'closed', 'ISSUE-I003'],
      ['dow --human task update --title "renamed" T004 --priority P1 2>&1', 'updated', 'TASK-T004'],
      ['dow issue remove ISSUE-I005 --confirm IRM-123 -H 2>&1', 'removed', 'ISSUE-I005'],
      ['dow task reopen T006 --confirm TRO-123 --human 2>&1', 'reopened', 'TASK-T006'],
    ];
    for (const [command, action, id, output] of expected) {
      const parsed = parse(command, output);
      assert.equal(parsed.action, action, command);
      assert.equal(parsed.items[0].id, id, command);
    }
    const released = parse('dow claim --revoke TASK-T001 ISSUE-I003 --timeout=300 -H 2>&1');
    assert.equal(released.action, 'released');
    assert.equal(released.items.length, 2);
    assert.equal(app.evaluate("formatBashDevFlowTitle(parseDevFlowBashPresentation({name:'bash',arguments:{command:'dow claim --revoke -H 2>&1'}},{isError:false,output:'',details:{success:true,exit_code:0,truncated:false}})).arg"), 'all claims');
    assert.equal(parse('dow task done T007 T008 --human 2>&1').items.length, 2);
    for (const command of ['dow task show TASK-T001 -H 2>&1', 'dow status set --phase DEV -H 2>&1', 'dow task update T001 2&>1', 'dow task update T001 2>&1; echo "==="']) {
      assert.equal(parse(command), null, command);
    }
    assert.equal(app.evaluate(`parseDevFlowBashPresentation(${JSON.stringify({ name: 'bash', arguments: { command: 'dow claim TASK-T001' } })}, {isError:true,output:'',details:{success:false,exit_code:1}})`), null);
    assert.equal(app.evaluate("devFlowTitleEndpoint('task', 'TASK-T001').pathname"), '/api/v1/tasks/TASK-T001');
    assert.equal(app.evaluate("devFlowTitleEndpoint('issue', 'ISSUE-I003').pathname"), '/api/v1/issues/ISSUE-I003');
    assert.match(app.evaluate('fetchDevFlowTitle.toString()'), /method: 'GET'/);
    assert.doesNotMatch(app.evaluate('fetchDevFlowTitle.toString()'), /payload\?\.items/);
  } finally { app.close(); }
});

test('tool titles normalize names and keep semantic arguments', () => {
  const app = loadApp();
  try {
    assert.equal(app.evaluate("formatToolArgs({name:'bash',arguments:{command:'dow task list -H 2>&1'}})"), 'dow task list -H 2>&1');
    assert.equal(app.evaluate("formatToolTitle({name:'bash',arguments:{command:'dow task list -H 2>&1'}}).arg"), 'dow task list -H 2>&1');
    const assertions = [
      ["formatToolTitle({name:'READ',arguments:{file_path:'src/main.rs',offset:20,limit:61}}).name", 'Read'],
      ["formatToolTitle({name:'READ',arguments:{file_path:'src/main.rs',offset:20,limit:61}}).arg", 'src/main.rs · lines 20–80'],
      ["formatToolTitle({name:'bash',arguments:{command:'pwd',description:'Inspect cwd'}}).arg", 'Inspect cwd'],
      ["formatToolTitle({name:'bash',arguments:{command:'pwd'}}).arg", 'pwd'],
      ["formatToolTitle({name:'BASH',arguments:{command:'grep AgentEvent crates'}}).name", 'Bash'],
      ["formatToolTitle({name:'subagent',arguments:{action:'spawn',name:'reviewer'}}).arg", 'reviewer'],
      ["formatToolTitle({name:'ask_user_question',arguments:{questions:[{question:'Choose model'}]}}).arg", 'Choose model'],
      ["formatToolTitle({name:'unknown',arguments:{a:1}}).arg", '{"a":1}'],
    ];
    for (const [expression, value] of assertions) assert.equal(app.evaluate(expression), value);
    assert.equal(app.evaluate("formatToolTitle({name:'BASH',arguments:{command:'grep AgentEvent crates'}}).arg"), 'grep AgentEvent crates');
    assert.equal(app.evaluate("formatToolTitle({name:'subagent',arguments:{action:'spawn',name:'reviewer'}}).name"), 'Spawn');
    app.evaluate("window.presentation = {action:'created',items:[{kind:'task',id:'TASK-T001',shortId:'T001',title:'Keep shared title'}]}");
    assert.equal(app.evaluate('formatBashDevFlowTitle(window.presentation).name'), 'Created');
    assert.equal(app.evaluate('formatBashDevFlowTitle(window.presentation).arg'), 'Task T001 Keep shared title');
  } finally { app.close(); }
});

test('bash evidence retains command output and execution facts', () => {
  const app = appWithSettings();
  try {
    app.evaluate("window.evidence = renderBashToolEvidence({arguments:{command:'dow task create < request.json'}},{output:'TASK-T001\\nstderr evidence',details:{exit_code:0,duration_ms:42,timeout_ms:120000,truncated:false}})");
    const evidence = app.evaluate('window.evidence');
    for (const expected of ['$ dow task create &lt; request.json', 'exit 0', '42ms', 'timeout 120000ms', 'not truncated', 'TASK-T001', 'stderr evidence']) assert.ok(evidence.includes(expected), expected);
  } finally { app.close(); }
});

test('covered Dev Flow calls use semantic titles and shared output evidence', () => {
  const app = appWithSettings();
  try {
    app.evaluate(`
      window.requests = []; window.cards = []; window.evidenceCalls = [];
      window.RozsaConversation = { createMessageFrame(role, body) { const el = document.createElement('div'); el.innerHTML = body; return el; } };
      window.RozsaTools = {
        toolView() { return {disposition:'covered',kind:'output',label:'Output',output:'Claim accepted',status:'s-success'}; },
        evidenceMarkup(label, output) { window.evidenceCalls.push([label, output]); return 'SHARED OUTPUT'; },
        evidenceViewMarkup() { throw new Error('wrong evidence renderer'); },
        toolMarkup(id, name, arg, output, status, options) { window.cards.push({id,name,arg,status,options}); return '<div tabindex="0"></div>'; },
      };
      devFlowTitleForItem = () => 'Keep shared title';
      requestDevFlowTitles = (id, presentation) => window.requests.push([id, presentation]);
      activeSessionId = 'session-1';
      window.result = {isError:false,output:'Claim accepted',details:{success:true,exit_code:0}};
      window.rendered = renderMessage({kind:'standard',message:{role:'assistant',content:[{type:'toolCall',id:'call-1',name:'bash',arguments:{command:'dow claim TASK-T001'}}]}},{'call-1':window.result});
    `);
    const card = app.window.cards[0];
    assert.equal(card.name, 'Claimed');
    assert.equal(card.arg, 'Task T001 Keep shared title');
    assert.equal(card.status, 's-success');
    assert.equal(card.options.bodyHtml, 'SHARED OUTPUT');
    assert.equal(card.options.className, 'dev-flow-tool-call');
    assert.deepEqual(JSON.parse(JSON.stringify(app.window.evidenceCalls)), [['Output', 'Claim accepted']]);
    assert.equal(app.window.requests.length, 1);
    assert.equal(app.window.requests[0][0], 'call-1');
    assert.equal(app.window.requests[0][1].items[0].id, 'TASK-T001');
    assert.ok(app.window.rendered.innerHTML.includes('data-session-id="session-1"'));
  } finally { app.close(); }
});

test('structured tool evidence retains output for read, write, edit, bash, subagent, questions, and errors', () => {
  const app = loadApp();
  try {
    app.evaluate(`window.RozsaTools = {
      codeViewMarkup() { return '<div class="code-view"><span class="code-line">code</span></div>'; },
      diffViewMarkup() { return '<div class="diff-view"><span class="diff-add"></span><span class="diff-del"></span></div>'; },
    };`);
    const render = (toolCall, result) => app.evaluate(`renderToolEvidence(${JSON.stringify(toolCall)}, ${JSON.stringify(result)}, null)`);
    const read = render({name:'read',arguments:{file_path:'src/main.rs',offset:20,limit:61}}, {isError:false,output:'  42 | let answer = 1;\n  43 | answer += 1;',details:{total_lines:100,output_lines:2,truncated:true,truncated_by:'lines'}});
    for (const value of ['File src/main.rs', 'Requested lines 20–80', 'truncated by lines', 'line numbers preserved from tool output', '42 | let answer = 1;']) assert.ok(read.html.includes(value), value);
    const write = render({name:'WRITE',arguments:{file_path:'src/new.rs',content:'fn main() {}'}}, {isError:false,output:'Wrote file',details:{file_path:'src/new.rs',bytes_written:13,line_count:1,file_deltas:[{path:'src/new.rs',status:'added',after:'fn main() {}',added:1,deleted:0}]}});
    for (const value of ['File src/new.rs', 'created', '13 bytes', 'code-view', 'code-line', 'Wrote file']) assert.ok(write.html.includes(value), value);
    const edit = render({name:'edit',arguments:{file_path:'src/lib.rs'}}, {isError:false,output:'Edited file',details:{file_path:'src/lib.rs',replacements:2,file_deltas:[{path:'src/lib.rs',status:'modified',patch:'@@ -1 +1 @@\n-old\n+new',added:1,deleted:1}]}});
    for (const value of ['File src/lib.rs', '2 replacements', 'diff-view', 'diff-add', 'diff-del', 'Edited file']) assert.ok(edit.html.includes(value), value);
    const bash = render({name:'bash',arguments:{command:'ls src'}}, {isError:false,output:'main.rs\nlib.rs',details:{exit_code:0,duration_ms:12,timeout_ms:120000,truncated:false}});
    for (const value of ['$ ls src', 'exit 0', 'main.rs']) assert.ok(bash.html.includes(value), value);
    const subagent = render({name:'subagent',arguments:{action:'spawn',name:'reviewer',system_prompt:'do not render this prompt'}}, {isError:false,output:'review complete',details:{action:'spawn',id:'subagent-1',name:'reviewer',status:'completed',model_id:'gpt-test',model_provider:'openai',thinking_effort:'medium'}});
    for (const value of ['Spawn', 'reviewer', 'status completed', 'model openai/gpt-test', 'review complete']) assert.ok(subagent.html.includes(value), value);
    assert.ok(!subagent.html.includes('do not render this prompt'));
    const pending = render({name:'ASK_USER_QUESTION',arguments:{questions:[{header:'Model',question:'Which model? ',options:[]}]}}, null);
    for (const value of ['1 question', 'waiting for answer', 'Which model?']) assert.ok(pending.html.includes(value), value);
    assert.ok(!pending.html.includes('options'));
    const answered = render({name:'ask_user_question',arguments:{questions:[{header:'Model',question:'Which model?',options:[]}]}}, {isError:false,output:'{"answers":{"Model":"GPT-5"}}',details:{answers:{Model:'GPT-5'}}});
    for (const value of ['answered', 'Model', 'GPT-5']) assert.ok(answered.html.includes(value), value);
    assert.ok(!answered.html.includes('{"answers"'));
    const error = render({name:'unknown_tool',arguments:{value:'x'}}, {isError:true,output:'permission denied',details:{}});
    assert.ok(error.html.includes('error') && error.html.includes('permission denied'));
    assert.doesNotMatch(readFrontend('app.js'), /render(Search|Ls)ToolEvidence/);
  } finally { app.close(); }
});

test('tool icons normalize names and status classes have styles', () => {
  const app = loadApp();
  try {
    assert.equal(app.evaluate("toolIcon('BASH')"), app.evaluate("toolIcon('bash')"));
    assert.match(app.evaluate("toolIcon('BASH')"), /<svg/);
  }
  finally { app.close(); }
  const css = readFrontend('styles/features/tools.css');
  for (const state of ['s-success', 's-error', 's-running']) assert.ok(css.includes(`.tool-call-status.${state}`), state);
});
