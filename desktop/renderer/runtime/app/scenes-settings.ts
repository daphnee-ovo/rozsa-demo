// @ts-nocheck
function isSidebarCollapsed() {
  return sidebarCollapsed || sidebarAutoCollapsed;
}

function preparePlatformSceneDom() {
  document.body.classList.toggle('native-split-main', nativeSplitMode);
  if (nativeSplitMode) {
    document.getElementById('nativeSidebarEdgeTrigger')
      ?.addEventListener('pointerenter', showNativeSidebarOverlay);
    return;
  }
  materializeFallbackTemplate('fallbackSidebarTemplate', 'fallbackSidebarMount');
  window.RozsaSidebar.suspendQuotaTooltip();
  window.RozsaSidebar.init({ boundModules: new Set(), byId: id => document.getElementById(id),
    activateSession: doSwitchSession, newSession, toggleSettings,
    openDevFlowDashboard: () => invoke('open_dev_flow_dashboard').catch(showError),
  });
  materializeFallbackTemplate('fallbackSettingsNavigationTemplate', 'fallbackSettingsNavigationMount');
}

function materializeFallbackTemplate(templateId, mountId) {
  const template = document.getElementById(templateId);
  const mount = document.getElementById(mountId);
  if (!template || !mount) return;
  mount.replaceWith(template.content.cloneNode(true));
}

function applyGuiSceneSnapshot(snapshot) {
  if (!nativeSplitMode) return false;
  if (isInputComposing && snapshot?.scene !== guiSceneState.scene) {
    if (!pendingGuiSceneSnapshot || snapshot.revision > pendingGuiSceneSnapshot.revision) {
      pendingGuiSceneSnapshot = snapshot;
    }
    return false;
  }
  return window.RozsaGuiShared.applySceneSnapshot(guiSceneState, snapshot, renderNativeMainScene);
}

function applyMainThemeState(snapshot) {
  return window.RozsaGuiShared.applyThemeSnapshot(mainThemeState, snapshot, renderMainThemeState);
}

function renderMainThemeState(snapshot) {
  themeDefinitions.light = snapshot.lightTheme;
  themeDefinitions.dark = snapshot.darkTheme;
  const theme = window.RozsaGuiShared.resolveTheme(snapshot);
  applyThemeDefinition(theme, snapshot.themeMode, snapshot.isMacos);
  applyFontSize(snapshot.fontSize);
}

function renderNativeMainScene(snapshot) {
  const settingsVisible = snapshot.scene === 'settings';
  const leavingMain = settingsVisible && guiSceneState.scene === 'main';
  const returningMain = !settingsVisible && guiSceneState.scene === 'settings';
  if (leavingMain) captureMainSceneContinuity();
  window.RozsaGuiShared.setSceneRootVisible(
    document.getElementById('mainContentScene'),
    !settingsVisible,
  );
  window.RozsaGuiShared.setSceneRootVisible(
    document.getElementById('settingsPanel'),
    settingsVisible,
  );
  document.body.classList.toggle('settings-visible', settingsVisible);
  document.getElementById('settingsPanel')?.classList.toggle('visible', settingsVisible);
  if (settingsVisible) {
    renderSettingsSelection(snapshot.selectedPane || 'appearance');
    loadSettings().catch(error => showError('Could not load settings: ' + String(error)));
    loadKeyBindings().catch(error => showError('Could not load key bindings: ' + String(error)));
  } else if (returningMain) {
    requestAnimationFrame(restoreMainSceneContinuity);
  }
}

async function requestGuiScene(scene, selectedPane = null, allowRetry = true) {
  if (scene === 'settings' && !window.RozsaSettings.isSettingsPaneAvailable(selectedPane)) return null;
  if (isInputComposing && scene !== guiSceneState.scene) {
    pendingGuiSceneIntent = { scene, selectedPane };
    return null;
  }
  const expectedRevision = guiSceneState.revision;
  const snapshot = await invoke('set_gui_scene', {
    scene,
    selectedPane,
    expectedRevision,
  });
  applyGuiSceneSnapshot(snapshot);
  if (isInputComposing) return snapshot;
  const desiredPane = scene === 'settings' ? selectedPane : null;
  if (allowRetry && snapshot.revision !== expectedRevision &&
      (guiSceneState.scene !== scene || guiSceneState.selectedPane !== desiredPane)) {
    return requestGuiScene(scene, selectedPane, false);
  }
  return snapshot;
}

