import { test } from 'vitest';
import assert from 'node:assert/strict';
import { queryByRole } from '@testing-library/dom';
import { loadApp } from './helpers/browser.js';

const composer = '<div class="input-wrapper"><div id="msgInput" contenteditable="true">/task</div>' +
  '<div id="autocomplete" class="visible"><div class="ac-item">stale</div></div>' +
  '<button data-od-id="send-btn"></button></div>';

function installSuggestionPopupStub(app) {
  const popup = app.document.getElementById('autocomplete');
  app.window.RozsaComposer = {
    renderSuggestions(items) {
      popup.replaceChildren(...items.map(item => {
        const option = app.document.createElement('div');
        option.className = 'ac-item';
        option.setAttribute('role', 'option');
        option.textContent = item.label || item.value;
        return option;
      }));
      popup.classList.toggle('visible', items.length > 0);
    },
    clearSuggestions() {
      popup.replaceChildren();
      popup.classList.remove('visible');
    },
    scrollSuggestionTo() {},
  };
}

test('stale autocomplete cannot replace the DOM during composition', async () => {
  let resolveAutocomplete;
  const app = loadApp({ html: composer, globals: { invoke: () => new Promise(resolve => { resolveAutocomplete = resolve; }) } });
  try {
    installSuggestionPopupStub(app);
    app.evaluate('invoke = window.invoke; conversationOperationSuspended = () => false');
    const input = app.document.getElementById('msgInput');
    input.textContent = '未提交文字';
    app.evaluate('updateInputHighlight = () => { document.getElementById("msgInput").replaceChildren(document.createElement("strong")); };');
    const pending = app.evaluate('updateAutocomplete()');
    app.evaluate("handleCompositionStart(document.getElementById('msgInput'))");
    resolveAutocomplete({ items: [{ label: 'replacement', value: '/replacement' }], prefix: '/', highlightRanges: [{ start: 0, end: 5 }] });
    await pending;
    assert.equal(queryByRole(app.document, 'option'), null);
    assert.equal(app.document.querySelector('#autocomplete').classList.contains('visible'), false);
    assert.equal(input.textContent, '未提交文字');
  } finally { app.close(); }
});

test('composition start invalidates pending autocomplete', async () => {
  let resolveAutocomplete;
  const app = loadApp({ html: composer, globals: { invoke: () => new Promise(resolve => { resolveAutocomplete = resolve; }) } });
  try {
    installSuggestionPopupStub(app);
    app.evaluate('invoke = window.invoke; conversationOperationSuspended = () => false');
    const pending = app.evaluate('updateAutocomplete()');
    app.evaluate("handleCompositionStart(document.getElementById('msgInput'))");
    resolveAutocomplete({ items: [{ label: 'stale', value: '/stale' }], prefix: '/', highlightRanges: [] });
    await pending;
    assert.equal(app.document.querySelector('#autocomplete').classList.contains('visible'), false);
    assert.equal(queryByRole(app.document, 'option'), null);
  } finally { app.close(); }
});

test('hiding autocomplete removes visible suggestions', () => {
  const app = loadApp({ html: composer });
  try {
    installSuggestionPopupStub(app);
    app.evaluate('acVisible = true; acSelectedIndex = 0; acPrefix = "/"; acItems = [1]; hideAutocomplete(false)');
    const popup = app.document.getElementById('autocomplete');
    assert.equal(queryByRole(app.document, 'option'), null);
    assert.equal(popup.classList.contains('visible'), false);
  } finally { app.close(); }
});

test('input highlight refresh leaves composing input DOM intact', () => {
  const app = loadApp({ html: composer });
  try {
    const input = app.document.getElementById('msgInput');
    input.textContent = '/task';
    app.evaluate('isInputComposing = true; updateInputHighlight([{start: 0, end: 5}])');
    assert.equal(input.textContent, '/task');
    assert.equal(input.children.length, 0);
  } finally { app.close(); }
});
