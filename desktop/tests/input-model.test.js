import test from 'node:test';
import assert from 'node:assert/strict';
import React, { act } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { ComposerInput, mountComposerInput, mountSuggestionPopup } from '../renderer/scripts/components/composer.tsx';
import { createDOM, loadApp, withDOM } from './helpers/browser.js';

const composerMarkup = '<div id="composer" class="input-wrapper"><div id="msgInput" contenteditable="true"></div>' +
  '<div id="autocomplete" role="listbox"></div></div>';

function createRuntime(invoke = async () => ({ prefix: '', items: [], validMatch: false, highlightRanges: [] })) {
  const app = loadApp({ html: composerMarkup, globals: { invoke } });
  const renders = [];
  const clears = [];
  const scrolls = [];
  app.window.RozsaComposer = {
    renderSuggestions(items, selectedIndex, onSelect, onHover, mode) {
      renders.push({ items: [...items], selectedIndex, onSelect, onHover, mode });
    },
    clearSuggestions() { clears.push(true); },
    scrollSuggestionTo(index) { scrolls.push(index); },
  };
  app.evaluate(`
    invoke = window.invoke;
    showError = message => { (window.errors ||= []).push(String(message)); };
    activeSessionId = 'session-a';
    activeSessionIdx = 0;
  `);
  return { ...app, renders, clears, scrolls };
}

test('React composer keeps the established input, actions, labels, and Lucide icons', () => {
  const markup = renderToStaticMarkup(React.createElement(ComposerInput));
  for (const fragment of [
    'id="msgInput"',
    'contentEditable="true"',
    'aria-multiline="true"',
    'class="input-toolbar"',
    'id="attachFileButton"',
    'id="attachDirectoryButton"',
    'data-action="slash-command"',
    'id="modelSelector"',
    'id="thinkingEffort"',
    'id="runningSendMode"',
    'id="send-btn"',
    'class="lucide lucide-paperclip',
    'class="lucide lucide-folder',
    'class="lucide lucide-slash',
  ]) assert.ok(markup.includes(fragment), fragment);
});

test('React composer portal preserves the existing shell and mounts input as a direct composer child', async () => {
  const dom = createDOM('<div id="composer"><div id="queuedMessages"></div><div id="inputMount" class="composer-input-mount"></div></div>');
  try {
    await withDOM(dom, async () => {
      const host = dom.window.document.getElementById('composer');
      const mount = dom.window.document.getElementById('inputMount');
      let dispose;
      const wasActEnvironment = globalThis.IS_REACT_ACT_ENVIRONMENT;
      globalThis.IS_REACT_ACT_ENVIRONMENT = true;
      try {
        await act(async () => {
          dispose = mountComposerInput(mount);
        });
        const input = host.querySelector('#msgInput');
        assert.equal(host.firstElementChild.id, 'queuedMessages');
        assert.equal(mount.parentElement, host);
        assert.equal(input.parentElement, host);
        await act(async () => dispose());
        assert.equal(host.querySelector('#msgInput'), null);
        assert.equal(host.querySelector('#queuedMessages') !== null, true);
      } finally {
        if (wasActEnvironment === undefined) delete globalThis.IS_REACT_ACT_ENVIRONMENT;
        else globalThis.IS_REACT_ACT_ENVIRONMENT = wasActEnvironment;
      }
    });
  } finally { dom.window.close(); }
});

test('React suggestion popup preserves hover, selection, keyboard scroll, and clear behavior', async () => {
  const dom = createDOM('<div id="autocomplete" role="listbox"></div>');
  const calls = [];
  try {
    await withDOM(dom, async () => {
      const target = dom.window.document.getElementById('autocomplete');
      const popup = mountSuggestionPopup(target);
      const wasActEnvironment = globalThis.IS_REACT_ACT_ENVIRONMENT;
      globalThis.IS_REACT_ACT_ENVIRONMENT = true;
      try {
        await act(async () => {
          popup.render([{ value: '/task ', label: '/task', description: 'Run task' }], 0,
            index => calls.push(['select', index]), index => calls.push(['hover', index]));
          assert.equal(target.classList.contains('visible'), true);
          assert.equal(target.querySelector('.ac-item')?.getAttribute('aria-selected'), 'true');
          const item = target.querySelector('.ac-item');
          const click = new dom.window.MouseEvent('mousedown', { bubbles: true, cancelable: true });
          item.dispatchEvent(click);
          item.dispatchEvent(new dom.window.MouseEvent('mouseover', { bubbles: true }));
          item.scrollIntoView = options => calls.push(['scroll', options.block]);
          popup.scrollTo(0);
          assert.equal(click.defaultPrevented, true);
          assert.deepEqual(calls, [['select', 0], ['hover', 0], ['scroll', 'nearest']]);
          popup.clear();
          assert.equal(target.classList.contains('visible'), false);
          assert.equal(target.childElementCount, 0);
          popup.dispose();
        });
      } finally {
        if (wasActEnvironment === undefined) delete globalThis.IS_REACT_ACT_ENVIRONMENT;
        else globalThis.IS_REACT_ACT_ENVIRONMENT = wasActEnvironment;
      }
    });
  } finally { dom.window.close(); }
});