function captureMainSceneContinuity() {
  const chat = document.getElementById('chatMessages');
  const input = document.getElementById('msgInput');
  const mainRoot = document.getElementById('mainContentScene');
  if (activeSessionId) captureSessionDraft(activeSessionId);
  if (activeSessionId && chat) persistSessionViewState(activeSessionId, chat);
  if (activeSessionId) capturePermissionUiState(activeSessionId);
  mainSceneContinuity = {
    activeSessionId,
    focusOwner: mainRoot?.contains(document.activeElement) ? document.activeElement : null,
    inputSelection: getInputSelection(input),
  };
}

function restoreMainSceneContinuity() {
  const memory = mainSceneContinuity;
  const input = document.getElementById('msgInput');
  const focusOwner = memory?.focusOwner;
  if (focusOwner?.isConnected && typeof focusOwner.focus === 'function') {
    focusOwner.focus({ preventScroll: true });
    if (focusOwner === input && memory.inputSelection) {
      setInputSelection(input, memory.inputSelection.start, memory.inputSelection.end);
    }
    return;
  }
  if (!input) return;
  input.focus({ preventScroll: true });
  const fallbackOffset = getInputText(input).length;
  setInputSelection(input, fallbackOffset, fallbackOffset);
}

function flushPendingGuiSceneTransition() {
  const snapshot = pendingGuiSceneSnapshot;
  const intent = pendingGuiSceneIntent;
  pendingGuiSceneSnapshot = null;
  pendingGuiSceneIntent = null;
  if (snapshot) applyGuiSceneSnapshot(snapshot);
  if (intent && (guiSceneState.scene !== intent.scene ||
      guiSceneState.selectedPane !== (intent.scene === 'settings' ? intent.selectedPane : null))) {
    void requestGuiScene(intent.scene, intent.selectedPane)
      .catch(error => showError('Failed to switch GUI scene: ' + String(error)));
  }
}

function updateSidebarToggleButtons(collapsed) {
  document.querySelectorAll('.sidebar-toggle-button').forEach(button => {
    button.setAttribute('aria-pressed', String(!collapsed));
    const label = collapsed ? 'Show sidebar' : 'Hide sidebar';
    button.setAttribute('aria-label', label);
    button.title = label;
  });
}

function updateSidebarLayout(collapsed) {
  const appBody = document.querySelector('[data-od-id="app-body"]');
  const settingsPanel = document.getElementById('settingsPanel');
  document.body.classList.toggle('sidebar-collapsed', collapsed);
  appBody?.classList.toggle('sidebar-collapsed', collapsed);
  settingsPanel?.classList.toggle('settings-sidebar-collapsed', collapsed);
  if (!collapsed) {
    appBody?.classList.remove('sidebar-edge-visible');
    settingsPanel?.classList.remove('settings-edge-visible');
  }
  updateSidebarToggleButtons(collapsed);
  syncChromeBackgroundGeometry();
  window.requestAnimationFrame(syncChromeBackgroundGeometry);
}

function setMainSidebarCollapsed(collapsed, fromUser = true) {
  sidebarCollapsed = collapsed;
  if (fromUser && !collapsed) sidebarAutoCollapsed = false;
  updateSidebarLayout(isSidebarCollapsed());
}

function toggleMainSidebar() {
  setMainSidebarCollapsed(!isSidebarCollapsed());
}

