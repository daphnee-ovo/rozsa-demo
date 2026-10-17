// @ts-nocheck
function renderSettingsPane(settings) {
  if (!settings) return;
  document.body.classList.toggle('hide-thinking', Boolean(settings.hide_thinking));

  // Thinking effort
  const thinkingSel = document.getElementById('settingsThinkingEffort');
  if (thinkingSel && settings.thinkingEffort) {
    thinkingSel.value = settings.thinkingEffort.toLowerCase();
  }


  // Model selector in settings
  const modelSel = document.getElementById('settingsModelSelect');
  if (modelSel && settings.model_id) {
    modelSel.value = settings.model_id;
  }
  const smallModelSel = document.getElementById('settingsSmallModelSelect');
  if (smallModelSel) smallModelSel.value = settings.small_model || '';

  // Provider info
  const providerEl = document.getElementById('settingsProvider');
  if (providerEl && settings.model_provider) {
    providerEl.textContent = settings.model_provider;
  }

  // Render general pane settings with live controls
  renderAppearanceSettings(settings.appearance);
  renderGeneralSettings(settings);
}

function renderAppearanceSettings(appearance) {
  if (!appearance) return;

  const modeSelect = document.getElementById('settingsThemeMode');
  if (modeSelect) {
    modeSelect.value = appearance.themeMode || 'system';
    renderThemeModeCards(modeSelect.value);
    modeSelect.onchange = () => saveSetting('appearance_theme_mode', modeSelect.value);
  }

  const range = document.getElementById('settingsFontSizeRange');
  const input = document.getElementById('settingsFontSizeInput');
  if (range && input) {
    window.RozsaSettings.syncFontSizeControls(document, appearance.fontSize);
  }

  quotaDisplayEnabled = appearance.showRateLimits !== false;
  weeklyQuotaDisplayEnabled = appearance.showWeeklyRateLimit !== false;
  hourlyQuotaDisplayEnabled = appearance.showHourlyRateLimit !== false;
  rateLimitDisplayMode = appearance.rateLimitDisplayMode || 'remained';
  const quotaSwitch = document.getElementById('settingsShowRateLimits');
  if (quotaSwitch) {
    setSettingSwitch(quotaSwitch, quotaDisplayEnabled);
    wireSettingSwitch('settingsShowRateLimits', enabled => {
      quotaDisplayEnabled = enabled;
      setRateLimitSettingsDisabled(!enabled);
      updateQuotaVisibility({ provider: currentSettings?.model_provider });
      return saveSetting('appearance_show_rate_limits', String(enabled));
    });
  }
  const weeklyQuotaSwitch = document.getElementById('settingsShowWeeklyRateLimit');
  if (weeklyQuotaSwitch) {
    setSettingSwitch(weeklyQuotaSwitch, weeklyQuotaDisplayEnabled);
    wireSettingSwitch('settingsShowWeeklyRateLimit', enabled => {
      weeklyQuotaDisplayEnabled = enabled;
      updateQuotaBars(quotaSnapshot);
      return saveSetting('appearance_show_weekly_rate_limit', String(enabled));
    });
  }
  const hourlyQuotaSwitch = document.getElementById('settingsShowHourlyRateLimit');
  if (hourlyQuotaSwitch) {
    setSettingSwitch(hourlyQuotaSwitch, hourlyQuotaDisplayEnabled);
    hourlyQuotaSwitch.disabled = !quotaDisplayEnabled;
    wireSettingSwitch('settingsShowHourlyRateLimit', enabled => { if (!quotaDisplayEnabled) return; hourlyQuotaDisplayEnabled = enabled; updateQuotaBars(quotaSnapshot); return saveSetting('appearance_show_hourly_rate_limit', String(enabled)); });
  }
  if (weeklyQuotaSwitch) weeklyQuotaSwitch.disabled = !quotaDisplayEnabled;
  const rateLimitMode = document.getElementById('settingsRateLimitDisplayMode');
  if (rateLimitMode) {
    rateLimitMode.value = rateLimitDisplayMode;
    rateLimitMode.disabled = !quotaDisplayEnabled;
    rateLimitMode.onchange = () => { if (!quotaDisplayEnabled) return; rateLimitDisplayMode = rateLimitMode.value; updateQuotaBars(quotaSnapshot); saveSetting('appearance_rate_limit_display_mode', rateLimitDisplayMode); };
  }
  setRateLimitSettingsDisabled(!quotaDisplayEnabled);
  const translucentSidebar = document.getElementById('settingsTranslucentSidebar');
  if (translucentSidebar) {
    setSettingSwitch(translucentSidebar, appearance.translucentSidebar);
    translucentSidebar.closest('.appearance-sidebar-option').hidden = !appearance.isMacos;
    wireSettingSwitch('settingsTranslucentSidebar', enabled =>
      saveSetting('appearance_translucent_sidebar', String(enabled)));
  }

  renderThemeSelect('light', appearance.lightTheme);
  renderThemeSelect('dark', appearance.darkTheme);
  renderThemeControls('light', themeDefinitions.light, appearance.isMacos);
  renderThemeControls('dark', themeDefinitions.dark, appearance.isMacos);
  wireDevFlowSettings();
  installSystemThemeListener();
}

