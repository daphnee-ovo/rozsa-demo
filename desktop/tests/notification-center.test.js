import assert from 'node:assert/strict';
import test from 'node:test';
import {
  applyNotificationPresentation, createNotificationElement, errorItemMarkup,
  notificationIcon, notificationMarkup,
} from '../renderer/scripts/features/notifications.js';
import { bindErrorTray, bindToast } from '../renderer/scripts/components/notification.js';
import { showRuntimeError, subscribeNotifications } from '../renderer/runtime/adapters/notifications.js';
import { loadApp, withDOM } from './helpers/browser.js';

const notifications = {
  applyNotificationPresentation, bindErrorTray, bindToast, createNotificationElement,
  notificationIcon, notificationMarkup, showRuntimeError, subscribeNotifications, errorItemMarkup,
};

function setupApp() {
  let clock = 0;
  let nextTimer = 1;
  const timers = new Map();
  const html = `<!doctype html><html><body><section id="notificationCenter">
    <div id="notificationStack"></div><div id="notificationErrorTray" hidden>
      <button id="notificationErrorTrayButton" aria-expanded="false"></button>
      <span id="notificationErrorCount">0</span><div id="notificationErrorList" hidden></div>
    </div></section></body></html>`;
  const app = loadApp({ html, globals: {
    RozsaNotifications: notifications,
    setTimeout(callback, delay) { const id = nextTimer++; timers.set(id, { callback, at: clock + delay }); return id; },
    clearTimeout(id) { timers.delete(id); },
  } });
  Object.defineProperty(app.window, 'performance', { value: { now: () => clock } });
  return {
    ...app,
    state: () => app.evaluate('({ toasts: notificationToasts, errors: unresolvedErrors })'),
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
      let state = app.state();
      const changed = state.toasts.get('changing');
      assert.equal(state.toasts.size, 1);
      assert.equal(changed.severity, 'warning');
      assert.match(changed.element.className, /notification-warning$/);
      assert.equal(changed.element.getAttribute('role'), 'status');
      assert.match(changed.element.querySelector('.notification-icon').innerHTML, /triangle-alert/);
      app.advance(5999);
      assert.equal(app.state().toasts.has('changing'), true);
      app.advance(1);
      assert.equal(app.state().toasts.has('changing'), false);

      app.evaluate(`upsertNotification({id:'first',severity:'error',title:'First',message:'a',timeoutMs:6000})`);
      app.advance(1000);
      app.evaluate(`upsertNotification({id:'second',severity:'error',title:'Second',message:'b',timeoutMs:6000})`);
      const second = app.state().toasts.get('second');
      second.element.dispatchEvent(new app.window.Event('pointerenter'));
      app.evaluate(`upsertNotification({id:'second',severity:'error',title:'Updated',message:'updated',timeoutMs:6000})`);
      app.advance(5000);
      assert.equal(app.state().toasts.has('first'), false);
      assert.equal(app.state().toasts.has('second'), true);
      app.advance(1000);
      assert.equal(app.state().toasts.has('second'), true);
      assert.equal(app.document.querySelector('#notificationStack').firstElementChild, second.element);
      second.element.dispatchEvent(new app.window.Event('pointerleave'));
      app.advance(5999);
      assert.equal(app.state().toasts.has('second'), true);
      app.advance(1);
      state = app.state();
      assert.equal(state.toasts.has('second'), false);
      assert.equal(state.errors.size, 2);
    });
  } finally { app.close(); }
});

test('dismiss leaves an error unresolved until backend resolution', async () => {
  const app = setupApp();
  try {
    await withDOM(app.dom, () => {
      app.evaluate(`upsertNotification({id:'dismiss',severity:'error',title:'Dismiss',message:'still failing',timeoutMs:6000})`);
      app.document.querySelector('.notification-close').click();
      assert.equal(app.state().toasts.has('dismiss'), false);
      assert.equal(app.state().errors.has('dismiss'), true);
      app.evaluate(`resolveNotification('dismiss')`);
      assert.equal(app.state().errors.has('dismiss'), false);
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
