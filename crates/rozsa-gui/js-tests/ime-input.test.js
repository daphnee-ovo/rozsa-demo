import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { loadApp } from './helpers/browser.js';
import { frontendRoot } from './helpers/browser.js';

test('shared composer dispatches input lifecycle and submit once', async () => {
  const input = { listeners: {}, addEventListener(name, fn) { this.listeners[name] = fn; } };
  const composer = { ...input, listeners: {} };
  const calls = [];
  const api = { boundModules: new Set(), byId: id => id === 'msgInput' ? input : id === 'composer' ? composer : null };
  for (const name of ['handleInput', 'handleCompositionStart', 'handleCompositionUpdate', 'handleCompositionEnd', 'addMessage']) api[name] = (...args) => calls.push([name, ...args]);
  const { init, renderSendState } = await import(new URL('../frontend/ui/scripts/components/composer.js', import.meta.url));
  init(api); init(api);
  const event = { isComposing: true };
  input.listeners.compositionstart(event); input.listeners.compositionupdate(event); input.listeners.input(event); input.listeners.compositionend(event);
  assert.deepEqual(calls.map(x => x[0]), ['handleCompositionStart', 'handleCompositionUpdate', 'handleInput', 'handleCompositionEnd']);
  assert.equal(calls[2][1], input); assert.equal(calls[2][2], event);
  composer.listeners.click({ target: { closest() { return { dataset: { action: 'send' } }; } } });
  assert.equal(calls.filter(x => x[0] === 'addMessage').length, 1);
  const button = { dataset: {}, setAttribute(name, value) { this[name] = value; } };
  for (const action of ['send', 'stop', 'queue', 'steer']) { renderSendState(button, action); assert.equal(button.dataset.sendAction, action); assert.ok(button['aria-label']); assert.ok(button.innerHTML.includes('<svg')); }
  assert.ok(readFileSync(new URL('../frontend/ui/templates/composites/composer.html', import.meta.url), 'utf8').includes('id="composer"'));
});

test('IME guards suppress input refresh and keyboard shortcuts during composition', () => {
  const source = readFileSync(new URL('../frontend/app.js', import.meta.url), 'utf8');
  const handleInputStart = source.indexOf('function handleInput(input, event)');
  const handleInputEnd = source.indexOf('function handleCompositionStart', handleInputStart);
  const handleInput = source.slice(handleInputStart, handleInputEnd);
  assert.match(handleInput, /if \(isInputComposing \|\| event\?\.isComposing \|\| event\?\.inputType === 'insertCompositionText'\)/);
  assert.match(handleInput, /updateAutocomplete\(\)/);
  const keydownStart = source.indexOf("document.addEventListener('keydown'");
  assert.notEqual(keydownStart, -1);
  assert.ok(source.slice(keydownStart).includes('if (isInputComposing || e.isComposing || e.keyCode === 229) return;'));
});

test('composition end refreshes autocomplete after the browser commits text', async () => {
  const app = loadApp({ html: '<div id="msgInput" contenteditable="true">日</div>' });
  try {
    app.evaluate('window.refreshes = 0; updateAutocomplete = () => { window.refreshes++; }; updateAbortButton = () => {}; flushPendingGuiSceneTransition = () => {}; isInputComposing = true; handleCompositionEnd(document.getElementById("msgInput"))');
    assert.equal(app.evaluate('isInputComposing'), false);
    await new Promise(resolve => setTimeout(resolve, 5));
    assert.equal(app.window.refreshes, 1);
  } finally { app.close(); }
});
