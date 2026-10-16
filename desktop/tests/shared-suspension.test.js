import assert from 'node:assert/strict';
import { test } from 'node:test';
import { loadApp } from './helpers/browser.js';

async function createRuntime() {
  const calls = [];
  const app = await loadApp({ html: `<div id="composer"><div id="msgInput" contenteditable="true">未发送 draft</div></div>
    <div id="autocomplete" hidden></div><div id="forkPicker" hidden></div><div id="subagentPanel" hidden></div>`, globals: { testCalls: calls, assert } });
  app.evaluate("invoke=async command=>{testCalls.push(command);throw Error('unexpected IPC: '+command);};");
  const run = code => app.evaluate('(async()=>{' + code + '})()');
  const input = app.document.getElementById('msgInput');
  const popup = app.document.getElementById('autocomplete');
  return { app, run, input, popup, calls };
}

test('suspended mouse, slash, native and selection paths do not call IPC or change draft', async () => {
  const { app, run, input, popup, calls } = await createRuntime();
  try {
    await run(`await attachFileReference();await attachDirectoryReference();await attachReference('file');
      insertFileReferences(['/tmp/file']);handleNativeFileDrag('enter');handleNativeFileDrag('drop',{paths:['/tmp/file']});
      await updateAutocomplete();acHighlight(1);selectAutocomplete(0);assert.equal(navigateAutocomplete('down'),false);assert.equal(confirmAutocomplete(),false);
      await showModelPicker();selectModel(0);await showForkPicker();await forkAtMessage(0);await showSubagentPanel();
      await handleSlashAction('modelPicker');await handleSlashAction('forkPicker');await handleSlashAction('subagentPanel');
      await dispatchSlashCommand('/model actual-model');await dispatchSlashCommand('/fork');await dispatchSlashCommand('/subagents');isStreaming=true;await sendMessage();`);
    const composer = await import('../renderer/scripts/components/composer.js');
    composer.init({ boundModules: new Set(), byId: id => app.document.getElementById(id), attachFileReference() {}, attachDirectoryReference() {}, insertSlashCommandPrefix() {}, showModelPicker() {}, addMessage() {}, updateAbortButton() {} });
    for (const action of ['attach-file', 'attach-directory', 'model-picker', 'send']) {
      const button = app.document.createElement('button');
      button.dataset.action = action;
      app.document.getElementById('composer').append(button);
      button.click();
    }
    assert.equal(input.textContent, '未发送 draft');
    assert.equal(popup.innerHTML, '');
    assert.equal(popup.hidden, true);
    assert.deepEqual(calls, []);
    await run(`assert.equal(suspendedConversationCommand('a /model model-id'),true);assert.equal(suspendedConversationCommand('/MODEL'),true);assert.equal(suspendedConversationCommand('/model-name'),false);assert.equal(suspendedConversationCommand('https://example.test/model'),false);`);
  } finally { app.close(); }
});

test('late autocomplete, picker and panel results cannot restore suspended UI', async () => {
  const { app, run, input, popup } = await createRuntime();
  try {
    for (const [operation, start, result] of [
      ['attachments', "attachReference('file')", '/tmp/late'],
      ['autocomplete', 'updateAutocomplete()', { items: [{ value: '/late' }], prefix: '/', highlightRanges: [] }],
      ['modelPicker', 'showModelPicker()', [{ id: 'late' }]],
      ['fork', 'showForkPicker()', [{ messageIndex: 0, label: 'late' }]],
      ['subagents', 'showSubagentPanel()', [{ name: 'late' }]],
    ]) {
      let release;
      app.window.pending = new Promise(resolve => { release = resolve; });
      await run('conversationOperationSuspended=()=>false;invoke=()=>pending;getInputText=()=>document.getElementById("msgInput").textContent;getInputCursor=()=>0;renderModelSelector=()=>{};');
      const pending = run('await ' + start + ';');
      await run('conversationOperationSuspended=operation=>Object.hasOwn(conversationSuspensions,operation);');
      release(result);
      await pending;
      assert.equal(input.textContent, '未发送 draft', operation);
      assert.equal(popup.innerHTML, '', operation);
    }
  } finally { app.close(); }
});

test('running stop preserves draft and passive queue state while keyboard send is paused', async () => {
  const { app, run, input, calls } = await createRuntime();
  try {
    const button = app.document.createElement('button');
    const mode = app.document.createElement('div');
    mode.id = 'runningSendMode';
    app.document.body.append(button, mode);
    app.window.RozsaComposer = { renderSendState(button, action) { button.dataset.sendAction = action; } };
    app.window.testSendButton = button;
    app.window.testRunningMode = mode;
    await run('isStreaming=true;document.querySelector=selector=>selector.includes("send-btn")?testSendButton:null;updateAbortButton();');
    assert.equal(button.dataset.sendAction, 'stop');
    assert.equal(mode.hidden, true);
    await run('notificationErrorList=()=>({hidden:true});');
    input.focus();
    const keydown = new app.window.KeyboardEvent('keydown', { key: 'Enter', bubbles: true });
    input.dispatchEvent(keydown);
    input.dispatchEvent(new app.window.KeyboardEvent('keydown', { key: 'p', ctrlKey: true, bubbles: true }));
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(input.textContent, '未发送 draft');
    assert.deepEqual(calls, []);
    const queued = app.document.createElement('div');
    queued.id = 'queuedMessages'; queued.hidden = true;
    const steering = app.document.createElement('div');
    steering.id = 'steeringConversation'; steering.hidden = true;
    app.document.body.append(queued, steering);
    app.window.RozsaConversation = { runningPanelMarkup: panel => JSON.stringify(panel) };
    await run("renderRunningMessages(['existing queue'],[{text:'existing steer'}]);");
    assert.match(queued.innerHTML, /existing queue/);
    assert.match(steering.innerHTML, /existing steer/);
    await run('renderRunningMessages([],[]);');
    assert.equal(queued.hidden, true);
    assert.equal(steering.hidden, true);
  } finally { app.close(); }
});

test('idle submit and slash prefix remain available', async () => {
  const { app, run, input } = await createRuntime();
  try {
    app.window.sent = [];
    await run(`nativeSplitMode=true;getInputText=input=>input.textContent;setInputText=(input,text)=>{input.textContent=text;};updateInputHighlight=()=>{};invoke=async(command,args)=>{sent.push([command,args]);};await sendMessage();`);
    assert.deepEqual(app.window.sent.map(([command, args]) => [command, args.message]), [['send_message', '未发送 draft']]);
    assert.equal(input.textContent, '');
    await run('insertInputText=text=>{document.getElementById("msgInput").textContent+=text;};insertSlashCommandPrefix();');
    assert.equal(input.textContent, '/');
    input.textContent = '/model selected';
    await run('await sendMessage();');
    assert.equal(app.window.sent.length, 1);
    assert.equal(input.textContent, '/model selected');
  } finally { app.close(); }
});
