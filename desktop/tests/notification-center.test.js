import assert from 'node:assert/strict';
import { test } from 'vitest';
import { getByRole, getByText, queryByRole } from '@testing-library/dom';
import {
  applyNotificationPresentation, createNotificationElement, errorItemMarkup,
  notificationIcon, notificationMarkup,
} from '../renderer/scripts/features/notifications.js';
import { bindErrorTray, bindToast } from '../renderer/scripts/components/notification.js';
import { reportError } from '../renderer/error-reporting.ts';
import { subscribeNotifications } from '../renderer/runtime/adapters/notifications.js';
import { loadApp, withDOM } from './helpers/browser.js';

const notifications = {
  applyNotificationPresentation, bindErrorTray, bindToast, createNotificationElement,
  notificationIcon, notificationMarkup, subscribeNotifications, errorItemMarkup,
};

function setupApp() {
  let clock = 0;
  let nextTimer = 1;
  const timers = new Map();
  const html = `<!doctype html><html><body><div id="chatMessages"></div><ul id="sessionList"><li>Existing session</li></ul><section id="notificationCenter">
    <div id="notificationStack"></div><div id="notificationErrorTray" hidden>
      <button id="notificationErrorTrayButton" aria-expanded="false"></button>
      <span id="notificationErrorCount">0</span><div id="notificationErrorList" hidden></div>
    </div></section></body></html>`;
  const app = loadApp({ html, globals: {
    RozsaNotifications: notifications,
    setTimeout(callback, delay) { const id = nextTimer++; timers.set(id, { callback, at: clock + delay }); return id; },
    clearTimeout(id) { timers.delete(id); },
  } });
  app.window.showError = reportError;
  app.evaluate('setupNotificationErrorTray()');
  Object.defineProperty(app.window, 'performance', { value: { now: () => clock } });
  return {
    ...app,
    advance(milliseconds) {
      const target = clock + milliseconds;
      while (true) {
        const due = [...timers.entries()].filter(([, timer]) => timer.at <= target)
          .sort((a, b) => a[1].at - b[1].at)[0];
        if (!due) break;
        clock = due[1].at;
        timers.delete(due[0]);
        due[1].callback();
      }
      clock = target;
    },
  };
}

test('stable IDs update in place and toast timers remain independent across hover', async () => {
  const app = setupApp();
  try {
    await withDOM(app.dom, () => {
      app.evaluate(`
        upsertNotification({id:'changing',severity:'error',title:'First',message:'old',timeoutMs:6000});
        upsertNotification({id:'changing',severity:'warning',title:'Second',message:'new',timeoutMs:6000});
      `);
      const changed = app.document.querySelector('#notificationStack [data-notification-id="changing"]');
      assert.ok(changed);
      assert.equal(app.document.querySelectorAll('#notificationStack [data-notification-id="changing"]').length, 1);
      assert.equal(getByRole(app.document, 'status'), changed);
      assert.equal(getByText(changed, 'new').textContent, 'new');
      app.advance(5999);
      assert.ok(app.document.querySelector('#notificationStack [data-notification-id="changing"]'));
      app.advance(1);
      assert.equal(app.document.querySelector('#notificationStack [data-notification-id="changing"]'), null);

      app.evaluate(`upsertNotification({id:'first',severity:'error',title:'First',message:'a',timeoutMs:6000})`);
      app.advance(1000);
      app.evaluate(`upsertNotification({id:'second',severity:'error',title:'Second',message:'b',timeoutMs:6000})`);
      const stack = app.document.getElementById('notificationStack');
      const second = stack.querySelector('[data-notification-id="second"]');
      second.dispatchEvent(new app.window.Event('pointerenter'));
      app.evaluate(`upsertNotification({id:'second',severity:'error',title:'Updated',message:'updated',timeoutMs:6000})`);
      app.advance(5000);
      assert.equal(stack.querySelector('[data-notification-id="first"]'), null);
      assert.ok(stack.querySelector('[data-notification-id="second"]'));
      app.advance(1000);
      assert.ok(stack.querySelector('[data-notification-id="second"]'));
      assert.equal(stack.firstElementChild, second);
      assert.equal(getByText(second, 'updated').textContent, 'updated');
      second.dispatchEvent(new app.window.Event('pointerleave'));
      app.advance(5999);
      assert.ok(stack.querySelector('[data-notification-id="second"]'));
      app.advance(1);
      assert.equal(stack.querySelector('[data-notification-id="second"]'), null);
      assert.equal(app.document.getElementById('notificationErrorCount').textContent, '2');
    });
  } finally { app.close(); }
});