test('slash suggestions use the React popup controller and support keyboard selection', async () => {
  const app = createRuntime(async () => ({
    prefix: '/mo',
    items: [{ value: '/model ', label: '/model', description: 'Select model' }],
    validMatch: false,
    highlightRanges: [],
  }));
  try {
    app.document.getElementById('msgInput').textContent = '/mo';
    await app.evaluate('updateAutocomplete()');
    assert.equal(app.renders.at(-1).mode, 'commands');
    assert.equal(app.renders.at(-1).items[0].value, '/model ');
    app.evaluate("navigateAutocomplete('down')");
    assert.equal(app.renders.at(-1).selectedIndex, 0);
    assert.deepEqual(app.scrolls, [0]);
    app.evaluate('confirmAutocomplete()');
    assert.equal(app.document.getElementById('msgInput').textContent, '/model ');
    await new Promise(resolve => setImmediate(resolve));
  } finally { app.close(); }
});

test('model picker is keyboard navigable and unsupported thinking efforts stay hidden', async () => {
  const app = createRuntime();
  try {
    const input = app.document.getElementById('msgInput');
    input.textContent = '切换模型时保留草稿';
    input.focus();
    app.evaluate('setInputSelection(document.getElementById("msgInput"), 6)');
    app.evaluate(`
      models = [
        { id: 'reasoning-model', name: 'Reasoning Model', provider: 'test', reasoning: true,
          thinkingEffortMap: { low: 'low', medium: null, high: null, xhigh: null, max: null } }
      ];
      currentSettings = { model_id: 'reasoning-model' };
      selectModel = index => { window.selectedModelIndex = index; };
    `);
    await app.evaluate('showModelPicker()');
    assert.equal(input.textContent, '切换模型时保留草稿');
    assert.deepEqual(JSON.parse(JSON.stringify(app.evaluate('getInputSelection(document.getElementById("msgInput"))'))), { start: 6, end: 6 });
    assert.equal(app.renders.at(-1).mode, 'models');
    assert.equal(app.renders.at(-1).items[0].label, 'Reasoning Model');
    app.evaluate("navigateAutocomplete('down'); confirmAutocomplete()");
    assert.equal(app.window.selectedModelIndex, 0);
    const efforts = app.evaluate('supportedThinkingEfforts().map(option => option.value)');
    assert.deepEqual(Array.from(efforts), ['off', 'low']);
    assert.equal(input.textContent, '切换模型时保留草稿');
    assert.deepEqual(JSON.parse(JSON.stringify(app.evaluate('getInputSelection(document.getElementById("msgInput"))'))), { start: 6, end: 6 });
  } finally { app.close(); }
});

test('autocomplete uses UTF-16 ranges and keeps the caret after highlighting astral text', () => {
  const app = createRuntime();
  try {
    const input = app.document.getElementById('msgInput');
    input.textContent = '😀 /model tail';
    app.evaluate('setInputSelection(document.getElementById("msgInput"), 9); updateInputHighlight([{ start: 3, end: 9 }])');
    assert.equal(input.textContent, '😀 /model tail');
    assert.equal(input.querySelector('.valid-token-text')?.textContent, '/model');
    assert.deepEqual(JSON.parse(JSON.stringify(app.evaluate('getInputSelection(document.getElementById("msgInput"))'))), { start: 9, end: 9 });
  } finally { app.close(); }
});

