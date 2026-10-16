import test from 'node:test';
import assert from 'node:assert/strict';
import { loadApp } from './helpers/browser.js';

test('stale Dev Flow settings responses cannot replace the latest intent', () => {
  const app = loadApp();
  try {
    app.evaluate('devFlowSettings = { enabled: true }; devFlowSettingsRevision = 2; window.renders = 0; renderDevFlowSettings = () => { window.renders++; };');
    assert.equal(app.evaluate('acceptDevFlowSettingsSnapshot(1, { enabled: false })'), false);
    assert.equal(app.evaluate('devFlowSettings.enabled'), true);
    assert.equal(app.window.renders, 0);
    assert.equal(app.evaluate('acceptDevFlowSettingsSnapshot(2, { enabled: false })'), true);
    assert.equal(app.evaluate('devFlowSettings.enabled'), false);
    assert.equal(app.window.renders, 1);
  } finally { app.close(); }
});

test('enabling Dev Flow restores dependent controls when CLI is available', () => {
  const app = loadApp();
  try {
    assert.equal(app.evaluate('devFlowDependentControlsDisabled({ enabled: false, cli: { available: true } })'), true);
    assert.equal(app.evaluate('devFlowDependentControlsDisabled({ enabled: true, cli: { available: true } })'), false);
    assert.equal(app.evaluate('devFlowDependentControlsDisabled({ enabled: true, cli: { available: false } })'), true);
  } finally { app.close(); }
});