function syncMainSidebarViewport() {
  if (nativeSplitMode) return;
  const shouldAutoCollapse = sidebarAutoCollapsed
    ? window.innerWidth < SIDEBAR_AUTO_EXPAND_MIN_WIDTH
    : window.innerWidth <= SIDEBAR_AUTO_COLLAPSE_MAX_WIDTH;
  if (shouldAutoCollapse !== sidebarAutoCollapsed) {
    sidebarAutoCollapsed = shouldAutoCollapse;
    updateSidebarLayout(isSidebarCollapsed());
  } else {
    updateSidebarToggleButtons(isSidebarCollapsed());
    syncChromeBackgroundGeometry();
  }
}

function sidebarChromeBoundary(element, hidden) {
  if (!element || hidden) return 0;
  // getBoundingClientRect() is in the post-zoom viewport coordinate space,
  // while gradients and CSS variables are resolved in layout CSS pixels.
  // offsetLeft/offsetWidth stay in that layout space and follow the actual
  // sidebar width after the responsive grid has resolved.
  return Math.max(0, element.offsetLeft + element.offsetWidth);
}

function syncChromeBackgroundGeometry() {
  if (nativeSplitMode) return;
  const root = document.documentElement;
  const appBody = document.querySelector('[data-od-id="app-body"]');
  const settingsPanel = document.getElementById('settingsPanel');
  const collapsed = isSidebarCollapsed();
  const mainHidden = collapsed && !appBody?.classList.contains('sidebar-edge-visible');
  const settingsHidden = collapsed && !settingsPanel?.classList.contains('settings-edge-visible');
  root.style.setProperty(
    '--chrome-sidebar-boundary',
    `${sidebarChromeBoundary(document.querySelector('[data-od-id="sidebar"]'), mainHidden)}px`
  );
  root.style.setProperty(
    '--settings-chrome-sidebar-boundary',
    `${sidebarChromeBoundary(document.querySelector('.settings-tabs'), settingsHidden)}px`
  );
}

function nativeSidebarEdgeTriggerWidth() {
  const trigger = document.getElementById('nativeSidebarEdgeTrigger');
  const width = trigger?.getBoundingClientRect().width || 0;
  return width > 0 ? width : 18;
}

function handleSidebarEdgeReveal(event) {
  if (nativeSplitMode) {
    nativePointerClientX = event.clientX;
    // Reparenting the sidebar WebView can send one more pointermove through
    // this WebView. The edge trigger is only a reveal affordance; once the
    // overlay is visible, keep it alive through its full native width so the
    // sidebar remains usable.
    if (
      (nativeSidebarOverlayVisible || nativeSidebarOverlayRevealInFlight) &&
      nativeSidebarOverlayWidth > 0 &&
      event.clientX > nativeSidebarOverlayWidth
    ) {
      hideNativeSidebarOverlay();
    } else if (
      isSidebarCollapsed() &&
      !nativeSidebarOverlayVisible &&
      !nativeSidebarOverlayRevealInFlight &&
      event.clientX <= nativeSidebarEdgeTriggerWidth()
    ) {
      // This is the scene-independent fallback for WebKit/AppKit paths that
      // deliver pointermove without a pointerenter on the empty edge node.
      showNativeSidebarOverlay(event);
    }
    return;
  }
  const collapsed = isSidebarCollapsed();
  const settingsPanel = document.getElementById('settingsPanel');
  const settingsVisible = settingsPanel?.classList.contains('visible');
  const panel = settingsVisible ? settingsPanel : document.querySelector('[data-od-id="app-body"]');
  const sidebar = settingsVisible
    ? settingsPanel?.querySelector('.settings-tabs')
    : panel?.querySelector('[data-od-id="sidebar"]');
  const sidebarWidth = sidebar?.getBoundingClientRect().width || 260;
  const visibleClass = settingsVisible ? 'settings-edge-visible' : 'sidebar-edge-visible';
  const edgeVisible = event.clientX <= 18 || (
    panel?.classList.contains(visibleClass) && event.clientX <= sidebarWidth + 12
  );
  setSidebarEdgeVisible(collapsed && edgeVisible);
}

