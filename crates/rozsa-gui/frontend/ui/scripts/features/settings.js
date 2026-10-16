export const suspendedSettingsPanes = Object.freeze(['general', 'skills', 'tools', 'keyboard-shortcuts', 'permissions']);

export function isSettingsPaneAvailable(pane) {
  return ['appearance', 'extensions', 'dev-flow'].includes(pane);
}

export function configureSettingsSuspensions(root = document) {
  for (const pane of suspendedSettingsPanes) {
    const container = root.querySelector('#pane-' + pane);
    if (container) {
      container.replaceChildren();
      container.dataset.uiSuspended = pane;
    }
    root.querySelectorAll('[data-settings-pane="' + pane + '"]').forEach(button => {
      button.disabled = true;
      button.setAttribute('aria-disabled', 'true');
    });
  }
}

export function syncFontSizeControls(root, value, { updateInput = true } = {}) {
  const range = root.querySelector('#settingsFontSizeRange');
  const input = root.querySelector('#settingsFontSizeInput');
  if (input && updateInput) input.value = String(value);
  if (range) {
    range.value = String(value);
    range.style.setProperty('--appearance-progress', ((Number(value) - Number(range.min)) / (Number(range.max) - Number(range.min))) * 100 + '%');
  }
}

export function renderThemeModeCards(root, mode) {
  root.querySelectorAll('[data-theme-mode-card]').forEach(card => {
    const active = card.dataset.themeModeCard === mode;
    card.classList.toggle('active', active);
    card.setAttribute('aria-pressed', String(active));
  });
}

export function bindSettingsNavigation(root, api) {
  root.addEventListener('click', event => {
    const tab = event.target.closest('.settings-tab');
    if (tab) {
      if (!tab.disabled && isSettingsPaneAvailable(tab.dataset.settingsPane)) api.switchSettingsTab(tab.dataset.settingsPane, tab);
      return;
    }
    if (event.target.closest('[data-action="close-settings"]')) api.closeSettings();
  });
}

export function init(api) {
  if (api.boundModules.has('settings')) return;
  api.boundModules.add('settings');
  configureSettingsSuspensions();
  const settings = api.byId('settingsPanel');
  bindSettingsNavigation(settings, api);
  settings.addEventListener('click', event => {
    const card = event.target.closest('[data-theme-mode-card]');
    if (card) { api.selectThemeModeCard(card.dataset.themeModeCard); return; }
    const toggle = event.target.closest('.setting-toggle');
    if (toggle) { if (!toggle.disabled) api.toggleSetting(toggle); return; }
  });
  settings.addEventListener('input', event => {
    if (['settingsFontSizeRange', 'settingsFontSizeInput'].includes(event.target.id)) api.previewFontSize(event.target.value, event.target);
  });
  settings.addEventListener('change', event => {
    if (['settingsFontSizeRange', 'settingsFontSizeInput'].includes(event.target.id)) api.commitFontSize(event.target.value);
  });
}
