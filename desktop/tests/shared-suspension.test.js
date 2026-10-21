import assert from 'node:assert/strict';
import { getByRole, queryByRole } from '@testing-library/dom';
import React, { act } from 'react';
import { test } from 'vitest';
import { loadApp, withDOM } from './helpers/browser.js';

async function withRendererDOM(dom, action) {
  return withDOM(dom, async () => {
    const previous = Object.getOwnPropertyDescriptor(globalThis, 'IS_REACT_ACT_ENVIRONMENT');
    globalThis.IS_REACT_ACT_ENVIRONMENT = true;
    try { return await action(); }
    finally {
      if (previous) Object.defineProperty(globalThis, 'IS_REACT_ACT_ENVIRONMENT', previous);
      else delete globalThis.IS_REACT_ACT_ENVIRONMENT;
    }
  });
}

async function createRuntime() {
  const calls = [];
  const errors = [];
  const app = await loadApp({ html: `<div id="composer"><div id="msgInput" contenteditable="true">未发送 draft</div></div>
    <div id="autocomplete" role="listbox" aria-label="Command suggestions"></div><div id="forkPicker" hidden></div><div id="subagentPanel" hidden></div>`, globals: { testCalls: calls, testErrors: errors, assert } });
  const { mountSuggestionPopup } = await import('../renderer/scripts/components/composer.js');
  const suggestionPopup = await withRendererDOM(app.dom, () => mountSuggestionPopup(app.document.getElementById('autocomplete')));
  app.window.RozsaComposer = {
    renderSuggestions: suggestionPopup.render.bind(suggestionPopup),
    clearSuggestions: suggestionPopup.clear.bind(suggestionPopup),
    scrollSuggestionTo: suggestionPopup.scrollTo.bind(suggestionPopup),
  };
  app.evaluate("showError=message=>testErrors.push(message);invoke=async command=>{testCalls.push(command);throw Error('unexpected IPC: '+command);};");
  const run = code => app.evaluate('(async()=>{' + code + '})()');
  const input = app.document.getElementById('msgInput');
  const popup = app.document.getElementById('autocomplete');
  const close = async () => {
    await withRendererDOM(app.dom, () => act(() => suggestionPopup.dispose()));
    app.close();
  };
  return { app, run, input, popup, calls, errors, close };
}

test('suspended mouse, slash, native and selection paths do not call IPC or change draft', async () => {
  const { app, run, input, popup, calls, errors, close } = await createRuntime();
  try {
    for (const action of ['attach-file', 'attach-directory', 'running-send-mode']) {
      const button = app.document.createElement('button');
      button.dataset.action = action;
      app.document.getElementById('composer').append(button);
    }
    await run(`configureConversationSuspensions();
      await attachFileReference();await attachDirectoryReference();await attachReference('file');
      insertFileReferences(['/tmp/file']);handleNativeFileDrag('enter');handleNativeFileDrag('drop',{paths:['/tmp/file']});
      await showForkPicker();await forkAtMessage(0);await showSubagentPanel();
      await handleSlashAction('forkPicker');await handleSlashAction('subagentPanel');
      await dispatchSlashCommand('/fork');await dispatchSlashCommand('/subagents');isStreaming=true;await sendMessage();`);
    const composer = await import('../renderer/scripts/components/composer.js');
    composer.init({ boundModules: new Set(), byId: id => app.document.getElementById(id), attachFileReference() {}, attachDirectoryReference() {}, insertSlashCommandPrefix() {}, showModelPicker() {}, addMessage() {}, updateAbortButton() {} });
    for (const action of ['attach-file', 'attach-directory', 'running-send-mode']) {
      const button = app.document.querySelector(`[data-action="${action}"]`);
      button.click();
    }
    assert.equal(input.textContent, '未发送 draft');
    assert.equal(queryByRole(popup, 'option'), null);
    assert.deepEqual(calls, []);
    assert.deepEqual(errors, []);
  } finally { await close(); }
});

