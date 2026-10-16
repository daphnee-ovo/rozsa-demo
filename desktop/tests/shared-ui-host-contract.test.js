import test from 'node:test';
import assert from 'node:assert/strict';
import { createUIHost } from '../renderer/scripts/core/renderer.js';
import { createTauriAdapter } from '../renderer/runtime/adapters/tauri.js';
import { init as bindPopover } from '../renderer/scripts/components/popover.js';
import { createDOM, loadApp, loadClassic, readFrontend, readPrototype, withDOM } from './helpers/browser.js';

test('ports preserve errors, cancellation, revisions and subscription disposal', async () => {
  let receive;
  let removed = 0;
  const calls = [];
  const failure = new Error('product rejected');
  const host = createUIHost(createTauriAdapter(() => ({
    core: { invoke: async (command, data) => {
      calls.push([command, data]);
      if (command === 'fail') throw failure;
      if (command === 'pending') return new Promise(() => {});
      return data;
    } },
    event: { listen: async (_event, callback) => {
      receive = callback;
      return () => { removed++; };
    } },
  })));
  assert.deepEqual(await host.port.request('get_state', { value: 7 }), { value: 7 });
  await assert.rejects(host.port.request('fail'), error => error === failure);
  const delivered = [];
  const unsubscribe = await host.port.subscribe('ui-state', payload => delivered.push(payload));
  receive({ payload: { revision: 1 } });
  unsubscribe(); unsubscribe();
  receive({ payload: { revision: 2 } });
  assert.deepEqual(delivered, [{ revision: 1 }]);
  assert.equal(removed, 1);
  const cancel = new AbortController();
  const pending = host.port.request('pending', null, { signal: cancel.signal });
  await Promise.resolve();
  cancel.abort();
  await assert.rejects(pending, { name: 'AbortError' });
  assert(!calls.some(([command]) => command === 'abort'));
  const preCancelled = new AbortController(); preCancelled.abort();
  const count = calls.length;
  await assert.rejects(host.port.request('never', null, { signal: preCancelled.signal }), { name: 'AbortError' });
  assert.equal(calls.length, count);
  const state = { revision: 1 };
  const valid = snapshot => snapshot.scene === 'main';
  assert.equal(host.applyRevision(state, { revision: 1, scene: 'main' }, valid, () => assert.fail()), false);
  assert.equal(host.applyRevision(state, { revision: 2, scene: 'unknown' }, valid, () => assert.fail()), false);
  assert.throws(() => host.applyRevision(state, { revision: 2, scene: 'main' }, valid, () => { throw failure; }));
  assert.equal(state.revision, 1);
  assert.equal(host.applyRevision(state, { revision: 2, scene: 'main' }, valid, () => {}), true);
  assert.equal(state.revision, 2);
  await host.port.subscribe('theme-state', () => {});
  const disposal = host.port.request('pending');
  host.port.dispose(); host.port.dispose();
  await assert.rejects(disposal, { name: 'AbortError' });
  await assert.rejects(host.port.request('never'), { name: 'AbortError' });
  assert.equal(removed, 2);
  let resolveRegistration;
  let lateRemoved = 0;
  const late = createUIHost({ request() {}, subscribe(_event, callback) {
    callback('before');
    return new Promise(resolve => { resolveRegistration = resolve; });
  } });
  const registration = late.port.subscribe('late', () => {});
  late.port.dispose();
  resolveRegistration(() => { lateRemoved++; });
  await assert.rejects(registration, { name: 'AbortError' });
  assert.equal(lateRemoved, 1);
});

test('both hosts reference shared UI and production stays isolated from mock adapters', () => {
  for (const webview of ['main', 'sidebar']) {
    const entry = readFrontend(`runtime/entries/${webview}.js`);
    assert(entry.includes('../../scripts/core/renderer.js'));
    assert(entry.includes('createTauriAdapter'));
    assert(readFrontend(`${webview === 'main' ? 'tauri' : 'sidebar'}.html`).includes(`type="module" src="runtime/entries/${webview}.js"`));
  }
  const shell = readPrototype('scripts/core/shell.js');
  assert(shell.includes('desktop/renderer/scripts/core/renderer.js'));
  assert(shell.includes('createMockAdapter'));
  for (const path of ['ports', 'renderer']) {
    const source = readFrontend(`scripts/core/${path}.js`);
    assert(!source.includes('__TAURI__'));
    assert(!source.includes('prototype/mock'));
  }
  const adapter = readFrontend('runtime/adapters/tauri.js');
  assert(!adapter.includes('document.'));
  assert(!adapter.includes('mock'));
});

