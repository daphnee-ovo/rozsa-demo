import assert from 'node:assert/strict';
import test from 'node:test';
import { JSDOM } from 'jsdom';
import { act } from 'react';
import { mountSettingsPanes } from '../renderer/scripts/features/settings.tsx';
import { createElectronAdapter } from '../renderer/runtime/adapters/electron.ts';
import { BackendService } from '../main/backend-service.ts';
import { RozsaBridge } from '../preload/bridge.ts';
import { createIsolatedBackendEnvironment } from './helpers/backend-config.ts';
import { installSessionBridge } from '../main/session-bridge.ts';
import { EventEmitter } from 'node:events';
import { join, resolve } from 'node:path';

const root = resolve(import.meta.dirname, '../..');
const backendBinary = join(root, 'dist', process.platform === 'win32' ? 'rozsa-backend.exe' : 'rozsa-backend');

function theme(id, mode, overrides = {}) {
  return { id, name: id, mode, accent: '#7D4F59', background: '#FFFFFF', foreground: '#211D1F', uiFont: 'system-ui', codeFont: 'monospace', variables: {}, ...overrides };
}
function snapshot(scope = 'global', models = [
  { id: 'claude-test', name: 'Claude Test', provider: 'anthropic', reasoning: true, contextWindow: 64000 },
  { id: 'haiku-test', name: 'Haiku Test', provider: 'anthropic', reasoning: true, contextWindow: 32000 },
]) {
  const lightTheme = theme('rozsa', 'light'), darkTheme = theme('rozsa-dark', 'dark');
  return {
    scope,
    settings: {
      defaultProvider: 'anthropic', defaultModel: 'claude-test', smallModel: 'haiku-test', defaultThinkingEffort: 'off',
      compaction: { enabled: false, triggerRatio: 0.9, targetRatio: 0.7 }, retry: { timeoutMs: null, maxRetries: null, maxRetryDelayMs: null },
      autoSessionNaming: true, steeringMode: 'one-at-a-time', followUpMode: 'one-at-a-time', runningSendMode: 'queue',
      transport: 'auto', blockImages: false, hideThinking: false,
      appearance: { themeMode: 'system', fontSize: 14, translucentSidebar: false, showRateLimits: false, showHourlyRateLimit: false, showWeeklyRateLimit: false, rateLimitDisplayMode: 'remained', lightTheme: 'rozsa', darkTheme: 'rozsa-dark' },
    },
    appearance: { themeMode: 'system', fontSize: 14, translucentSidebar: false, lightTheme, darkTheme },
    themes: [{ id: lightTheme.id, name: lightTheme.name, mode: lightTheme.mode }, { id: darkTheme.id, name: darkTheme.name, mode: darkTheme.mode }],
    models,
  };
}
function harness({ failUpdate = false, loading } = {}) {
  const dom = new JSDOM('<main id="settings"><div id="pane-general" class="settings-pane active"></div><div id="pane-appearance" class="settings-pane"></div></main>', { url: 'http://localhost/' });
  dom.window.matchMedia = () => ({ matches: false, addEventListener() {}, removeEventListener() {} });
  const globals = ['window', 'document', 'navigator', 'HTMLElement', 'Element', 'matchMedia', 'IS_REACT_ACT_ENVIRONMENT'];
  const previous = new Map(globals.map(key => [key, Object.getOwnPropertyDescriptor(globalThis, key)]));
  const replacements = {
    window: dom.window, document: dom.window.document, navigator: dom.window.navigator,
    HTMLElement: dom.window.HTMLElement, Element: dom.window.Element,
    matchMedia: () => ({ matches: false, addEventListener() {}, removeEventListener() {} }),
    IS_REACT_ACT_ENVIRONMENT: true,
  };
  for (const [key, value] of Object.entries(replacements)) Object.defineProperty(globalThis, key, { configurable: true, writable: true, value });
  const calls = [];
  const state = { global: snapshot('global'), project: snapshot('project') };
  const bridge = {
    getSettings(scope) { calls.push(['get', scope]); return loading || Promise.resolve(state[scope]); },
    async updateSetting(scope, key, value) {
      calls.push(['update', scope, key, value]);
      if (failUpdate) throw new Error('write denied');
      const next = structuredClone(state[scope]);
      if (key === 'auto_compact') next.settings.compaction.enabled = value;
      if (key === 'hide_thinking') next.settings.hideThinking = value;
      if (key === 'model') { next.settings.defaultProvider = value.provider; next.settings.defaultModel = value.id; }
      if (key === 'small_model') next.settings.smallModel = value;
      if (key === 'appearance_show_rate_limits') next.settings.appearance.showRateLimits = value;
      if (key === 'appearance_font_size') { next.appearance.fontSize = value; next.settings.appearance.fontSize = value; }
      state[scope] = next;
      return next;
    },
    async resetSetting(scope, key) { calls.push(['reset', scope, key]); return state[scope]; },
    async listThemes(scope) { return state[scope].themes; },
    async getTheme() { return theme('rozsa', 'light'); },
    async saveTheme() {},
  };
  let dispose;
  return {
    dom, calls, bridge,
    async mount() { await act(async () => { dispose = mountSettingsPanes(document, bridge); await Promise.resolve(); }); },
    async flush(action) { await act(async () => { action(); await new Promise(resolve => setTimeout(resolve, 0)); }); },
    async close() {
      await act(async () => dispose?.()); dom.window.close();
      for (const key of globals) {
        const descriptor = previous.get(key);
        if (descriptor) Object.defineProperty(globalThis, key, descriptor); else delete globalThis[key];
      }
    },
  };
}