function showNativeSidebarOverlay(event) {
  if (
    !nativeSplitMode ||
    !isSidebarCollapsed() ||
    nativeSidebarOverlayVisible ||
    nativeSidebarOverlayRevealInFlight
  ) return;
  const request = ++nativeSidebarOverlayRequest;
  nativeSidebarOverlayRevealInFlight = true;
  const revealPointerX = event?.clientX ?? nativePointerClientX ?? 0;
  invoke('native_sidebar_overlay_width')
    .then(width => {
      if (request !== nativeSidebarOverlayRequest) return null;
      nativeSidebarOverlayRevealInFlight = false;
      const normalizedWidth = Number(width);
      if (!Number.isFinite(normalizedWidth) || normalizedWidth <= 0) {
        throw new Error('native sidebar overlay width is invalid: ' + String(width));
      }
      nativeSidebarOverlayWidth = normalizedWidth;
      const currentPointerX = nativePointerClientX ?? revealPointerX;
      if (currentPointerX > normalizedWidth) return null;
      nativeSidebarOverlayVisible = true;
      return invoke('set_native_sidebar_overlay_visible', { visible: true });
    })
    .catch(error => {
      if (request === nativeSidebarOverlayRequest) {
        nativeSidebarOverlayRevealInFlight = false;
        nativeSidebarOverlayVisible = false;
        showError('Failed to reveal sidebar: ' + String(error));
      }
    });
}

function hideNativeSidebarOverlay() {
  nativeSidebarOverlayRequest += 1;
  nativeSidebarOverlayRevealInFlight = false;
  if (!nativeSidebarOverlayVisible) return;
  nativeSidebarOverlayVisible = false;
  invoke('set_native_sidebar_overlay_visible', { visible: false })
    .catch(error => showError('Failed to hide sidebar: ' + String(error)));
}

function setSidebarEdgeVisible(visible) {
  const settingsPanel = document.getElementById('settingsPanel');
  const settingsVisible = settingsPanel?.classList.contains('visible');
  settingsPanel?.classList.toggle('settings-edge-visible', settingsVisible && visible);
  document.querySelector('[data-od-id="app-body"]')
    ?.classList.toggle('sidebar-edge-visible', !settingsVisible && visible);
  syncChromeBackgroundGeometry();
}

async function syncNativeFullscreen(source) {
  if (nativeFullscreenTransitioning) {
    console.debug('[rozsa-gui][fullscreen] calibration suppressed during transition', source);
    return;
  }
}

function scheduleNativeFullscreenSync(source = 'scheduled') {
  [0, 80, 240].forEach(delay => {
    window.setTimeout(() => {
      syncNativeFullscreen(`${source}:${delay}`).catch(error => {
        console.error('[rozsa-gui][fullscreen] calibration failed', source, error);
      });
    }, delay);
  });
}

function setNativeFullscreen(fullscreen) {
  document.body.classList.toggle('native-fullscreen', fullscreen);
  const appBodyRect = document.querySelector('[data-od-id="app-body"]')?.getBoundingClientRect();
  const settingsDialogRect = document.querySelector('.settings-dialog')?.getBoundingClientRect();
  console.debug(
    '[rozsa-gui][fullscreen] class applied',
    document.body.classList.contains('native-fullscreen'),
    {
      body: document.body.getBoundingClientRect().toJSON(),
      appBody: appBodyRect?.toJSON(),
      settingsDialog: settingsDialogRect?.toJSON(),
    },
  );
  syncChromeBackgroundGeometry();
  window.requestAnimationFrame(syncChromeBackgroundGeometry);
}

function toggleSettings() {
  if (nativeSplitMode) {
    const scene = guiSceneState.scene === 'settings' ? 'main' : 'settings';
    void requestGuiScene(scene, scene === 'settings' ? 'appearance' : null)
      .catch(error => showError('Failed to switch GUI scene: ' + String(error)));
    return;
  }
  const panel = document.getElementById('settingsPanel');
  if (!panel) return;
  if (panel.classList.contains('visible')) {
    closeSettings();
  } else {
    document.body.classList.add('settings-visible');
    panel.classList.add('visible');
    panel.classList.toggle('settings-sidebar-collapsed', isSidebarCollapsed());
    syncChromeBackgroundGeometry();
    loadSettings().catch(error => showError('Could not load settings: ' + String(error)));
    loadKeyBindings().catch(error => showError('Could not load key bindings: ' + String(error)));
  }
}