test('popover positioning accounts for host zoom in CSS coordinates', async () => {
  const dom = createDOM('<button id="thinkingEffort"></button><div id="thinkingEffortPopover"><input id="thinkingEffortSlider"></div>');
  try {
    await withDOM(dom, () => {
      window.innerWidth = 896; window.innerHeight = 720;
      const scale = 16 / 14;
      const popover = document.getElementById('thinkingEffortPopover');
      Object.defineProperty(popover, 'offsetWidth', { value: 320 });
      popover.getBoundingClientRect = () => ({ width: 320 * scale, height: 106 * scale });
      document.getElementById('thinkingEffort').getBoundingClientRect = () => ({ left: 790, bottom: 694 });
      bindPopover({ boundModules: new Set(), byId: id => document.getElementById(id) }).position();
      assert(parseFloat(popover.style.left) * scale + 320 * scale <= window.innerWidth);
      assert(parseFloat(popover.style.top) * scale + 106 * scale <= window.innerHeight);
    });
  } finally { dom.window.close(); }
});

// Classic scripts and module entries share browser globals. Preserve that contract
// while importing the complete production entry and its real template dependencies.
async function withCallbacks(harness, names, action) {
  const previous = new Map(names.map(name => [name, Object.getOwnPropertyDescriptor(globalThis, name)]));
  for (const name of names) globalThis[name] = harness.evaluate(name);
  try { await withDOM(harness.dom, action); }
  finally {
    for (const [name, descriptor] of previous) {
      if (descriptor) Object.defineProperty(globalThis, name, descriptor);
      else delete globalThis[name];
    }
    harness.close();
  }
}

async function checkDelayedEntry(entry, mounted) {
  window.__TAURI__ = { window: { getCurrentWindow: () => ({
    outerSize: async () => ({ width: 1280 }), scaleFactor: async () => 1,
  }) } };
  const localFetch = globalThis.fetch;
  let release;
  let started;
  const pending = new Promise(resolve => { release = resolve; });
  const loadingStarted = new Promise(resolve => { started = resolve; });
  let first = true;
  globalThis.fetch = async input => {
    if (first) { first = false; started(); await pending; }
    return localFetch(input);
  };
  const loading = import(entry);
  await Promise.race([loadingStarted, loading.then(() => { throw new Error('entry did not load its template'); })]);
  assert.equal(typeof window.RozsaUIReady?.then, 'function');
  assert.equal(mounted(), false);
  release();
  await loading;
  await window.RozsaUIReady;
  assert.equal(mounted(), true);
  window.RozsaUIHost.port.dispose();
}

test('main entry publishes readiness before loading actual shared templates', async () => {
  const harness = loadApp({ html: readFrontend('tauri.html') });
  harness.evaluate(readFrontend('gui_shared.js'));
  harness.window.testStops = 0;
  harness.evaluate('isStreaming=true;abortAgent=async()=>{testStops++;};');
  await withCallbacks(harness, [
    'previewThinkingEffort', 'selectThinkingEffort', 'toggleToolCall', 'toggleTurnDiff', 'closeDevFlowDetail',
    'switchSettingsTab', 'closeSettings', 'selectThemeModeCard', 'previewFontSize', 'commitFontSize',
    'configureInterruptionSuspensions', 'respondPermission', 'enterPermissionTrust', 'enterPermissionHint',
    'togglePermissionCommand', 'showPermissionMainPage', 'submitPermissionHint', 'submitUserQuestion',
    'normalizePermissionHint', 'handlePermissionHintKeydown', 'configureConversationSuspensions',
    'attachFileReference', 'attachDirectoryReference', 'insertSlashCommandPrefix', 'showModelPicker',
    'updateAbortButton', 'handleInput', 'handleCompositionStart', 'handleCompositionUpdate',
    'handleCompositionEnd', 'toggleThinking', 'isStreaming', 'abortAgent', 'sendMessage',
  ], async () => {
    await checkDelayedEntry('../renderer/runtime/entries/main.js', () => Boolean(window.RozsaPopover));
    assert(document.getElementById('thinkingEffortPopover'));
    assert(document.getElementById('fallbackSidebarTemplate').content.querySelector('#sessionList'));
    const input = document.getElementById('msgInput');
    input.textContent = '未发送 draft';
    document.querySelector('[data-action="send"]').click();
    await Promise.resolve();
    assert.equal(harness.window.testStops, 1, 'actual runtime composer dispatches Stop while streaming');
    assert.equal(input.textContent, '未发送 draft');
  });
});

test('sidebar entry publishes readiness before mounting actual settings navigation', async () => {
  const harness = loadClassic('sidebar.js', { html: readFrontend('sidebar.html'),
    globals: { ResizeObserver: class { observe() {} disconnect() {} } } });
  harness.evaluate(readFrontend('gui_shared.js'));
  await withCallbacks(harness, ['selectSidebarSettingsPane', 'closeSidebarSettings', 'newSidebarSession',
    'openSidebarSettings', 'openDevFlowDashboard', 'switchSidebarSession'], async () => {
    await checkDelayedEntry('../renderer/runtime/entries/sidebar.js', () => Boolean(document.querySelector('#settingsSidebarScene [data-settings-pane]')));
    assert(window.RozsaSidebar);
  });
});
