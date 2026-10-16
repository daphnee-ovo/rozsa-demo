import test from 'node:test';
import assert from 'node:assert/strict';
import { JSDOM } from 'jsdom';
import { bindViewport } from '../renderer/scripts/core/viewport.ts';
import { readWindowWidth } from '../renderer/runtime/adapters/viewport.js';

test('split panes use the complete logical window for dimensions and breakpoints', async () => {
  const dom = new JSDOM('<html><body></body></html>');
  const api = { window: { getCurrentWindow: () => ({
    innerSize: async () => { throw new Error('detached content view is not the window'); },
    outerSize: async () => ({ width: 2560 }), scaleFactor: async () => 2,
  }) } };
  const stop = await bindViewport(dom.window, () => readWindowWidth(api));
  const root = dom.window.document.documentElement;
  assert.equal(root.style.getPropertyValue('--ui-vw'), '12.8px');
  assert.equal(root.getAttribute('data-ui-max-width'), '');
  // A 256px Sidebar and 1024px main viewport must not select compact rules.
  assert.equal(root.matches('[data-ui-max-width~="560"]'), false);
  assert.equal(root.matches('[data-ui-max-width~="1100"]'), false);
  stop(); dom.window.close();
});

test('browser resize preserves original viewport boundaries and removes listeners on disposal', async () => {
  const dom = new JSDOM('<html><body></body></html>');
  let width = 900;
  const stop = await bindViewport(dom.window, () => width);
  const root = dom.window.document.documentElement;
  assert.equal(root.style.getPropertyValue('--ui-vw'), '9px');
  assert.equal(root.getAttribute('data-ui-max-width'), '900 1100');
  width = 560;
  dom.window.dispatchEvent(new dom.window.Event('resize'));
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(root.getAttribute('data-ui-max-width'), '560 760 780 900 1100');
  stop(); width = 1440;
  dom.window.dispatchEvent(new dom.window.Event('resize'));
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(root.style.getPropertyValue('--ui-vw'), '5.6px');
  dom.window.close();
});

test('invalid native geometry fails instead of substituting the pane viewport', async () => {
  for (const scale of [0, NaN, -1]) {
    await assert.rejects(readWindowWidth({ window: { getCurrentWindow: () => ({
      outerSize: async () => ({ width: 2560 }), scaleFactor: async () => scale,
    }) } }), /Invalid native window geometry/);
  }
});
