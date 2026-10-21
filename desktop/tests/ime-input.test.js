import { test } from 'vitest';
import assert from 'node:assert/strict';
import { getByRole } from '@testing-library/dom';
import { init, renderSendState } from '../renderer/scripts/components/composer.tsx';
import { createDOM, loadApp } from './helpers/browser.js';

test('composer commits composition text and submits once after duplicate initialization', () => {
  const dom = createDOM('<div id="composer"><div id="msgInput" role="textbox" aria-label="Message Rózsa" contenteditable="true"></div><button data-action="send" aria-label="Send message"></button></div><ul aria-label="Sent messages"></ul><p role="status" aria-label="Draft status"></p>');
  try {
    const { document } = dom.window;
    const input = getByRole(document, 'textbox', { name: 'Message Rózsa' });
    const messages = document.querySelector('ul[aria-label="Sent messages"]');
    const status = document.querySelector('p[role="status"]');
    const api = {
      boundModules: new Set(),
      byId: id => document.getElementById(id),
      handleInput: target => { status.textContent = target.textContent; },
      handleCompositionStart: () => { status.textContent = 'Composing'; },
      handleCompositionUpdate: () => {},
      handleCompositionEnd: target => { status.textContent = target.textContent; },
      addMessage: () => { const message = document.createElement('li'); message.textContent = input.textContent; messages.append(message); },
    };
    init(api);
    init(api);
    input.dispatchEvent(new dom.window.CompositionEvent('compositionstart', { bubbles: true }));
    input.textContent = '机';
    input.dispatchEvent(new dom.window.CompositionEvent('compositionend', { bubbles: true, data: '机' }));
    assert.equal(getByRole(document, 'status', { name: 'Draft status' }).textContent, '机');
    getByRole(document, 'button', { name: 'Send message' }).click();
    assert.equal(getByRole(document, 'list', { name: 'Sent messages' }).textContent, '机');
    assert.equal(messages.children.length, 1);

    const sendButton = getByRole(document, 'button', { name: 'Send message' });
    renderSendState(sendButton, 'queue');
    assert.ok(getByRole(document, 'button', { name: 'Queue message' }));
    renderSendState(sendButton, 'steer');
    assert.ok(getByRole(document, 'button', { name: 'Steer' }));
  } finally { dom.window.close(); }
});

test('composition end refreshes autocomplete after the browser commits text', async () => {
  const app = loadApp({
    html: '<div id="msgInput" role="textbox" aria-label="Message Rózsa" contenteditable="true">日</div><div id="autocomplete" role="listbox"></div>',
    globals: { invoke: async () => ({ prefix: '机', items: [{ value: '机器', label: '机器' }], highlightRanges: [] }) },
  });
  try {
    const input = getByRole(app.document, 'textbox', { name: 'Message Rózsa' });
    const popup = getByRole(app.document, 'listbox');
    app.window.RozsaComposer = {
      renderSuggestions(items) {
        popup.replaceChildren(...items.map(item => {
          const option = app.document.createElement('div');
          option.setAttribute('role', 'option');
          option.textContent = item.label;
          return option;
        }));
      },
      clearSuggestions() { popup.replaceChildren(); },
      scrollSuggestionTo() {},
    };
    app.evaluate('invoke = window.invoke; updateAbortButton = () => {}; flushPendingGuiSceneTransition = () => {};');
    init({
      boundModules: new Set(), byId: id => app.document.getElementById(id),
      handleInput: () => {},
      handleCompositionStart: () => app.evaluate('handleCompositionStart()'),
      handleCompositionUpdate: () => app.evaluate('handleCompositionUpdate(document.getElementById("msgInput"))'),
      handleCompositionEnd: () => app.evaluate('handleCompositionEnd(document.getElementById("msgInput"))'),
    });
    input.focus();
    input.dispatchEvent(new app.window.CompositionEvent('compositionstart', { bubbles: true }));
    input.textContent = '机';
    input.dispatchEvent(new app.window.CompositionEvent('compositionend', { bubbles: true, data: '机' }));
    await new Promise(resolve => setTimeout(resolve, 10));
    assert.equal(input.textContent, '机');
    assert.ok(getByRole(app.document, 'option', { name: '机器' }));
  } finally { app.close(); }
});
