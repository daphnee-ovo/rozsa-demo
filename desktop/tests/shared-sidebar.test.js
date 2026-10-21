import { test } from 'vitest';
import assert from 'node:assert/strict';
import { getByRole, getByText } from '@testing-library/dom';
import { sessionsMarkup, init, suspendQuotaTooltip } from '../renderer/scripts/features/sidebar.js';
import { loadTemplate } from '../renderer/scripts/core/template-loader.ts';
import { createDOM, withDOM, readFrontend } from './helpers/browser.js';

test('the actual prototype shim resolves the shared sidebar through its loader', async () => {
  const dom = createDOM();
  try {
    await withDOM(dom, async () => {
      const fragment = await loadTemplate(new URL('../../prototype/templates/features/sidebar.html', import.meta.url));
      dom.window.document.body.append(fragment);
      for (const id of ['sidebar', 'sessionList', 'gitBranch', 'quotaGroup', 'quotaHourRow', 'quotaWeekRow',
        'devFlowSidebarGroup', 'devFlowSidebarClaimed', 'sidebarDevFlowDashboard']) {
        assert(dom.window.document.getElementById(id), id);
      }
    });
  } finally { dom.window.close(); }
});

test('session rendering and real click/keyboard events bind once to the shared sidebar', () => {
  const dom = createDOM(readFrontend('templates/features/sidebar.html'));
  try {
    const { document, MouseEvent, KeyboardEvent } = dom.window;
    document.getElementById('sessionList').innerHTML = sessionsMarkup([
      { name: '<script>', date: 'Today', activity: 'running', active: true },
    ]);
    const session = getByRole(document, 'button', { name: /Today/ });
    assert.equal(getByText(session, '<script>').textContent, '<script>');
    assert.equal(session.querySelector('script'), null);
    assert.equal(getByText(session, 'Today').textContent, 'Today');
    const calls = [];
    const api = { boundModules: new Set(), byId: id => document.getElementById(id),
      activateSession: index => calls.push(['activate', index]), newSession: () => calls.push(['new']),
      toggleSettings: () => calls.push(['settings']), openDevFlowDashboard: () => calls.push(['dashboard']) };
    init(api); init(api);
    session.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    session.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    session.dispatchEvent(new KeyboardEvent('keydown', { key: ' ', bubbles: true }));
    getByRole(document, 'button', { name: 'New session' }).click();
    getByRole(document, 'button', { name: 'Open settings' }).click();
    document.getElementById('sidebarDevFlowDashboard').hidden = false;
    getByRole(document, 'button', { name: 'Dashboard' }).click();
    assert.deepEqual(calls, [['activate', 0], ['activate', 0], ['activate', 0], ['new'], ['settings'], ['dashboard']]);
  } finally { dom.window.close(); }
});

test('quota tooltip suspension clears late DOM content and hides its slot', () => {
  const dom = createDOM('<div id="quotaTooltip">late quota details</div>');
  try {
    suspendQuotaTooltip(dom.window.document);
    const slot = dom.window.document.getElementById('quotaTooltip');
    assert.equal(slot.childNodes.length, 0);
    assert.equal(slot.hidden, true);
    assert.equal(slot.dataset.uiSuspended, 'quota-tooltip');
  } finally { dom.window.close(); }
});
