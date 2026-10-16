import test from 'node:test';
import assert from 'node:assert/strict';
import { createUIHost } from '../renderer/scripts/core/renderer.ts';
import { resolveTemplateRoot } from '../renderer/scripts/core/shell.ts';
import { clearTemplateCache, loadTemplate } from '../renderer/scripts/core/template-loader.ts';
import { init as bindPopover } from '../renderer/scripts/components/popover.js';
import { createDOM, readFrontend, readPrototype, withDOM } from './helpers/browser.js';

test('ports preserve errors, cancellation, revisions and subscription disposal', async () => {
  let receive;
  let removed = 0;
  const calls = [];
  const failure = new Error('product rejected');
  const host = createUIHost({
    request: async (command, data) => {
      calls.push([command, data]);
      if (command === 'fail') throw failure;
      if (command === 'pending') return new Promise(() => {});
      return data;
    },
    subscribe: async (_event, callback) => {
      receive = callback;
      return () => { removed++; };
    },
  });
  assert.deepEqual(await host.port.request('get_state', { value: 7 }), { value: 7 });
  await assert.rejects(host.port.request('fail'), error => error === failure);
  const delivered = [];
  const unsubscribe = await host.port.subscribe('ui-state', payload => delivered.push(payload));
  receive({ revision: 1 });
  unsubscribe(); unsubscribe();
  receive({ revision: 2 });
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

test('the actual Electron entry and HTTP prototype use the TypeScript source boundary', () => {
  const electronHtml = readFrontend('index.html');
  assert(electronHtml.includes('type="module" src="./entry.ts"'));
  const electronEntry = readFrontend('entry.ts');
  assert(electronEntry.includes('./product-client.js'));
  assert(!electronEntry.includes('__TAURI__'));
  const prototypeHtml = readPrototype('index.html');
  assert(prototypeHtml.includes('./scenarios/registry.ts'));
  assert(prototypeHtml.includes('./scripts/core/shell.ts'));
  assert(prototypeHtml.includes('../desktop/renderer/styles/legacy/main.css'));
  const shell = readPrototype('scripts/core/shell.ts');
  assert(shell.includes('../../../desktop/renderer/scripts/core/renderer.ts'));
  assert(shell.includes('createMockAdapter'));
  for (const path of ['ports', 'renderer']) {
    const source = readFrontend(`scripts/core/${path}.ts`);
    assert(!source.includes('__TAURI__'));
    assert(!source.includes('prototype/mock'));
  }
  assert(!electronEntry.includes('prototype/mock'));
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

test('page-scoped HTTP template fixture resolves to the shared renderer source', async () => {
  clearTemplateCache();
  const dom = createDOM('<!doctype html><html><head><meta name="rozsa-template-root" content="./templates/"></head><body></body></html>');
  try {
    await withDOM(dom, async () => {
      const root = resolveTemplateRoot();
      assert.equal(root.href, 'http://127.0.0.1/templates/');
      const fragment = await loadTemplate(new URL('features/sidebar.html', root));
      assert.ok(fragment.querySelector('#sessionList'));
    });
  } finally {
    clearTemplateCache();
    dom.window.close();
  }
});
