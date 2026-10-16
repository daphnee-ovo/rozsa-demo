import test from 'node:test';
import assert from 'node:assert/strict';
import { Script } from 'node:vm';
import { frontendRoot, readFrontend } from './helpers/browser.js';
import { createDOM } from './helpers/browser.js';

test('sidebar adapts Dev Flow rows, hover preview, and Dashboard state to available space', async () => {
  const { claimedMarkup } = await import(new URL('ui/scripts/features/dev-flow.js', frontendRoot));
  const dom = createDOM('<main id="mainSidebarScene"><section data-od-id="sidebar-status"><div id="devFlowSidebarClaimed"></div><button id="devFlowSidebarMore" hidden></button></section></main><footer class="sidebar-bottom"></footer><button id="sidebarDevFlowDashboard"></button>');
  const { window } = dom;
  let rowHeight = 20;
  window.RozsaSidebar = { claimedMarkup };
  window.innerHeight = 300;
  window.ResizeObserver = class { constructor(callback) { this.callback = callback; } observe(element) { element.observed = true; } };
  window.requestAnimationFrame = callback => callback();
  const timers = new Map();
  let timerId = 0;
  window.setTimeout = (callback, delay) => { const id = ++timerId; timers.set(id, { callback, delay }); return id; };
  window.clearTimeout = id => timers.delete(id);
  Object.defineProperty(window.HTMLElement.prototype, 'getBoundingClientRect', { configurable: true, value() {
    if (this.matches('[data-od-id="sidebar-status"]')) return { height: 100 + window.document.getElementById('devFlowSidebarClaimed').children.length * rowHeight + (window.document.getElementById('devFlowSidebarMore').hidden ? 0 : rowHeight) };
    if (this.id === 'devFlowSidebarClaimed') return { height: this.children.length * rowHeight };
    if (this.id === 'devFlowSidebarMore' || this.classList.contains('dev-flow-claimed-row')) return { height: rowHeight };
    if (this.matches('.sidebar-bottom')) return { height: 40 };
    return { height: 0 };
  } });
  const addWindowListener = window.addEventListener.bind(window);
  window.addEventListener = (name, callback, options) => { if (name !== 'DOMContentLoaded') addWindowListener(name, callback, options); };
  new Script(readFrontend('sidebar.js'), { filename: 'sidebar.js' }).runInContext(dom.getInternalVMContext());
  const calls = [];
  const events = [];
  window.sidebarInvocations = calls;
  window.sidebarEvents = events;
  const items = count => Array.from({ length: count }, (_, index) => ({ id: `TASK-T${String(index + 1).padStart(3, '0')}`, shortId: `T${String(index + 1).padStart(3, '0')}`, title: `A very long claimed task title ${index}` }));
  window.__items = items;
  window.eval("sidebarInvoke = async (name, args) => sidebarInvocations.push({name,args}); sidebarEmit = async name => sidebarEvents.push(name); sidebarDevFlow = {availability:'ready',claimed:__items(5),project:{projectKey:'test-project'},revision:7}; renderDevFlowClaimedRows();");
  const claimed = window.document.getElementById('devFlowSidebarClaimed');
  const more = window.document.getElementById('devFlowSidebarMore');
  const dashboard = window.document.getElementById('sidebarDevFlowDashboard');
  assert.equal(claimed.children.length, 1, '300px height must leave 96px for Sessions and show one row');
  assert.equal(more.hidden, false);
  assert.equal(more.textContent, 'more 4', 'remaining rows must be summarized exactly');
  assert.ok(claimed.children[0].children[2].textContent.includes('very long'), 'title content remains intact for CSS ellipsis');
  claimed.children[0].onpointerenter();
  assert.equal(calls.length, 0, 'hover must wait before requesting detail');
  const runDelayed = milliseconds => {
    for (const [id, timer] of [...timers]) if (timer.delay <= milliseconds) { timers.delete(id); timer.callback(); }
  };
  runDelayed(179);
  assert.equal(calls.length, 0, 'hover duration must filter pointer sweeps');
  runDelayed(180);
  assert.equal(calls.length, 1);
  assert.equal(calls[0].name, 'dev_flow_detail', 'sustained hover requests item detail');
  claimed.children[0].onpointerleave();
  assert.ok(events.includes('dev-flow-detail-dismiss'), 'hover preview closes when the pointer leaves');
  claimed.children[0].onclick();
  assert.equal(calls.length, 2, 'click requests detail immediately');
  claimed.children[0].onpointerleave();
  assert.equal(events.length, 1, 'click pins detail across pointer leave');

  window.innerHeight = 360;
  window.eval('invalidateDevFlowRowMetrics()');
  assert.equal(claimed.children.length, 5, 'taller windows consume all available row slots');
  assert.equal(more.hidden, true);
  window.eval("sidebarDevFlow.claimed = []; sidebarLastDevFlowLayoutKey = ''; renderDevFlowClaimedRows()");
  assert.equal(claimed.children.length, 0, 'zero claimed work renders no rows');
  assert.equal(more.hidden, true);
  rowHeight = 30;
  window.eval('sidebarDevFlow.claimed = __items(5); invalidateDevFlowRowMetrics()');
  assert.equal(claimed.children.length, 2, 'font metric changes invalidate cached row height');
  assert.equal(more.textContent, 'more 3');
  assert.equal([...window.document.getElementById('mainSidebarScene').children].filter(element => element.style.visibility === 'hidden').length, 1, 'one persistent measurement probe is reused');

  window.eval('updateDevFlowDashboardButton(null)');
  assert.equal(dashboard.disabled, true);
  assert.equal(dashboard.title, 'Dev-flow is disabled');
  for (const availability of ['starting', 'loading', 'error']) {
    window.__availability = availability;
    window.eval("updateDevFlowDashboardButton({availability:__availability,availabilityMessage:__availability+' state'})");
    assert.equal(dashboard.disabled, true);
    assert.equal(dashboard.title, `${availability} state`);
  }
  window.eval("updateDevFlowDashboardButton({availability:'ready'})");
  assert.equal(dashboard.disabled, false);
  assert.equal(dashboard.title, 'Open dev-flow dashboard');
  window.close();
});
