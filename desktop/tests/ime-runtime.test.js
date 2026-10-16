import test from 'node:test';
import assert from 'node:assert/strict';
import { loadApp } from './helpers/browser.js';

const composer = '<div class="input-wrapper"><div id="msgInput" contenteditable="true">/task</div>' +
  '<div id="autocomplete" class="visible"><div class="ac-item">stale</div></div>' +
  '<button data-od-id="send-btn"></button></div>';

test('stale autocomplete cannot replace the DOM during composition', async () => {
  let resolveAutocomplete;
  const app = loadApp({ html: composer, globals: { invoke: () => new Promise(resolve => { resolveAutocomplete = resolve; }) } });
  try {
    app.evaluate('invoke = window.invoke; conversationOperationSuspended = () => false');
    app.evaluate('window.highlightCalls = 0; updateInputHighlight = () => { window.highlightCalls++; };');
    const pending = app.evaluate('updateAutocomplete()');
    app.evaluate("handleCompositionStart(document.getElementById('msgInput'))");
    resolveAutocomplete({ items: [{ label: 'replacement', value: '/replacement' }], prefix: '/', highlightRanges: [{ start: 0, end: 5 }] });
    await pending;
    assert.equal(app.document.querySelector('#autocomplete').textContent, '');
    assert.equal(app.document.querySelector('#autocomplete').classList.contains('visible'), false);
    assert.equal(app.window.highlightCalls, 0);
  } finally { app.close(); }
});

test('composition start invalidates pending autocomplete', async () => {
  let resolveAutocomplete;
  const app = loadApp({ html: composer, globals: { invoke: () => new Promise(resolve => { resolveAutocomplete = resolve; }) } });
  try {
    app.evaluate('invoke = window.invoke; conversationOperationSuspended = () => false');
    const pending = app.evaluate('updateAutocomplete()');
    app.evaluate("handleCompositionStart(document.getElementById('msgInput'))");
    resolveAutocomplete({ items: [{ label: 'stale', value: '/stale' }], prefix: '/', highlightRanges: [] });
    await pending;
    assert.equal(app.document.querySelector('#autocomplete').classList.contains('visible'), false);
    assert.equal(app.document.querySelector('#autocomplete').children.length, 0);
  } finally { app.close(); }
});

test('hiding autocomplete removes visible suggestions', () => {
  const app = loadApp({ html: composer });
  try {
    app.evaluate('acVisible = true; acSelectedIndex = 0; acPrefix = "/"; acItems = [1]; hideAutocomplete(false)');
    const popup = app.document.getElementById('autocomplete');
    assert.equal(popup.children.length, 0, 'shared popup displays nonempty suggestions');
    assert.equal(popup.classList.contains('visible'), false);
    assert.equal(app.evaluate('acVisible'), false);
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
