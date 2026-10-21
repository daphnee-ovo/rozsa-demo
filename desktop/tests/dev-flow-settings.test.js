import { test } from 'vitest';
import assert from 'node:assert/strict';
import { getByRole } from '@testing-library/dom';
import { devFlowSettingsView } from '../renderer/runtime/adapters/sidebar-dev-flow.js';
import { renderSettings } from '../renderer/scripts/features/dev-flow.js';
import { loadApp } from './helpers/browser.js';

const controls = '<button id="devFlowEnabled" role="switch" aria-label="Enable Dev Flow integration"></button>' +
  '<button id="devFlowSidebarStatus" role="switch" aria-label="Show Dev Flow status in sidebar"></button>' +
  '<button id="devFlowDashboardButton" role="switch" aria-label="Show Dev Flow Dashboard button"></button>';

function settings(enabled, available) {
  return { enabled, showSidebarStatus: true, showDashboardButton: true,
    cli: { available, version: '1.0.0' }, project: { availability: 'ready', dashboardUrl: 'http://127.0.0.1:9800/' } };
}

test('stale Dev Flow settings responses cannot replace the latest visible intent', async () => {
  const app = loadApp({ html: controls + '<section id="notificationErrorTray" hidden><button id="notificationErrorTrayButton"></button><span id="notificationErrorCount">0</span><div id="notificationErrorList" hidden></div></section>' });
  try {
    const initial = settings(false, true);
    app.window.RozsaDevFlow = { devFlowSettingsView, renderSettings: view => renderSettings(view, app.document) };
    renderSettings(devFlowSettingsView(initial), app.document);
    const enabled = getByRole(app.document, 'switch', { name: 'Enable Dev Flow integration' });
    assert.equal(enabled.getAttribute('aria-checked'), 'false');

    const pending = [];
    app.window.RozsaUIHost = { port: { request: name => new Promise(resolve => pending.push({ name, resolve })) } };
    app.evaluate('invoke = window.RozsaUIHost.port.request');
    const older = app.evaluate('loadDevFlowSettings()');
    const latest = app.evaluate('loadDevFlowSettings()');
    assert.equal(pending.length, 2);
    assert.ok(pending.every(request => request.name === 'get_dev_flow_settings'));

    pending[1].resolve(settings(true, true));
    await latest;
    assert.equal(enabled.getAttribute('aria-checked'), 'true');

    pending[0].resolve(settings(false, true));
    await older;
    assert.equal(enabled.getAttribute('aria-checked'), 'true');
  } finally { app.close(); }
});

test('enabling Dev Flow restores dependent controls when CLI is available', () => {
  const app = loadApp({ html: controls });
  try {
    const sidebar = getByRole(app.document, 'switch', { name: 'Show Dev Flow status in sidebar' });
    const dashboard = getByRole(app.document, 'switch', { name: 'Show Dev Flow Dashboard button' });
    renderSettings(devFlowSettingsView(settings(false, true)), app.document);
    assert.equal(sidebar.disabled, true);
    assert.equal(dashboard.disabled, true);
    renderSettings(devFlowSettingsView(settings(true, true)), app.document);
    assert.equal(sidebar.disabled, false);
    assert.equal(dashboard.disabled, false);
    renderSettings(devFlowSettingsView(settings(true, false)), app.document);
    assert.equal(sidebar.disabled, true);
    assert.equal(dashboard.disabled, true);
  } finally { app.close(); }
});