function previewFontSize(value, control = null) {
  if (String(value).trim() === '') return;
  const size = clampFontSize(value);
  window.RozsaSettings.syncFontSizeControls(document, size, { updateInput: control?.id === 'settingsFontSizeRange' });
  applyFontSize(size);
}

function commitFontSize(value) {
  const size = clampFontSize(value);
  window.RozsaSettings.syncFontSizeControls(document, size);
  return saveSetting('appearance_font_size', String(size));
}

function setRateLimitSettingsDisabled(disabled) {
  for (const id of [
    'settingsShowHourlyRateLimit',
    'settingsShowWeeklyRateLimit',
    'settingsRateLimitDisplayMode',
  ]) {
    const control = document.getElementById(id);
    if (!control) continue;
    control.disabled = disabled;
    control.closest('.setting-item')?.classList.toggle('is-disabled', disabled);
  }
}

function selectThemeModeCard(mode) {
  const modeSelect = document.getElementById('settingsThemeMode');
  if (!modeSelect || !['system', 'light', 'dark'].includes(mode)) return;
  modeSelect.value = mode;
  renderThemeModeCards(mode);
  saveSetting('appearance_theme_mode', mode);
}

function renderThemeModeCards(mode) {
  window.RozsaSettings.renderThemeModeCards(document, mode);
}

function renderThemeSelect(mode, selectedId) {
  const select = document.getElementById(mode === 'light' ? 'settingsLightTheme' : 'settingsDarkTheme');
  if (!select) return;
  select.innerHTML = '';
  availableThemes
    .filter(theme => theme.mode === mode)
    .forEach(theme => {
      const option = document.createElement('option');
      option.value = theme.id;
      option.textContent = theme.name;
      select.appendChild(option);
    });
  select.value = selectedId;
  select.onchange = async () => {
    await saveSetting('appearance_' + mode + '_theme', select.value);
  };
}

function renderThemeControls(mode, theme, isMacos) {
  if (!theme) return;
  const prefix = mode === 'light' ? 'light' : 'dark';
  setThemeColorControlValue(prefix + 'ThemeAccent', theme.accent);
  setThemeColorControlValue(prefix + 'ThemeBackground', theme.background);
  setThemeColorControlValue(prefix + 'ThemeForeground', theme.foreground);
  setThemeControlValue(prefix + 'ThemeUiFont', theme.uiFont);
  setThemeControlValue(prefix + 'ThemeCodeFont', theme.codeFont);

  const textIds = [
    prefix + 'ThemeUiFont',
    prefix + 'ThemeCodeFont',
  ];
  textIds.forEach(id => {
    const control = document.getElementById(id);
    if (control) {
      control.oninput = () => previewTheme(mode);
      control.onchange = () => scheduleThemeSave(mode);
    }
  });
  ['Accent', 'Background', 'Foreground'].forEach(field => {
    const id = prefix + 'Theme' + field;
    const text = document.getElementById(id);
    const picker = document.getElementById(id + 'Picker');
    if (picker) {
      picker.oninput = () => {
        const value = picker.value.toUpperCase();
        if (text) text.value = value;
        updateThemeColorVisual(id, value);
        previewTheme(mode);
      };
      picker.onchange = () => scheduleThemeSave(mode);
    }
    if (text) {
      text.oninput = () => {
        updateThemeColorVisual(id, text.value);
        if (isHexColor(text.value)) previewTheme(mode);
      };
      text.onchange = () => scheduleThemeSave(mode);
    }
  });
}