test('late autocomplete and model results cannot replace active-session suggestions', async () => {
  const { app, run, input, popup, calls, errors, close } = await createRuntime();
  try {
    const pending = [];
    app.window.testPending = pending;
    app.window.testCalls = calls;
    await run(`
      showError=message=>testErrors.push(message);
      activeSessionId='session-a';activeSessionIdx=0;models=[];
      sessions=[{id:'session-a',path:'/a'},{id:'session-b',path:'/b'}];
      getInputText=()=>document.getElementById('msgInput').textContent;getInputCursor=()=>0;
      renderModelSelector=()=>{};
      displayPermPanelIfNeeded=()=>{};
      invoke=(command,args)=>{
        testCalls.push([command,args]);
        if(command==='switch_session'){activeSessionId='session-b';return Promise.resolve();}
        return new Promise(resolve=>testPending.push({command,args,resolve}));
      };
    `);

    let autocompleteA;
    await withRendererDOM(app.dom, () => act(() => { autocompleteA = run('return await updateAutocomplete();'); }));
    assert.equal(calls.at(-1)[0], 'autocomplete_input');
    assert.equal(calls.at(-1)[1].text, '未发送 draft');
    assert.equal(calls.at(-1)[1].cursor, 0);
    await withRendererDOM(app.dom, () => act(async () => { await run('await doSwitchSession(1);'); }));
    let autocompleteB;
    await withRendererDOM(app.dom, () => act(() => { autocompleteB = run('return await updateAutocomplete();'); }));
    assert.equal(calls.at(-1)[0], 'autocomplete_input');
    assert.equal(calls.at(-1)[1].text, '未发送 draft');
    assert.equal(calls.at(-1)[1].cursor, 0);
    await withRendererDOM(app.dom, async () => {
      await act(async () => {
        pending[1].resolve({ items: [{ value: '/current', label: 'Current command', description: 'session B' }], prefix: '/' });
        await autocompleteB;
      });
      getByRole(popup, 'option', { name: /Current command/ });
      await act(async () => {
        pending[0].resolve({ items: [{ value: '/stale', label: 'Stale command' }], prefix: '/' });
        await autocompleteA;
      });
      getByRole(popup, 'option', { name: /Current command/ });
      assert.equal(queryByRole(popup, 'option', { name: /Stale command/ }), null);
    });

    await run('models=[];activeSessionId="session-a";activeSessionIdx=0;');
    let pickerA;
    await withRendererDOM(app.dom, () => act(() => { pickerA = run('return await showModelPicker();'); }));
    assert.equal(calls.at(-1)[0], 'list_models');
    await withRendererDOM(app.dom, () => act(async () => { await run('await doSwitchSession(1);'); }));
    let pickerB;
    await withRendererDOM(app.dom, () => act(() => { pickerB = run('return await showModelPicker();'); }));
    assert.equal(calls.at(-1)[0], 'list_models');
    await withRendererDOM(app.dom, async () => {
      await act(async () => {
        pending[3].resolve([{ id: 'model-b', name: 'Current model', provider: 'provider-b' }]);
        await pickerB;
      });
      getByRole(popup, 'option', { name: /Current model.*provider-b/ });
      await act(async () => {
        pending[2].resolve([{ id: 'model-a', name: 'Stale model', provider: 'provider-a' }]);
        await pickerA;
      });
      getByRole(popup, 'option', { name: /Current model.*provider-b/ });
      assert.equal(queryByRole(popup, 'option', { name: /Stale model/ }), null);
    });
    assert.equal(input.textContent, '未发送 draft');
    assert.deepEqual(errors, []);
  } finally { await close(); }
});

test('running stop preserves draft and passive queue state while keyboard send is paused', async () => {
  const { app, run, input, calls, close } = await createRuntime();
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
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(input.textContent, '未发送 draft');
    assert.deepEqual(calls, []);
    const queued = app.document.createElement('div');
    queued.id = 'queuedMessages'; queued.hidden = true;
    const steering = app.document.createElement('div');
    steering.id = 'steeringConversation'; steering.hidden = true;
    app.document.body.append(queued, steering);
    app.window.RozsaConversation = { runningPanelMarkup: panel => panel?.items?.join('\n') || '' };
    await run("renderRunningMessages(['existing queue'],[{text:'existing steer'}]);");
    assert.equal(queued.textContent, 'existing queue');
    assert.equal(steering.textContent, 'existing steer · Awaiting tool result');
    await run('renderRunningMessages([],[]);');
    assert.equal(queued.hidden, true);
    assert.equal(steering.hidden, true);
  } finally { await close(); }
});

test('idle submit and slash prefix remain available', async () => {
  const { app, run, input, close } = await createRuntime();
  try {
    app.window.sent = [];
    await withRendererDOM(app.dom, () => act(async () => {
      await run(`nativeSplitMode=true;getInputText=input=>input.textContent;setInputText=(input,text)=>{input.textContent=text;};updateInputHighlight=()=>{};invoke=async(command,args)=>{sent.push([command,args]);};await sendMessage();`);
    }));
    assert.deepEqual(app.window.sent.map(([command, args]) => [command, args.message]), [['send_message', '未发送 draft']]);
    assert.equal(input.textContent, '');
    await run('insertInputText=text=>{document.getElementById("msgInput").textContent+=text;};insertSlashCommandPrefix();');
    assert.equal(input.textContent, '/');
    input.textContent = '/unknown selected';
    await withRendererDOM(app.dom, () => act(async () => {
      await run(`invoke=async(command,args)=>{
        sent.push([command,args]);
        if(command==='dispatch_slash_command')return {handled:false};
      };
      await sendMessage();`);
    }));
    assert.deepEqual(app.window.sent.map(([command, args]) => [command, args.message ?? args.text]), [
      ['send_message', '未发送 draft'],
      ['dispatch_slash_command', '/unknown selected'],
      ['send_message', '/unknown selected'],
    ]);
    assert.equal(input.textContent, '');
  } finally { await close(); }
});
