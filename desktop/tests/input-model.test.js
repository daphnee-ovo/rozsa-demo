import { test } from 'vitest';
import assert from 'node:assert/strict';
import { getByRole, queryByRole } from '@testing-library/dom';
import React, { act } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { ComposerInput, mountComposerInput, mountSuggestionPopup } from '../renderer/scripts/components/composer.tsx';
import { init as initComposer } from '../renderer/scripts/components/composer.tsx';
import { createDOM, loadApp, withDOM } from './helpers/browser.js';

const composerMarkup = '<div id="composer" class="input-wrapper"><div id="msgInput" contenteditable="true"></div>' +
  '<div id="autocomplete" role="listbox"></div></div>';

function createRuntime(invoke = async () => ({ prefix: '', items: [], validMatch: false, highlightRanges: [] })) {
  const app = loadApp({ html: composerMarkup, globals: { invoke } });
  const popup = app.document.getElementById('autocomplete');
  app.window.RozsaComposer = {
    renderSuggestions(items, selectedIndex, onSelect, onHover, mode) {
      popup.replaceChildren(...items.map((item, index) => {
        const option = app.document.createElement('div');
        option.setAttribute('role', 'option');
        option.setAttribute('aria-selected', String(index === selectedIndex));
        option.textContent = item.label || item.value;
        option.addEventListener('mousedown', () => onSelect(index));
        option.addEventListener('mouseover', () => onHover(index));
        return option;
      }));
      popup.classList.toggle('visible', items.length > 0);
    },
    clearSuggestions() { popup.replaceChildren(); popup.classList.remove('visible'); },
    scrollSuggestionTo() {},
  };
  app.evaluate(`
    invoke = window.invoke;
    showError = message => { (window.errors ||= []).push(String(message)); };
    activeSessionId = 'session-a';
    activeSessionIdx = 0;
  `);
  return app;
}

test('composer exposes the message box and named actions to assistive technology', () => {
  const dom = createDOM(renderToStaticMarkup(React.createElement(ComposerInput)));
  try {
    const { document } = dom.window;
    assert.ok(getByRole(document, 'textbox', { name: 'Message Rózsa' }));
    assert.ok(getByRole(document, 'toolbar', { name: 'Message actions' }));
    assert.ok(getByRole(document, 'button', { name: 'Attach file' }));
    assert.ok(getByRole(document, 'button', { name: 'Slash commands' }));
    assert.ok(getByRole(document, 'button', { name: 'Send message' }));
  } finally { dom.window.close(); }
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
    getByRole(app.document, 'option', { name: '/model' });
    app.evaluate("navigateAutocomplete('down')");
    assert.equal(getByRole(app.document, 'option', { name: '/model' }).getAttribute('aria-selected'), 'true');
    app.evaluate('confirmAutocomplete()');
    assert.equal(app.document.getElementById('msgInput').textContent, '/model ');
    assert.equal(queryByRole(app.document, 'option', { name: '/model' }), null);
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
    assert.ok(getByRole(app.document, 'option', { name: 'Reasoning Model' }));
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

test('focused Enter sends once after composition commits and remains blocked by each active IME signal', () => {
  const app = createRuntime();
  const sends = [];
  try {
    app.window.sendForTest = () => sends.push(true);
    app.evaluate('capturingKeyBindingAction = false; sendMessage = window.sendForTest');

    const input = app.document.getElementById('msgInput');
    input.focus();
    assert.equal(app.document.activeElement, input);
    initComposer({
      boundModules: new Set(),
      byId: id => app.document.getElementById(id),
      handleCompositionStart: () => app.evaluate('handleCompositionStart()'),
      handleCompositionUpdate: () => app.evaluate('handleCompositionUpdate(document.getElementById("msgInput"))'),
      handleCompositionEnd: () => app.evaluate('handleCompositionEnd(document.getElementById("msgInput"))'),
    });

    const pressEnter = options => input.dispatchEvent(new app.window.KeyboardEvent('keydown', {
      key: 'Enter', keyCode: 13, bubbles: true, cancelable: true, ...options,
    }));

    pressEnter({});
    assert.deepEqual(sends, [true]);
    sends.length = 0;

    input.dispatchEvent(new app.window.CompositionEvent('compositionstart', { bubbles: true }));
    pressEnter({});
    assert.deepEqual(sends, []);

    input.dispatchEvent(new app.window.CompositionEvent('compositionend', { bubbles: true, data: '机' }));
    pressEnter({ isComposing: true });
    assert.deepEqual(sends, []);

    pressEnter({ keyCode: 229 });
    assert.deepEqual(sends, []);

    pressEnter({});
    assert.deepEqual(sends, [true]);
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
    assert.equal(queryByRole(app.document, 'option'), null);
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
    assert.ok(getByRole(app.document, 'option', { name: 'New Model' }));
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
    assert.equal(queryByRole(app.document, 'option'), null);
    assert.equal(app.document.getElementById('autocomplete').classList.contains('visible'), false);
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
    assert.equal(queryByRole(app.document, 'option'), null);
    assert.equal(app.document.getElementById('autocomplete').classList.contains('visible'), false);
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
    assert.equal(app.document.getElementById('autocomplete').classList.contains('visible'), false);
    assert.ok(app.window.errors.includes('No models available'));
    assert.equal(input.textContent, '中文草稿保留');
    assert.deepEqual(JSON.parse(JSON.stringify(app.evaluate('getInputSelection(document.getElementById("msgInput"))'))), { start: 4, end: 4 });

    app.window.invoke = async () => { throw new Error('registry unavailable'); };
    app.evaluate('invoke = window.invoke; models = [{ id: "old-model", name: "Old Model", provider: "test" }]; acMode = "models"; acVisible = true');
    await app.evaluate('refreshModelRegistry()');
    assert.equal(app.document.getElementById('autocomplete').classList.contains('visible'), false);
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
    mode.value = 'steer';
    assert.equal(mode.value, 'steer');
    input.textContent = 'replacement';
    app.evaluate('restoreSessionDraft("session-a")');
    assert.equal(input.textContent, '😀 draft text');
    assert.equal(mode.value, 'queue');
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
    input.textContent = 'replacement';
    app.evaluate('restoreSessionDraft("session-a")');
    assert.equal(input.textContent, 'A draft here');
    assert.deepEqual(JSON.parse(JSON.stringify(app.evaluate('getInputSelection(document.getElementById("msgInput"))'))), { start: 2, end: 7 });
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
    assert.equal(input.textContent, 'new draft');
    assert.deepEqual(JSON.parse(JSON.stringify(app.evaluate('getInputSelection(document.getElementById("msgInput"))'))), { start: 4, end: 6 });
    app.evaluate("activeSessionId = 'session-a'; restoreSessionDraft('session-a')");
    assert.equal(input.textContent, 'draft 中');
    assert.deepEqual(JSON.parse(JSON.stringify(app.evaluate('getInputSelection(document.getElementById("msgInput"))'))), { start: 7, end: 7 });
  } finally { app.close(); }
});