function closeSettings() {
  if (nativeSplitMode) {
    void requestGuiScene('main').catch(error => showError('Failed to close Settings: ' + String(error)));
    return;
  }
  const panel = document.getElementById('settingsPanel');
  document.body.classList.remove('settings-visible');
  if (panel) panel.classList.remove('visible');
  syncChromeBackgroundGeometry();
}

function switchSettingsTab(tabId, btn) {
  if (!window.RozsaSettings.isSettingsPaneAvailable(tabId)) return;
  if (nativeSplitMode) {
    void requestGuiScene('settings', tabId)
      .catch(error => showError('Failed to switch Settings pane: ' + String(error)));
    return;
  }
  renderSettingsSelection(tabId, btn);
}

function renderSettingsSelection(tabId, btn = null) {
  if (!window.RozsaSettings.isSettingsPaneAvailable(tabId)) {
    document.querySelectorAll('.settings-pane, .settings-tab').forEach(element => element.classList.remove('active'));
    return;
  }
  document.querySelectorAll('.settings-tab').forEach(t => t.classList.remove('active'));
  document.querySelectorAll('.settings-pane').forEach(p => p.classList.remove('active'));
  const selectedButton = btn || document.querySelector(`[data-settings-pane="${tabId}"]`);
  if (selectedButton) selectedButton.classList.add('active');
  const pane = document.getElementById('pane-' + tabId);
  if (pane) pane.classList.add('active');
}

function isSettingSwitchOn(control) {
  return control?.getAttribute('aria-checked') === 'true';
}

function setSettingSwitch(control, checked) {
  if (!control) return;
  const enabled = Boolean(checked);
  control.classList.toggle('on', enabled);
  control.setAttribute('aria-checked', String(enabled));
}

function wireSettingSwitch(id, onChange) {
  const control = document.getElementById(id);
  if (!control) return;
  window.RozsaSettings.bindSettingSwitch(control, onChange, isSettingSwitchOn, setSettingSwitch);
}

async function loadSettings() {
  try {
    [currentSettings, availableThemes] = await Promise.all([
      invoke('get_settings'),
      invoke('list_themes'),
    ]);
    renderSettingsPane(currentSettings);
    loadDevFlowSettings();
    await applySelectedTheme();
  } catch (e) {
    console.warn('settings:', e);
    currentSettings = {};
    availableThemes = [];
    capabilitySettings = null;
    permissionSettings = null;
    devFlowSettings = null;
    throw e;
  }
}

let devFlowSettings = null;
let devFlowSettingsRevision = 0;

function acceptDevFlowSettingsSnapshot(revision, snapshot) {
  if (revision !== devFlowSettingsRevision) return false;
  devFlowSettings = snapshot;
  renderDevFlowSettings();
  return true;
}

async function loadDevFlowSettings() {
  const revision = ++devFlowSettingsRevision;
  try {
    const snapshot = await invoke('get_dev_flow_settings');
    if (!acceptDevFlowSettingsSnapshot(revision, snapshot)) return;
    resolveNotification('dev-flow.settings-load');
  } catch (e) {
    if (revision !== devFlowSettingsRevision) return;
    console.warn('dev-flow settings:', e);
    devFlowSettings = null;
    showDevFlowSettingsError('dev-flow.settings-load', 'Could not load settings', e);
  }
  if (revision === devFlowSettingsRevision && !devFlowSettings) renderDevFlowSettings();
}

function showDevFlowSettingsError(id, title, error) {
  upsertNotification({
    id,
    severity: 'error',
    title,
    message: String(error),
    timeoutMs: NOTIFICATION_TIMEOUT_MS,
  });
}

function setDevFlowDependentControlDisabled(control, disabled) {
  if (!control) return;
  control.disabled = disabled;
  control.setAttribute('aria-disabled', String(disabled));
  const item = control.closest('.setting-item');
  if (item) item.classList.toggle('is-disabled', disabled);
}