test('autocomplete renders the Go-provided range for an exact skill command', async () => {
  const skillCommand = '/skill:impeccable';
  const app = createRuntime(async () => ({
    prefix: skillCommand,
    items: [{ value: `${skillCommand} `, label: skillCommand, description: 'Improve a frontend interface' }],
    validMatch: true,
    highlightRanges: [{ start: 0, end: skillCommand.length }],
  }));
  try {
    const input = app.document.getElementById('msgInput');
    input.textContent = skillCommand;
    input.focus();
    app.evaluate('setInputSelection(document.getElementById("msgInput"), document.getElementById("msgInput").textContent.length)');
    await app.evaluate('updateAutocomplete()');
    assert.equal(input.querySelector('.valid-token-text')?.textContent, skillCommand);
    assert.equal(app.document.querySelector('.input-wrapper')?.classList.contains('valid-token'), true);
  } finally { app.close(); }
});

test('IME composition keeps Enter out of message submission', () => {
  const app = createRuntime();
  const sends = [];
  try {
    app.window.sendForTest = () => sends.push(true);
    app.evaluate('capturingKeyBindingAction = false; isInputComposing = true; sendMessage = window.sendForTest');
    app.document.dispatchEvent(new app.window.KeyboardEvent('keydown', {
      key: 'Enter', keyCode: 229, isComposing: true, bubbles: true, cancelable: true,
    }));
    assert.deepEqual(sends, []);
  } finally { app.close(); }
});

test('IME composition blocks direct autocomplete selection and direct message submission', () => {
  const calls = [];
  const app = createRuntime(async (...args) => { calls.push(args); });
  try {
    const input = app.document.getElementById('msgInput');
    input.textContent = '/ta';
    app.evaluate(`
      isInputComposing = true;
      acVisible = true;
      acItems = [{ value: '/task ', label: '/task' }];
      acPrefix = '/ta';
      selectAutocomplete(0);
      confirmAutocomplete();
      sendMessage();
    `);
    assert.equal(input.textContent, '/ta');
    assert.deepEqual(calls, []);
  } finally { app.close(); }
});

test('late autocomplete and model refresh results cannot replace the active session UI', async () => {
  let resolveAutocomplete;
  const app = createRuntime(() => new Promise(resolve => { resolveAutocomplete = resolve; }));
  try {
    const input = app.document.getElementById('msgInput');
    input.textContent = '保留草稿 /mo';
    app.evaluate('setInputSelection(document.getElementById("msgInput"), 8)');
    const pending = app.evaluate('updateAutocomplete()');
    app.evaluate('activeSessionId = "session-b"; activeSessionIdx = 1');
    resolveAutocomplete({ prefix: '/mo', items: [{ value: '/model ', label: '/model' }], highlightRanges: [] });
    await pending;
    assert.equal(app.renders.length, 0);
    assert.equal(input.textContent, '保留草稿 /mo');

    let resolveFirst;
    let resolveSecond;
    app.window.invoke = name => name === 'list_models'
      ? new Promise(resolve => { (resolveFirst ? resolveSecond = resolve : resolveFirst = resolve); })
      : Promise.resolve({});
    app.evaluate(`
      invoke = window.invoke;
      acMode = 'models';
      acVisible = true;
      modelPickerItems = [{ value: 'old-model', label: 'Old Model' }];
      const firstRefresh = refreshModelRegistry();
      const secondRefresh = refreshModelRegistry();
      window.refreshes = [firstRefresh, secondRefresh];
    `);
    resolveSecond([{ id: 'new-model', name: 'New Model', provider: 'test' }]);
    await app.window.refreshes[1];
    resolveFirst([{ id: 'stale-model', name: 'Stale Model', provider: 'test' }]);
    await app.window.refreshes[0];
    assert.equal(app.evaluate('models[0].id'), 'new-model');
    assert.equal(app.renders.at(-1).mode, 'models');
    assert.equal(app.renders.at(-1).items[0].value, 'new-model');
    assert.equal(input.textContent, '保留草稿 /mo');
  } finally { app.close(); }
});

test('a model selection response for the previous session cannot update the new session UI', async () => {
  let resolveSelection;
  const calls = [];
  const app = createRuntime((method) => {
    calls.push(method);
    if (method === 'switch_model') return new Promise(resolve => { resolveSelection = resolve; });
    return Promise.resolve({});
  });
  try {
    const selection = app.evaluate('onModelChange("model-a", "provider-a")');
    app.evaluate('activeSessionId = "session-b"; activeSessionIdx = 1');
    resolveSelection(true);
    await selection;
    assert.deepEqual(calls, ['switch_model']);
  } finally { app.close(); }
});

