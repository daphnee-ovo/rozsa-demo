import test from 'node:test';
import assert from 'node:assert/strict';
import { init as bindSidebar } from '../renderer/scripts/features/sidebar.js';
import { init as bindDevFlow } from '../renderer/scripts/features/dev-flow.js';
import { createDOM, withDOM, loadApp, readFrontend } from './helpers/browser.js';

test('uncovered detail states retain empty inert slots and a working Close exit', () => {
  const dom = createDOM(readFrontend('templates/features/dev-flow.html'));
  try {
    for (const name of ['dev-flow-empty', 'dev-flow-done-when']) {
      const slot = dom.window.document.querySelector(`[data-ui-slot="${name}"]`);
      assert.equal(slot.textContent, '');
      assert.equal(slot.hidden, true);
      assert(slot.hasAttribute('inert'));
    }
    assert(dom.window.document.getElementById('devFlowDetailClose'));
  } finally { dom.window.close(); }
});

test('Dashboard click dispatches once when Sidebar and Dev Flow share the page', async () => {
  const dom = createDOM('<aside id="sidebar"><button id="sidebarDevFlowDashboard" data-action="dev-flow-dashboard">Dashboard</button></aside>');
  try {
    await withDOM(dom, () => {
      let opened = 0;
      const api = { boundModules: new Set(), byId: id => document.getElementById(id),
        openDevFlowDashboard: () => { opened++; }, closeDevFlowDetail() {} };
      bindSidebar(api); bindDevFlow(api);
      document.getElementById('sidebarDevFlowDashboard').click();
      assert.equal(opened, 1);
    });
  } finally { dom.window.close(); }
});

test('native detail opens with the prototype Close focus instead of selecting the first task', () => {
  const app = loadApp({ html: '<div id="devFlowDetail" hidden><button id="devFlowDetailClose">Close</button><div class="dev-flow-detail-item" tabindex="0">T001</div></div>' });
  try {
    app.evaluate('openDevFlowDetailPanel()');
    assert.equal(app.document.getElementById('devFlowDetail').hidden, false);
    assert.equal(app.document.activeElement.id, 'devFlowDetailClose');
  } finally { app.close(); }
});

test('native main detail preserves the prototype gap without a second Sidebar offset', () => {
  const dom = createDOM('<style>' + readFrontend('styles/features/dev-flow-detail.css') + readFrontend('styles/features/dev-flow-runtime.css') + readFrontend('styles/legacy/native-host.css') + '</style><body class="native-split-main dev-flow-detail-open"><div class="dev-flow-detail"></div></body>');
  try {
    const style = dom.window.getComputedStyle(dom.window.document.querySelector('.dev-flow-detail'));
    assert.equal(style.left, '12px');
    assert.equal(style.getPropertyValue('--main-sidebar-width'), '0px');
  } finally { dom.window.close(); }
});