function setThemeControlValue(id, value) {
  const control = document.getElementById(id);
  if (control) control.value = value || '';
}

function isHexColor(value) {
  return /^#[0-9A-Fa-f]{6}$/.test(String(value || '').trim());
}

function normalizeHexColor(value, fallback = '#000000') {
  const normalized = String(value || '').trim().toUpperCase();
  return isHexColor(normalized) ? normalized : fallback;
}

function themeColorTextColor(hex) {
  const value = normalizeHexColor(hex);
  const channels = [1, 3, 5].map(offset => Number.parseInt(value.slice(offset, offset + 2), 16) / 255);
  const linear = channels.map(channel => channel <= 0.03928 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4);
  const luminance = 0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2];
  return luminance > 0.46 ? '#211E20' : '#FFFFFF';
}

function updateThemeColorVisual(id, value) {
  if (!isHexColor(value)) return;
  const hex = normalizeHexColor(value);
  const control = document.getElementById(id);
  const picker = document.getElementById(id + 'Picker');
  const wrapper = control?.closest('.theme-color-control');
  if (picker) picker.value = hex;
  if (wrapper) {
    wrapper.style.setProperty('--theme-color', hex);
    wrapper.style.setProperty('--theme-color-text', themeColorTextColor(hex));
  }
}

function setThemeColorControlValue(id, value) {
  const hex = normalizeHexColor(value);
  setThemeControlValue(id, hex);
  updateThemeColorVisual(id, hex);
}

function getThemeControlValue(id) {
  const control = document.getElementById(id);
  return control ? control.value.trim() : '';
}

function readThemeEditor(mode) {
  const prefix = mode === 'light' ? 'light' : 'dark';
  const base = themeDefinitions[mode];
  if (!base) return null;
  return {
    ...base,
    accent: getThemeControlValue(prefix + 'ThemeAccent'),
    background: getThemeControlValue(prefix + 'ThemeBackground'),
    foreground: getThemeControlValue(prefix + 'ThemeForeground'),
    uiFont: getThemeControlValue(prefix + 'ThemeUiFont'),
    codeFont: getThemeControlValue(prefix + 'ThemeCodeFont'),
  };
}

function prepareThemeForPersistence(mode, theme) {
  const persisted = { ...theme };
  const builtinId = mode === 'light' ? 'rozsa' : 'rozsa-dark';
  if (persisted.id === builtinId) {
    persisted.id = mode === 'light' ? 'rozsa-custom' : 'rozsa-dark-custom';
    persisted.name = mode === 'light' ? 'Rozsa Custom' : 'Rozsa Dark Custom';
  }
  return persisted;
}

async function persistTheme(mode, theme) {
  await invoke('save_theme', { theme: theme });
  themeDefinitions[mode] = theme;
  await saveSetting('appearance_' + mode + '_theme', theme.id);
}

function enqueueThemeSave(mode, theme, errorPrefix) {
  themeSaveQueues[mode] = themeSaveQueues[mode]
    .catch(() => {})
    .then(() => persistTheme(mode, theme))
    .catch(error => {
      showError(errorPrefix + String(error));
    });
  return themeSaveQueues[mode];
}

function scheduleThemeSave(mode) {
  const draft = readThemeEditor(mode);
  if (!draft) return Promise.resolve();
  if (!['accent', 'background', 'foreground'].every(field => isHexColor(draft[field]))) {
    return Promise.resolve();
  }
  return enqueueThemeSave(
    mode,
    prepareThemeForPersistence(mode, draft),
    'Failed to persist theme settings: '
  );
}