test('a late model catalog response cannot open the picker in a newly selected session', async () => {
  let resolveModels;
  const app = createRuntime(() => new Promise(resolve => { resolveModels = resolve; }));
  try {
    app.evaluate('models = []');
    const picker = app.evaluate('showModelPicker()');
    app.evaluate('activeSessionId = "session-b"; activeSessionIdx = 1');
    resolveModels([{ id: 'late-model', name: 'Late Model', provider: 'test' }]);
    await picker;
    assert.equal(app.renders.length, 0);
    assert.equal(app.evaluate('acVisible'), false);
  } finally { app.close(); }
});

test('dismissing the composer invalidates a pending autocomplete response', async () => {
  let resolveAutocomplete;
  const app = createRuntime(() => new Promise(resolve => { resolveAutocomplete = resolve; }));
  try {
    app.document.getElementById('msgInput').textContent = '/mo';
    const pending = app.evaluate('updateAutocomplete()');
    app.evaluate('hideAutocomplete()');
    resolveAutocomplete({ prefix: '/mo', items: [{ value: '/model ', label: '/model' }], highlightRanges: [] });
    await pending;
    assert.equal(app.renders.length, 0);
    assert.equal(app.evaluate('acVisible'), false);
  } finally { app.close(); }
});

test('model registry empty and error states close the picker and report the failure', async () => {
  const app = createRuntime(async () => []);
  try {
    const input = app.document.getElementById('msgInput');
    input.textContent = '中文草稿保留';
    app.evaluate('setInputSelection(document.getElementById("msgInput"), 4)');
    app.evaluate("models = [{ id: 'old-model', name: 'Old Model', provider: 'test' }]; acMode = 'models'; acVisible = true; modelPickerItems = [{ value: 'old-model', label: 'Old Model' }]");
    await app.evaluate('refreshModelRegistry()');
    assert.equal(app.evaluate('acVisible'), false);
    assert.ok(app.window.errors.includes('No models available'));
    assert.equal(input.textContent, '中文草稿保留');
    assert.deepEqual(JSON.parse(JSON.stringify(app.evaluate('getInputSelection(document.getElementById("msgInput"))'))), { start: 4, end: 4 });

    app.window.invoke = async () => { throw new Error('registry unavailable'); };
    app.evaluate('invoke = window.invoke; models = [{ id: "old-model", name: "Old Model", provider: "test" }]; acMode = "models"; acVisible = true');
    await app.evaluate('refreshModelRegistry()');
    assert.equal(app.evaluate('acVisible'), false);
    assert.ok(app.window.errors.some(message => message.includes('registry unavailable')));
    assert.equal(input.textContent, '中文草稿保留');
    assert.deepEqual(JSON.parse(JSON.stringify(app.evaluate('getInputSelection(document.getElementById("msgInput"))'))), { start: 4, end: 4 });
  } finally { app.close(); }
});

test('session drafts keep text, send mode, and UTF-16 selection through restore', () => {
  const app = createRuntime();
  try {
    const input = app.document.getElementById('msgInput');
    const mode = app.document.createElement('select');
    mode.id = 'runningSendMode';
    mode.innerHTML = '<option value="queue">Queue</option><option value="steer">Steer</option>';
    app.document.getElementById('composer').append(mode);
    input.textContent = '😀 draft text';
    app.evaluate('setInputSelection(document.getElementById("msgInput"), 3, 8); persistConversationState = () => {}; captureSessionDraft("session-a")');
    assert.deepEqual(JSON.parse(JSON.stringify(app.evaluate('sessionDraftState["session-a"]'))), {
      text: '😀 draft text', selectionStart: 3, selectionEnd: 8, runningSendMode: 'queue',
    });
    input.textContent = 'replacement';
    app.evaluate('restoreSessionDraft("session-a")');
    assert.equal(input.textContent, '😀 draft text');
    assert.deepEqual(JSON.parse(JSON.stringify(app.evaluate('getInputSelection(document.getElementById("msgInput"))'))), { start: 3, end: 8 });
  } finally { app.close(); }
});

