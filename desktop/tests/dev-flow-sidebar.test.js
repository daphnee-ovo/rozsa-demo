import assert from 'node:assert/strict';
import { Script } from 'node:vm';
import { getAllByRole, getByRole } from '@testing-library/dom';
import { test } from 'vitest';
import { claimedMarkup } from '../renderer/scripts/features/dev-flow.js';
import { sessionsMarkup } from '../renderer/scripts/features/sidebar.js';
import { readFrontend, createDOM } from './helpers/browser.js';

test('Dev Flow sidebar keeps visible rows responsive and delays hover detail until intent is clear', async () => {
  const dom = createDOM('<main id="mainSidebarScene"><section data-od-id="sidebar-status"><div id="devFlowSidebarGroup" hidden><button id="devFlowSidebarSummary"></button><div id="devFlowSidebarClaimed"></div><button id="devFlowSidebarMore" hidden></button></div></section><ul id="sessionList"></ul></main><footer class="sidebar-bottom"></footer><button id="sidebarDevFlowDashboard">Dashboard</button>');
  const { window } = dom;
  const handlers = new Map();
  const calls = [];
  const events = [];
  let fontsLoaded;
  let rowHeight = 20;
  let initialize;
  window.RozsaSidebar = { claimedMarkup, sessionsMarkup };
  window.RozsaUIReady = Promise.resolve();
  window.RozsaUIHost = { port: {
    async request(name, args) {
      if (name === 'gui_webview_ready') return { revision: 0, scene: 'main', selectedPane: null };
      calls.push({ name, args });
    },
    async subscribe(name, handler) { handlers.set(name, handler); return () => {}; },
  } };
  window.RozsaGuiShared = { applySceneSnapshot(state, snapshot, render) { Object.assign(state, snapshot); render(snapshot); } };
  window.RozsaSidebarEmit = async name => events.push(name);
  window.innerHeight = 300;
  window.ResizeObserver = class { constructor(callback) { this.callback = callback; } observe() {} };
  window.requestAnimationFrame = callback => callback();
  Object.defineProperty(window.document, 'fonts', { value: { addEventListener: (_name, callback) => { fontsLoaded = callback; } } });
  const timers = new Map();
  let timerId = 0;
  window.setTimeout = (callback, delay) => { const id = ++timerId; timers.set(id, { callback, delay }); return id; };
  window.clearTimeout = id => timers.delete(id);
  Object.defineProperty(window.HTMLElement.prototype, 'getBoundingClientRect', { configurable: true, value() {
    if (this.matches('[data-od-id="sidebar-status"]')) return { height: 100 + window.document.getElementById('devFlowSidebarClaimed').children.length * rowHeight + (window.document.getElementById('devFlowSidebarMore').hidden ? 0 : rowHeight) };
    if (this.id === 'devFlowSidebarClaimed' || this.id === 'devFlowSidebarMore' || this.classList.contains('dev-flow-claimed-row')) return { height: this.id === 'devFlowSidebarClaimed' ? this.children.length * rowHeight : rowHeight };
    if (this.matches('.sidebar-bottom')) return { height: 40 };
    return { height: 0 };
  } });
  const addWindowListener = window.addEventListener.bind(window);
  window.addEventListener = (name, callback, options) => {
    if (name === 'DOMContentLoaded') initialize = callback;
    else addWindowListener(name, callback, options);
  };
  new Script(readFrontend('sidebar.js'), { filename: 'sidebar.js' }).runInContext(dom.getInternalVMContext());
  try {
    await initialize();
    const items = count => Array.from({ length: count }, (_, index) => ({ id: `TASK-T${String(index + 1).padStart(3, '0')}`, shortId: `T${String(index + 1).padStart(3, '0')}`, title: `A very long claimed task title ${index}` }));
    const publish = devFlow => handlers.get('sidebar-state')({
      devFlow, showDevFlowDashboard: true, sessions: [], activeSessionId: null,
    });
    publish({ availability: 'ready', showSidebarStatus: true, claimed: items(5), project: { projectKey: 'test-project' }, revision: 7, openTasks: 5, openIssues: 0 });

    const claimed = window.document.getElementById('devFlowSidebarClaimed');
    const more = getByRole(window.document, 'button', { name: 'more 4' });
    assert.equal(getAllByRole(claimed, 'button').length, 1);
    assert.equal(more.textContent, 'more 4');
    const row = getByRole(claimed, 'button', { name: /T001A very long claimed task title 0/ });
    row.dispatchEvent(new window.Event('pointerenter'));
    const runDelayed = milliseconds => {
      for (const [id, timer] of [...timers]) if (timer.delay <= milliseconds) { timers.delete(id); timer.callback(); }
    };
    runDelayed(179);
    assert.equal(calls.length, 0);
    runDelayed(180);
    assert.equal(calls.length, 1);
    assert.equal(calls[0].name, 'dev_flow_detail');
    assert.equal(calls[0].args.request.target.id, 'TASK-T001');
    row.dispatchEvent(new window.Event('pointerleave'));
    assert.deepEqual(events, ['dev-flow-detail-dismiss']);
    row.click();
    assert.equal(calls.length, 2);
    row.dispatchEvent(new window.Event('pointerleave'));
    assert.deepEqual(events, ['dev-flow-detail-dismiss']);

    window.innerHeight = 360;
    publish({ availability: 'ready', showSidebarStatus: true, claimed: items(5), project: { projectKey: 'test-project' }, revision: 8, openTasks: 5, openIssues: 0 });
    assert.equal(getAllByRole(claimed, 'button').length, 5);
    assert.equal(window.document.getElementById('devFlowSidebarMore').hidden, true);
    publish({ availability: 'ready', showSidebarStatus: true, claimed: [], project: { projectKey: 'test-project' }, revision: 9, openTasks: 0, openIssues: 0 });
    assert.equal(claimed.textContent, '');

    rowHeight = 30;
    fontsLoaded();
    publish({ availability: 'ready', showSidebarStatus: true, claimed: items(5), project: { projectKey: 'test-project' }, revision: 10, openTasks: 5, openIssues: 0 });
    assert.equal(getAllByRole(claimed, 'button').length, 2);
    assert.equal(window.document.getElementById('devFlowSidebarMore').textContent, 'more 3');

    const dashboard = getByRole(window.document, 'button', { name: 'Dashboard' });
    for (const [devFlow, title] of [[null, 'Dev-flow is disabled'], [{ availability: 'starting', availabilityMessage: 'starting state' }, 'starting state'], [{ availability: 'loading', availabilityMessage: 'loading state' }, 'loading state'], [{ availability: 'error', availabilityMessage: 'error state' }, 'error state']]) {
      publish(devFlow);
      assert.equal(dashboard.disabled, true);
      assert.equal(dashboard.title, title);
    }
    publish({ availability: 'ready', showSidebarStatus: false, claimed: [], project: { projectKey: 'test-project' }, revision: 11, openTasks: 0, openIssues: 0 });
    assert.equal(dashboard.disabled, false);
    assert.equal(dashboard.title, 'Open dev-flow dashboard');
  } finally { dom.window.close(); }
});