async function saveThemeAsCustom(mode) {
  const theme = readThemeEditor(mode);
  if (!theme) return;
  if (!['accent', 'background', 'foreground'].every(field => isHexColor(theme[field]))) {
    showError('Accent, background, and foreground must be six-digit HEX colors.');
    return;
  }
  const name = window.prompt('Custom theme name', theme.name || 'My Theme');
  if (name === null) return;
  const trimmedName = name.trim();
  const id = trimmedName.toLowerCase().replace(/[^a-z0-9_-]+/g, '-').replace(/^-+|-+$/g, '');
  if (!id) {
    showError('Theme name must contain letters, numbers, - or _.');
    return;
  }
  theme.id = id;
  theme.name = trimmedName;
  await enqueueThemeSave(mode, theme, 'Failed to save custom theme: ');
}

function previewTheme(mode) {
  const theme = readThemeEditor(mode);
  if (!theme || !currentSettings?.appearance) return;
  if (!['accent', 'background', 'foreground'].every(field => isHexColor(theme[field]))) return;
  const activeMode = effectiveThemeMode(currentSettings.appearance.themeMode);
  if (activeMode === mode) applyThemeDefinition(theme, currentSettings.appearance.themeMode);
}

async function applySelectedTheme() {
  const appearance = currentSettings?.appearance;
  if (!appearance) return;
  const mode = effectiveThemeMode(appearance.themeMode);
  const [lightTheme, darkTheme] = await Promise.all([
    invoke('get_theme', { id: appearance.lightTheme, mode: 'light' }),
    invoke('get_theme', { id: appearance.darkTheme, mode: 'dark' }),
  ]);
  themeDefinitions.light = lightTheme;
  themeDefinitions.dark = darkTheme;
  const theme = mode === 'light' ? lightTheme : darkTheme;
  applyThemeDefinition(theme, appearance.themeMode);
  applyFontSize(appearance.fontSize);
  renderThemeControls('light', lightTheme, appearance.isMacos);
  renderThemeControls('dark', darkTheme, appearance.isMacos);
}