function setDevFlowText(id, value) {
  const element = document.getElementById(id);
  if (element) element.textContent = value;
}

function formatDevFlowMemory(bytes) {
  if (!Number.isFinite(bytes)) return 'Unavailable';
  return (bytes / (1024 * 1024)).toFixed(bytes < 10 * 1024 * 1024 ? 1 : 0) + ' MiB';
}

function devFlowDependentControlsDisabled(settings) {
  return !settings.enabled || !settings.cli.available;
}

function renderDevFlowSettings() {
  window.RozsaDevFlow.renderSettings(window.RozsaDevFlow.devFlowSettingsView(devFlowSettings));
}
async function updateDevFlowSettings(command, args, optimistic, errorId, errorTitle) {
  const revision = ++devFlowSettingsRevision;
  const previous = devFlowSettings;
  if (devFlowSettings && optimistic) {
    devFlowSettings = { ...devFlowSettings, ...optimistic };
    renderDevFlowSettings();
  }
  try {
    const snapshot = await invoke(command, args || {});
    if (!acceptDevFlowSettingsSnapshot(revision, snapshot)) return;
    resolveNotification(errorId);
  } catch (e) {
    if (revision !== devFlowSettingsRevision) return;
    devFlowSettings = previous;
    renderDevFlowSettings();
    showDevFlowSettingsError(errorId, errorTitle, e);
  }
}

function wireDevFlowSettings() {
  wireSettingSwitch('devFlowEnabled', enabled => {
    void updateDevFlowSettings(
      'set_dev_flow_enabled', { enabled }, { enabled },
      'dev-flow.settings-enabled', 'Could not update integration'
    );
  });
  wireSettingSwitch('devFlowSidebarStatus', enabled => {
    void updateDevFlowSettings(
      'set_dev_flow_sidebar_status', { enabled }, { showSidebarStatus: enabled },
      'dev-flow.settings-sidebar', 'Could not update sidebar status'
    );
  });
  wireSettingSwitch('devFlowDashboardButton', enabled => {
    void updateDevFlowSettings(
      'set_dev_flow_dashboard_button', { enabled }, { showDashboardButton: enabled },
      'dev-flow.settings-dashboard', 'Could not update Dashboard button'
    );
  });
  const pickExecutable = document.getElementById('devFlowPickExecutable');
  const dashboardAddress = document.getElementById('devFlowDashboardAddress');
  const pathInput = document.getElementById('devFlowExecutablePath');
  const rescan = document.getElementById('devFlowRescan');
  if (dashboardAddress) {
    dashboardAddress.onclick = async () => {
      try {
        await invoke('open_dev_flow_dashboard');
      } catch (e) {
        showDevFlowSettingsError('dev-flow.dashboard-open', 'Could not open Dashboard', e);
      }
    };
  }
  if (pathInput) {
    pathInput.addEventListener('blur', () => {
      const path = pathInput.value.trim();
      const currentPath = devFlowSettings?.executablePath || devFlowSettings?.cli?.executable || '';
      if (path === currentPath) return;
      void updateDevFlowSettings(
        'set_dev_flow_executable_path', { path: path || null }, { executablePath: path || null },
        'dev-flow.settings-executable', 'Could not update executable'
      );
    });
  }
  if (pickExecutable) {
    pickExecutable.onclick = async () => {
      try {
        const path = await invoke('pick_dev_flow_executable');
        if (!path) return;
        await updateDevFlowSettings(
          'set_dev_flow_executable_path', { path }, { executablePath: path },
          'dev-flow.settings-executable', 'Could not update executable'
        );
      } catch (e) {
        showDevFlowSettingsError('dev-flow.settings-picker', 'Could not choose executable', e);
      }
    };
  }
  if (rescan) {
    rescan.onclick = () => void updateDevFlowSettings(
      'rescan_dev_flow', {}, null,
      'dev-flow.settings-rescan', 'Could not check for Dev Flow'
    );
  }
}

