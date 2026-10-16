import assert from 'node:assert/strict';
import { test } from 'node:test';
import * as settings from '../renderer/scripts/features/settings.js';
import { bindSettingSwitch, createSettingsAdapter } from '../renderer/runtime/adapters/settings.js';
import { createDOM, loadApp, readFrontend, withDOM } from './helpers/browser.js';

test('settings pane navigation, toggles and appearance controls preserve supported behavior', async () => {
  const dom = createDOM(`<section id="settingsPanel">
    <button class="settings-tab" data-settings-pane="permissions"></button>
    <button class="settings-tab" data-settings-pane="extensions"></button>
    <button class="setting-toggle" id="toggle"></button>
    <button data-theme-mode-card="dark"></button>
    <input id="settingsFontSizeRange" type="range" min="5" max="30" value="14">
    <input id="settingsFontSizeInput" type="number" value="14">
  </section>
  <section id="pane-permissions"><p>old</p></section><button data-settings-pane="permissions"></button>`);
  try {
    await withDOM(dom, async () => {
      const calls = [];
      const api = createSettingsAdapter({
        switchSettingsTab: pane => calls.push(pane), closeSettings() {}, selectThemeModeCard: mode => calls.push(mode),
        previewFontSize: value => calls.push(value), commitFontSize: value => calls.push(value),
        boundModules: new Set(), byId: id => document.getElementById(id),
      });
      settings.init(api);
      assert.deepEqual(Array.from(document.querySelector('#pane-permissions').children), []);
      const permissions = document.querySelector('[data-settings-pane="permissions"]');
      assert.equal(permissions.disabled, true);
      assert.equal(permissions.getAttribute('aria-disabled'), 'true');
      document.querySelector('#settingsPanel').dispatchEvent(new dom.window.MouseEvent('click', { bubbles: true }));
      const extension = document.querySelector('[data-settings-pane="extensions"]');
      extension.dispatchEvent(new dom.window.MouseEvent('click', { bubbles: true }));
      assert.deepEqual(calls, ['extensions']);
      const toggle = document.getElementById('toggle');
      bindSettingSwitch(toggle, enabled => calls.push(enabled), control => control.dataset.enabled === 'true', (control, enabled) => { control.dataset.enabled = String(enabled); });
      assert.equal(toggle.onclick, null);
      api.toggleSetting(toggle);
      assert.deepEqual(calls, ['extensions', true]);
      settings.syncFontSizeControls(document, 18);
      assert.equal(document.getElementById('settingsFontSizeInput').value, '18');
      assert.equal(document.getElementById('settingsFontSizeRange').value, '18');
      assert.equal(document.getElementById('settingsFontSizeRange').style.getPropertyValue('--appearance-progress'), '52%');
      settings.syncFontSizeControls(document, 19, { updateInput: false });
      assert.equal(document.getElementById('settingsFontSizeInput').value, '18');
      assert.equal(document.getElementById('settingsFontSizeRange').value, '19');
      settings.renderThemeModeCards(document, 'dark');
      assert.equal(document.querySelector('[data-theme-mode-card="dark"]').getAttribute('aria-pressed'), 'true');
      assert.equal(settings.isSettingsPaneAvailable('permissions'), false);
    });
  } finally { dom.window.close(); }
});

test('suspended settings routes and mutations stay inert while font size edits remain usable', async () => {
  const app = await loadApp({ html: `<body><main id="mainContentScene"></main><section id="settingsPanel"></section>
    <div id="pane-general"><p>old</p></div><div id="pane-skills"></div><div id="pane-tools"></div>
    <div id="pane-keyboard-shortcuts"></div><div id="pane-permissions"></div>
    <input id="settingsFontSizeRange" type="range" min="5" max="30" value="14"><input id="settingsFontSizeInput" type="number" value="14"></body>` });
  try {
    const { document, evaluate } = app;
    const calls = [];
    evaluate(`window.RozsaSettings={isSettingsPaneAvailable:${settings.isSettingsPaneAvailable.toString()},syncFontSizeControls:${settings.syncFontSizeControls.toString()},renderThemeModeCards:${settings.renderThemeModeCards.toString()},bindSettingSwitch(){}};
      invoke=async(command,payload)=>{testCalls.push([command,payload]);return command==='get_settings'?{appearance:{fontSize:18}}:[];};
      nativeSplitMode=true;`);
    app.window.testCalls = calls;
    await evaluate(`(async()=>{await requestGuiScene('settings','general');switchSettingsTab('skills');
      renderGeneralSettings({});renderCapabilityPane('tools',[{}]);renderPermissionSettings();renderPermissionRuleList('deny');
      openPermissionRuleEditor('deny');await savePermissionRule();await replacePermissionRules('deny',[]);await movePermissionRule('deny','allow','Bash(*)');
      renderKeyBindings();beginKeyBindingCapture('openSettings');await resetKeyBinding('openSettings');
      if(await saveCapturedKeyBinding({})!==false)throw Error('suspended keybinding save was enabled');})()`);
    assert.deepEqual(calls, []);
    evaluate(`window.RozsaSettings.bindSettingSwitch=(control,onChange,read,render)=>{control.onclick=null;};
      renderSettingsPane=()=>{};applySelectedTheme=async()=>{};loadDevFlowSettings=()=>{};applyFontSize=value=>testCalls.push(['font',value]);`);
    await evaluate('loadSettings()');
    assert.deepEqual(calls.map(call => call[0]), ['get_settings', 'list_themes']);
    calls.length = 0;
    const input = document.getElementById('settingsFontSizeInput');
    const range = document.getElementById('settingsFontSizeRange');
    input.value = '';
    evaluate("previewFontSize('')");
    assert.equal(input.value, '');
    input.value = '1';
    evaluate("previewFontSize('1')");
    assert.equal(input.value, '1');
    calls.length = 0;
    input.value = '18';
    evaluate('previewFontSize(18)');
    assert.equal(range.value, '18');
    assert.equal(input.value, '18');
    assert.equal(range.style.getPropertyValue('--appearance-progress'), '52%');
    assert.deepEqual(JSON.parse(JSON.stringify(calls)), [['font', 18]]);
    calls.length = 0;
    await evaluate('commitFontSize(19)');
    assert.deepEqual(JSON.parse(JSON.stringify(calls[0])), ['update_setting', { key: 'appearance_font_size', value: '19' }]);
    assert.deepEqual(calls.slice(1).map(call => call[0]), ['get_settings', 'list_themes']);
    calls.length = 0;
    evaluate('window.RozsaGuiShared={};');
    app.window.ResizeObserver = class { observe() {} };
    evaluate(readFrontend('sidebar.js'));
    await evaluate(`sidebarInvoke=async()=>{throw Error('unexpected sidebar IPC');};(async()=>{await requestSidebarScene('settings','permissions');selectSidebarSettingsPane('tools');})()`);
    assert.deepEqual(calls, []);
  } finally { app.close(); }
});