test('settings panes show configured model metadata, save selection and expose dependent controls', async () => {
  const app = harness();
  try {
    await app.mount();
    const { document } = app.dom.window;
    assert.equal(document.querySelector('#pane-general [aria-label="Model"]').value, 'anthropic\u0000claude-test');
    assert.equal(document.querySelector('#pane-general [aria-label="Model"]').disabled, false);
    const rows = [...document.querySelectorAll('#pane-general .setting-item')];
    assert.equal(rows.find(row => row.querySelector('.setting-label')?.textContent === 'Provider')?.querySelector('.setting-value')?.textContent, 'anthropic');
    assert.equal(rows.find(row => row.querySelector('.setting-label')?.textContent === 'Context window')?.querySelector('.setting-value')?.textContent, '64,000');
    assert.equal(document.querySelector('#pane-appearance [aria-label="Font size (px)"]').value, '14');
    assert.equal(document.querySelector('#pane-appearance [aria-label="Show hourly limit"]').disabled, true);
    await app.flush(() => { document.querySelector('#pane-general [aria-label="Settings scope"]').value = 'project'; document.querySelector('#pane-general [aria-label="Settings scope"]').dispatchEvent(new app.dom.window.Event('change', { bubbles: true })); });
    await app.flush(() => document.querySelector('#pane-general [aria-label="Auto compact"]').click());
    await app.flush(() => { document.querySelector('#pane-general [aria-label="Model"]').value = 'anthropic\u0000haiku-test'; document.querySelector('#pane-general [aria-label="Model"]').dispatchEvent(new app.dom.window.Event('change', { bubbles: true })); });
    assert.ok(app.calls.some(call => call[0] === 'get' && call[1] === 'project'));
    assert.ok(app.calls.some(call => call[0] === 'update' && call[1] === 'project' && call[2] === 'auto_compact' && call[3] === true));
    assert.ok(app.calls.some(call => call[0] === 'update' && call[1] === 'project' && call[2] === 'model' && call[3].id === 'haiku-test'));
  } finally { await app.close(); }
});

test('an empty model catalog is visible and disables only model selection', async () => {
  const app = harness();
  app.bridge.getSettings = scope => Promise.resolve(snapshot(scope, []));
  try {
    await app.mount();
    const { document } = app.dom.window;
    assert.equal(document.querySelector('#pane-general [aria-label="Model"]').disabled, true);
    assert.match(document.querySelector('#pane-general [aria-label="Model"]').textContent, /No configured models/);
    assert.equal(document.querySelector('#pane-general [aria-label="Auto compact"]').disabled, false);
  } finally { await app.close(); }
});

test('hide thinking updates the conversation display class after the setting is saved', async () => {
  const app = harness();
  try {
    await app.mount();
    const { document } = app.dom.window;
    assert.equal(document.body.classList.contains('hide-thinking'), false);
    await app.flush(() => document.querySelector('#pane-general [aria-label="Hide thinking blocks"]').click());
    assert.equal(document.body.classList.contains('hide-thinking'), true);
  } finally { await app.close(); }
});