function effectiveThemeMode(themeMode) {
  if (themeMode !== 'system') return themeMode;
  return window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

function installSystemThemeListener() {
  if (!window.matchMedia || systemThemeMediaQuery) return;
  systemThemeMediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
  const handleChange = () => {
    if (currentSettings?.appearance?.themeMode === 'system') {
      invoke('get_settings').catch(error => showError('Failed to apply system theme: ' + String(error)));
    }
  };
  if (systemThemeMediaQuery.addEventListener) systemThemeMediaQuery.addEventListener('change', handleChange);
  else systemThemeMediaQuery.addListener(handleChange);
}

function applyThemeDefinition(theme, themeMode, isMacos = currentSettings?.appearance?.isMacos) {
  const root = document.documentElement;
  root.setAttribute('data-theme-mode', themeMode === 'system' ? effectiveThemeMode(themeMode) : themeMode);
  root.setAttribute('data-theme-id', theme.id);
  root.setAttribute('data-theme-translucent-sidebar', currentSettings?.appearance?.translucentSidebar && isMacos ? 'true' : 'false');
  Object.entries(theme.variables || {}).forEach(([key, value]) => root.style.setProperty(key, value));
  root.style.setProperty('--accent', theme.accent);
  root.style.setProperty('--semantic-accent', theme.accent);
  root.style.setProperty('--bg', theme.background);
  root.style.setProperty('--fg', theme.foreground);
  root.style.setProperty('--font-ui', theme.uiFont);
  root.style.setProperty('--font-mono', theme.codeFont);
  root.style.setProperty('--sidebar-bg', theme.variables?.['--sidebar-bg'] || theme.background);
  root.style.setProperty('--titlebar-bg', theme.variables?.['--titlebar-bg'] || theme.background);
}

function clampFontSize(value) {
  const parsed = Number.parseInt(value, 10);
  if (!Number.isFinite(parsed)) return 14;
  return Math.min(30, Math.max(5, parsed));
}

function renderGeneralSettings(settings) {
  // This settings UI has no approved prototype yet; late callbacks must stay inert.
  return;
  // Thinking effort
  const thinkingSel = document.getElementById('settingsThinkingEffort');
  if (thinkingSel) {
    if (settings.thinkingEffort) thinkingSel.value = settings.thinkingEffort.toLowerCase();
    thinkingSel.onchange = () => saveSetting('thinking', thinkingSel.value);
  }

  // Auto compact
  const compactSwitch = document.getElementById('settingsAutoCompact');
  if (compactSwitch) {
    setSettingSwitch(compactSwitch, settings.auto_compact);
    wireSettingSwitch('settingsAutoCompact', enabled => saveSetting('auto_compact', String(enabled)));
  }

  wireNumberSetting('settingsCompactionTriggerRatio', settings.compaction_trigger_ratio,
    'compaction_trigger_ratio');
  wireNumberSetting('settingsCompactionTargetRatio', settings.compaction_target_ratio,
    'compaction_target_ratio');

  const namingSwitch = document.getElementById('settingsAutoSessionNaming');
  if (namingSwitch) {
    setSettingSwitch(namingSwitch, settings.auto_session_naming);
    wireSettingSwitch('settingsAutoSessionNaming', enabled =>
      saveSetting('auto_session_naming', String(enabled)));
  }

  // Steering mode
  const steerSel = document.getElementById('settingsSteeringMode');
  if (steerSel) {
    if (settings.steering_mode) steerSel.value = settings.steering_mode;
    steerSel.onchange = () => saveSetting('steering_mode', steerSel.value);
  }

  // Follow-up mode
  const followSel = document.getElementById('settingsFollowUpMode');
  if (followSel) {
    if (settings.follow_up_mode) followSel.value = settings.follow_up_mode;
    followSel.onchange = () => saveSetting('follow_up_mode', followSel.value);
  }

  const runningSendSel = document.getElementById('settingsRunningSendMode');
  if (runningSendSel) {
    if (settings.running_send_mode) runningSendSel.value = settings.running_send_mode;
    runningSendSel.onchange = () => {
      const mode = document.getElementById('runningSendMode');
      if (mode) { mode.value = runningSendSel.value; mode.dataset.initialized = 'true'; }
      saveSetting('running_send_mode', runningSendSel.value);
    };
  }

  // Block images
  const blockSwitch = document.getElementById('settingsBlockImages');
  if (blockSwitch) {
    setSettingSwitch(blockSwitch, settings.block_images);
    wireSettingSwitch('settingsBlockImages', enabled => saveSetting('block_images', String(enabled)));
  }

  const hideThinkingSwitch = document.getElementById('settingsHideThinking');
  if (hideThinkingSwitch) {
    setSettingSwitch(hideThinkingSwitch, settings.hide_thinking);
    wireSettingSwitch('settingsHideThinking', enabled => {
      document.body.classList.toggle('hide-thinking', enabled);
      return saveSetting('hide_thinking', String(enabled));
    });
  }

  // Transport
  const transportSel = document.getElementById('settingsTransport');
  if (transportSel) {
    if (settings.transport) transportSel.value = settings.transport;
    transportSel.onchange = () => saveSetting('transport', transportSel.value);
  }

  wireOptionalNumberSetting('settingsRetryTimeout', settings.retry_timeout_ms, 'retry_timeout_ms');
  wireOptionalNumberSetting('settingsRetryMax', settings.retry_max_retries, 'retry_max_retries');
  wireOptionalNumberSetting('settingsRetryDelay', settings.retry_max_delay_ms, 'retry_max_delay_ms');

}

function wireNumberSetting(id, value, key) {
  const input = document.getElementById(id);
  if (!input) return;
  input.value = String(value);
  input.onchange = () => saveSetting(key, input.value);
}

function wireOptionalNumberSetting(id, value, key) {
  const input = document.getElementById(id);
  if (!input) return;
  input.value = value == null ? '' : String(value);
  input.onchange = () => saveSetting(key, input.value);
}

function wireLinesSetting(id, values, key) {
  const input = document.getElementById(id);
  if (!input) return;
  input.value = Array.isArray(values) ? values.join('\n') : '';
  input.onchange = () => saveSetting(key, input.value);
}

async function saveSetting(key, value) {
  try {
    await invoke('update_setting', { key: key, value: value });
    await loadSettings();
    return true;
  } catch (e) {
    console.warn('update_setting failed:', e);
    showError('Failed to save setting: ' + key);
    return false;
  }
}

// =============== Input Handling ===============