test('dismiss leaves an error unresolved until backend resolution', async () => {
  const app = setupApp();
  try {
    await withDOM(app.dom, () => {
      app.evaluate(`upsertNotification({id:'dismiss',severity:'error',title:'Dismiss',message:'still failing',timeoutMs:6000})`);
      app.document.querySelector('.notification-close').click();
      assert.equal(app.document.querySelector('#notificationStack [data-notification-id="dismiss"]'), null);
      assert.equal(app.document.getElementById('notificationErrorCount').textContent, '1');
      app.document.getElementById('notificationErrorTrayButton').click();
      assert.equal(getByText(app.document.getElementById('notificationErrorList'), 'still failing').textContent, 'still failing');
      app.evaluate(`resolveNotification('dismiss')`);
      assert.equal(app.document.getElementById('notificationErrorCount').textContent, '0');
      assert.equal(queryByRole(app.document.getElementById('notificationErrorList'), 'listitem'), null);
    });
  } finally { app.close(); }
});

test('error tray opens on hover or focus, pins on click, and escape unpins', async () => {
  const app = setupApp();
  try {
    await withDOM(app.dom, () => {
      app.evaluate(`upsertNotification({id:'tray',severity:'error',title:'Tray',message:'active',timeoutMs:6000}); dismissToast('tray'); setupNotificationErrorTray()`);
      const tray = app.document.querySelector('#notificationErrorTray');
      const button = app.document.querySelector('#notificationErrorTrayButton');
      const list = app.document.querySelector('#notificationErrorList');
      tray.dispatchEvent(new app.window.Event('pointerenter'));
      assert.equal(list.hidden, false);
      tray.dispatchEvent(new app.window.Event('pointerleave'));
      assert.equal(list.hidden, true);
      button.dispatchEvent(new app.window.FocusEvent('focusin'));
      assert.equal(list.hidden, false);
      button.dispatchEvent(new app.window.FocusEvent('focusout'));
      assert.equal(list.hidden, true);
      button.click();
      tray.dispatchEvent(new app.window.Event('pointerleave'));
      assert.equal(list.hidden, false);
      app.document.dispatchEvent(new app.window.KeyboardEvent('keydown', { key: 'Escape' }));
      assert.equal(list.hidden, true);
      assert.equal(button.getAttribute('aria-expanded'), 'false');
    });
  } finally { app.close(); }
});