test('capturing a draft after focus leaves the composer preserves its last saved selection', () => {
  const app = createRuntime();
  try {
    const input = app.document.getElementById('msgInput');
    input.textContent = 'A draft here';
    const outside = app.document.createElement('button');
    outside.id = 'outsideSelection';
    outside.textContent = 'switch session';
    app.document.body.append(outside);
    app.evaluate('persistConversationState = () => {}; setInputSelection(document.getElementById("msgInput"), 2, 7); captureSessionDraft("session-a")');
    app.evaluate(`(()=>{const target=document.getElementById('outsideSelection'),range=document.createRange();range.selectNodeContents(target);range.collapse(false);const selection=window.getSelection();selection.removeAllRanges();selection.addRange(range);captureSessionDraft('session-a')})()`);
    assert.deepEqual(JSON.parse(JSON.stringify(app.evaluate('sessionDraftState["session-a"]'))), {
      text: 'A draft here', selectionStart: 2, selectionEnd: 7, runningSendMode: 'queue',
    });
  } finally { app.close(); }
});

test('session snapshot changes save and restore the draft belonging to each session', () => {
  const app = createRuntime();
  try {
    const input = app.document.getElementById('msgInput');
    const mode = app.document.createElement('select');
    mode.id = 'runningSendMode';
    mode.innerHTML = '<option value="queue">Queue</option><option value="steer">Steer</option>';
    app.document.getElementById('composer').append(mode);
    const interruptionRoot = app.document.createElement('div');
    interruptionRoot.id = 'interruptionReactRoot';
    app.document.body.append(interruptionRoot);
    input.textContent = '草稿A';
    app.evaluate(`
      persistConversationState = () => {};
      setInputSelection(document.getElementById('msgInput'), 2);
      sessionDraftState['session-b'] = { text: '草稿B', selectionStart: 2, selectionEnd: 3, runningSendMode: 'steer' };
      renderState({ sessionId: 'session-b', isStreaming: false, messages: [], queuedMessages: [], steeringConversation: [] });
    `);
    assert.equal(input.textContent, '草稿B');
    assert.equal(mode.value, 'steer');
    assert.deepEqual(JSON.parse(JSON.stringify(app.evaluate('getInputSelection(document.getElementById("msgInput"))'))), { start: 2, end: 3 });

    input.textContent = '更新后的草稿B';
    app.evaluate(`
      setInputSelection(document.getElementById('msgInput'), 5);
      renderState({ sessionId: 'session-a', isStreaming: false, messages: [], queuedMessages: [], steeringConversation: [] });
    `);
    assert.equal(app.evaluate('sessionDraftState["session-b"].text'), '更新后的草稿B');
    assert.equal(input.textContent, '草稿A');
    assert.equal(mode.value, 'queue');

    app.evaluate("renderState({ sessionId: 'session-b', isStreaming: false, messages: [], queuedMessages: [], steeringConversation: [] })");
    assert.equal(input.textContent, '更新后的草稿B');
    assert.deepEqual(JSON.parse(JSON.stringify(app.evaluate('getInputSelection(document.getElementById("msgInput"))'))), { start: 5, end: 5 });
  } finally { app.close(); }
});

test('a session change during IME commits the old draft before restoring the selected session', async () => {
  const app = createRuntime();
  try {
    const input = app.document.getElementById('msgInput');
    input.textContent = 'draft';
    app.evaluate(`
      persistConversationState = () => {};
      updateAbortButton = () => {};
      flushPendingGuiSceneTransition = () => {};
      sessionDraftState['session-b'] = { text: 'new draft', selectionStart: 4, selectionEnd: 6, runningSendMode: 'queue' };
      setInputSelection(document.getElementById('msgInput'), 5);
      handleCompositionStart();
    `);
    input.textContent = 'draft 中';
    app.evaluate(`
      setInputSelection(document.getElementById('msgInput'), 7);
      activeSessionId = 'session-b';
      restoreSessionDraft('session-b');
    `);
    assert.equal(input.textContent, 'draft 中', 'restore waits until the IME commit');
    app.evaluate('handleCompositionEnd(document.getElementById("msgInput"))');
    await new Promise(resolve => setTimeout(resolve, 10));
    assert.equal(app.evaluate('sessionDraftState["session-a"].text'), 'draft 中');
    assert.equal(input.textContent, 'new draft');
    assert.deepEqual(JSON.parse(JSON.stringify(app.evaluate('getInputSelection(document.getElementById("msgInput"))'))), { start: 4, end: 6 });
  } finally { app.close(); }
});