test('settings keep save failures visible without discarding the previous value', async () => {
  const app = harness({ failUpdate: true });
  try {
    await app.mount();
    const { document } = app.dom.window;
    await app.flush(() => document.querySelector('#pane-general [aria-label="Auto compact"]').click());
    assert.match(document.querySelector('#pane-general [role="alert"]').textContent, /write denied/);
    assert.equal(document.querySelector('#pane-general [aria-label="Auto compact"]').getAttribute('aria-checked'), 'false');
    assert.equal(app.calls.filter(call => call[0] === 'update').length, 1);
  } finally { await app.close(); }
});

test('settings loading and read errors are represented explicitly', async () => {
  let rejectRead;
  const loading = new Promise((_, reject) => { rejectRead = reject; });
  const app = harness({ loading });
  try {
    await app.mount();
    assert.match(app.dom.window.document.querySelector('#pane-general [role="status"]').textContent, /Loading settings/);
    rejectRead(new Error('settings file unreadable'));
    await app.flush(() => {});
    assert.match(app.dom.window.document.querySelector('#pane-general [role="alert"]').textContent, /settings file unreadable/);
  } finally { await app.close(); }
});

test('the Electron preload contract reads and updates layered settings through the Go process', async () => {
  const backend = new BackendService(backendBinary, [], { env: await createIsolatedBackendEnvironment() });
  const trustedURL = 'file:///synthetic/index.html';
  const webContents = Object.assign(new EventEmitter(), { mainFrame: { url: trustedURL } });
  const vibrancyCalls = [];
  const window = Object.assign(new EventEmitter(), { webContents, isDestroyed: () => false, setVibrancy: value => vibrancyCalls.push(value) });
  const handlers = new Map();
  const ipc = { handle: (channel, handler) => handlers.set(channel, handler), removeHandler: channel => handlers.delete(channel) };
  installSessionBridge(window, ipc, backend, trustedURL);
  const event = { sender: webContents, senderFrame: webContents.mainFrame };
  const bridge = RozsaBridge.create((channel, ...args) => handlers.get(channel)(event, ...args));
  try {
    await backend.initialize();
    assert.equal((await backend.getSettings('project')).models[0].id, 'test');
    const initial = await bridge.getSettings('project');
    assert.equal(initial.scope, 'project');
    assert.equal(initial.appearance.fontSize, 14);
    if (process.platform === 'darwin') assert.deepEqual(vibrancyCalls, [null]);
    assert.deepEqual(initial.models.map(model => [model.provider, model.id, model.contextWindow]), [['synthetic', 'test', 4096]]);
    const selected = await bridge.updateSetting('project', 'model', { provider: 'synthetic', id: 'test' });
    assert.equal(selected.settings.defaultProvider, 'synthetic');
    assert.equal(selected.settings.defaultModel, 'test');
    const thinking = await bridge.updateSetting('project', 'thinking', 'high');
    assert.equal(thinking.settings.defaultThinkingEffort, 'off');
    assert.deepEqual((await bridge.listThemes('project')).map(item => item.id), ['rozsa', 'rozsa-dark']);
    const updated = await bridge.updateSetting('project', 'appearance_font_size', 19);
    assert.equal(updated.appearance.fontSize, 19);
    const reset = await bridge.resetSetting('project', 'appearance_font_size');
    assert.equal(reset.appearance.fontSize, 14);
    await assert.rejects(bridge.updateSetting('project', 'appearance_theme_mode', 'sepia'), /invalid theme mode/i);
  } finally { window.emit('closed'); await backend.stop(); }
});

test('the existing renderer settings adapter maps legacy settings intents to typed Go calls', async () => {
  const state = snapshot();
  const calls = [];
  const adapter = createElectronAdapter({
    async getSessions() { return { ok: true, sessions: [] }; },
    async getSettings(scope) { calls.push(['get', scope]); return { ...state, scope }; },
    async listThemes(scope) { calls.push(['themes', scope]); return state.themes; },
    async updateSetting(scope, key, value) { calls.push(['update', scope, key, value]); return state; },
  });
  const settings = await adapter.request('get_settings');
  assert.equal(settings.auto_compact, false);
  assert.equal(settings.model_provider, 'anthropic');
  await adapter.request('update_setting', { key: 'auto_compact', value: 'true' });
  assert.deepEqual(calls, [['get', 'global'], ['update', 'global', 'auto_compact', true]]);
});