test('shared notification markup escapes content and exposes accessible roles', () => {
  const markup = notificationMarkup({ id: `id<&>'"`, severity: `error<&>'"`, title: '<img src=x>', message: `A & B <script>alert('x')</script>` });
  assert.match(markup, /data-notification-id="id&lt;&amp;&gt;&#39;&quot;"/);
  assert.match(markup, /&lt;img src=x&gt;/);
  assert.match(markup, /A &amp; B &lt;script&gt;alert\(&#39;x&#39;\)&lt;\/script&gt;/);
  assert.doesNotMatch(markup, /<script>|<img/);
  assert.match(errorItemMarkup({ title: '<svg>', message: 'bad & <broken>' }), /&lt;svg&gt;/);
  assert.match(errorItemMarkup({ title: 'x', message: 'y' }), /role="listitem"/);
  assert.match(notificationMarkup({ id: 'i', severity: 'info', title: 'Info', message: 'ready' }), /role="status"/);
  assert.match(notificationMarkup({ id: 'e', severity: 'error', title: 'Error', message: 'failed' }), /role="alert"/);
  assert.match(markup, /aria-label="Dismiss notification"/);
  assert.match(notificationIcon('error'), /data-lucide="circle-x"/);
});

test('runtime adapter accepts structured events and preserves legacy notifications', async () => {
  const handlers = new Map();
  const result = { upserts: [], resolved: [], legacy: [], errors: [] };
  await subscribeNotifications({ subscribe: async (event, handler) => handlers.set(event, handler) }, {
    upsertNotification: value => result.upserts.push(value), resolveNotification: id => result.resolved.push(id),
    showNotification: value => result.legacy.push(value), showError: value => result.errors.push(value),
  });
  await handlers.get('app-notification')({ type: 'upsert', id: 'backend:stable', severity: 'error', title: 'Failure', message: 'broken', timeoutMs: 3000 });
  await handlers.get('app-notification')({ type: 'resolve', id: 'backend:stable' });
  await handlers.get('app-notification')({ type: 'upsert', id: 'invalid', severity: 'fatal', title: 'Failure', message: 'bad severity' });
  await handlers.get('app-notification')({ type: 'resolve', id: '' });
  await handlers.get('notification')('legacy message');
  assert.deepEqual(result.upserts.map(event => event.id), ['backend:stable']);
  assert.deepEqual(result.resolved, ['backend:stable']);
  assert.equal(result.errors.length, 2);
  assert.match(result.errors[0], /Invalid app-notification event/);
  assert.deepEqual(result.legacy, ['legacy message']);
});

test('runtime initialization errors are presented through the internal notification center', async () => {
  const app = setupApp();
  try {
    await withDOM(app.dom, () => {
      reportError('Codex OAuth runtime initialization failed', { id: 'runtime-error-1' });
      const toast = app.document.querySelector('#notificationStack [data-notification-id="runtime-error-1"]');
      assert.ok(toast);
      assert.equal(getByRole(app.document, 'alert'), toast);
      assert.equal(getByText(toast, 'Codex OAuth runtime initialization failed').textContent, 'Codex OAuth runtime initialization failed');
    });
  } finally { app.close(); }
});

test('runtime errors use the application notification and never enter conversation content', async () => {
  const app = setupApp();
  const consoleErrors = [];
  app.window.console.error = (...values) => consoleErrors.push(values);
  try {
    await withDOM(app.dom, () => {
      app.evaluate(`showError('request failed')`);
      const toast = app.document.querySelector('#notificationStack [data-notification-id="runtime-error-1"]');
      assert.ok(toast);
      assert.equal(getByText(toast, 'request failed').textContent, 'request failed');
      assert.deepEqual(consoleErrors, [['request failed']]);
      assert.equal(app.document.querySelector('#chatMessages').textContent, '');
    });
  } finally { app.close(); }
});

test('runtime errors stay in the console when notifications are unavailable or delivery fails', async () => {
  const app = setupApp();
  const consoleErrors = [];
  app.window.console.error = (...values) => consoleErrors.push(values);
  try {
    await withDOM(app.dom, () => {
      app.evaluate(`window.upsertNotification = undefined; showError('notifications unavailable')`);
      assert.deepEqual(consoleErrors, [['notifications unavailable']]);
      assert.equal(app.document.querySelector('#chatMessages').textContent, '');
      assert.equal(app.document.querySelector('#notificationStack').textContent, '');

      app.evaluate(`window.upsertNotification = () => { throw new Error('delivery failed'); }; showError('delivery attempted')`);
      assert.deepEqual(consoleErrors.slice(1).map(([message, error]) => [message, error && String(error)]), [
        ['delivery attempted', undefined],
        ['Could not deliver runtime error notification:', 'Error: delivery failed'],
      ]);
      assert.equal(app.document.querySelector('#chatMessages').textContent, '');
      assert.equal(app.document.querySelector('#notificationStack').textContent, '');
    });
  } finally { app.close(); }
});

test('failed session catalog requests report globally and a later successful render recovers the sidebar', async () => {
  const app = setupApp();
  const consoleErrors = [];
  app.window.console.error = (...values) => consoleErrors.push(values);
  try {
    await withDOM(app.dom, () => {
      app.evaluate(`showSidebarError('sessionList', 'get_sessions failed: offline')`);
      assert.deepEqual(consoleErrors, [['get_sessions failed: offline']]);
      assert.match(app.document.querySelector('#notificationStack').textContent, /get_sessions failed: offline/);
      assert.equal(app.document.querySelector('#sessionList').textContent, 'Existing session');
      assert.equal(app.document.querySelector('#chatMessages').textContent, '');

      app.evaluate(`window.RozsaSidebar = { sessionsMarkup(items) { return items.map(item => '<li>' + item.name + '</li>').join(''); } }; sessions = [{id:'recovered',name:'Recovered',modified:'2026-10-20T10:00:00Z'}]; renderSessionList()`);
      assert.match(app.document.querySelector('#sessionList').textContent, /Recovered/);
      assert.doesNotMatch(app.document.querySelector('#sessionList').textContent, /offline|get_sessions failed/);
    });
  } finally { app.close(); }
});
