"use strict";

// Sidebar behavior for the single renderer window.
// Structure: initialization, revisioned scene switching, session/status rendering.
// It owns only MainSidebar and SettingsSidebar; stateful chat and settings-form
// roots remain in the main WebView.
// Design: ../../../.dev-doc/main/SPEC.md#2-两个持久-webview
let sidebarInvoke;
let sidebarListen;
let sidebarEmit;
let sidebarSessions = [];
let sidebarActiveSessionId = null;
let sidebarDevFlow = null;
let sidebarLastDevFlowLayoutKey = '';
let sidebarDevFlowRowHeight = 0;
let sidebarDevFlowMeasureProbe = null;
let sidebarDevFlowResizePending = false;
const SIDEBAR_SESSIONS_MIN_PX = 96;
const DEV_FLOW_HOVER_DELAY_MS = 180;
let sidebarDevFlowHoverTimer = null;
let sidebarDevFlowHoverElement = null;
let sidebarDevFlowDetailPinned = false;
const sidebarDevFlowResizeObserver = new ResizeObserver(() => {
  invalidateDevFlowRowMetrics();
});
const sidebarSceneState = { revision: 0, scene: 'main', selectedPane: null };
const sidebarThemeState = { revision: 0 };

window.addEventListener('DOMContentLoaded', async () => {
  try {
    await window.RozsaUIReady;
  } catch (error) {
    renderSidebarError('Shared Settings UI failed to load: ' + String(error));
    return;
  }
  if (!window.RozsaUIHost) {
    renderSidebarError('Electron UI host not loaded');
    return;
  }

  sidebarInvoke = window.RozsaUIHost.port.request;
  sidebarListen = (event, receive) => window.RozsaUIHost.port.subscribe(event, payload => receive({ payload }));
  sidebarEmit = window.RozsaSidebarEmit;
  await sidebarListen('gui-scene-snapshot', event => applySidebarSceneSnapshot(event.payload));
  await sidebarListen('sidebar-state', event => renderSidebarState(event.payload));
  await sidebarListen('theme-state', event => applySidebarThemeState(event.payload));
  await sidebarListen('native-fullscreen', event => {
    document.body.classList.toggle('native-fullscreen', Boolean(event.payload?.fullscreen));
  });
  setupDevFlowResponsiveLayout();

  try {
    const snapshot = await sidebarInvoke('gui_webview_ready', {
      webview: 'sidebar',
      lastRevision: sidebarSceneState.revision,
    });
    applySidebarSceneSnapshot(snapshot);
  } catch (error) {
    renderSidebarError(String(error));
  }
});

function applySidebarSceneSnapshot(snapshot) {
  return window.RozsaGuiShared.applySceneSnapshot(sidebarSceneState, snapshot, renderSidebarScene);
}

function renderSidebarScene(snapshot) {
  const settingsVisible = snapshot.scene === 'settings';
  window.RozsaGuiShared.setSceneRootVisible(document.getElementById('mainSidebarScene'), !settingsVisible);
  window.RozsaGuiShared.setSceneRootVisible(document.getElementById('settingsSidebarScene'), settingsVisible);
  document.querySelectorAll('[data-settings-pane]').forEach(button => {
    button.classList.toggle('active', button.dataset.settingsPane === snapshot.selectedPane);
  });
}

async function requestSidebarScene(scene, selectedPane = null, allowRetry = true) {
  if (scene === 'settings' && !window.RozsaSettings.isSettingsPaneAvailable(selectedPane)) return null;
  const expectedRevision = sidebarSceneState.revision;
  const snapshot = await sidebarInvoke('set_gui_scene', { scene, selectedPane, expectedRevision });
  applySidebarSceneSnapshot(snapshot);
  const desiredPane = scene === 'settings' ? selectedPane : null;
  if (allowRetry && snapshot.revision !== expectedRevision &&
      (sidebarSceneState.scene !== scene || sidebarSceneState.selectedPane !== desiredPane)) {
    return requestSidebarScene(scene, selectedPane, false);
  }
  return snapshot;
}

function openSidebarSettings() {
  void requestSidebarScene('settings', 'appearance').catch(renderSidebarError);
}

function closeSidebarSettings() {
  void requestSidebarScene('main').catch(renderSidebarError);
}

function selectSidebarSettingsPane(pane) {
  void requestSidebarScene('settings', pane).catch(renderSidebarError);
}

function renderSidebarSessions() {
  const container = document.getElementById('sessionList');
  if (!container) return;
  container.innerHTML = window.RozsaSidebar.sessionsMarkup(sidebarSessions.map(session => ({
    name: session.name || 'Untitled', date: formatSidebarSessionDate(session.modified),
    activity: session.activity || 'idle', active: session.id === sidebarActiveSessionId,
  })));
}

function renderSidebarState(snapshot) {
  if (!snapshot) return;
  sidebarSessions = Array.isArray(snapshot.sessions) ? snapshot.sessions : [];
  sidebarActiveSessionId = snapshot.activeSessionId || null;
  if (snapshot.git) {
    setSidebarText('gitBranch', snapshot.git.label || snapshot.git.projectName || '—');
    setSidebarText('gitAdd', '+' + Number(snapshot.git.added || 0));
    setSidebarText('gitDel', '-' + Number(snapshot.git.deleted || 0));
    setSidebarText('gitFiles', Number(snapshot.git.files || 0) + ' files');
  }
  renderSidebarQuota(snapshot);
  renderSidebarDevFlow(snapshot);
  renderSidebarSessions();
}

function renderSidebarQuota(snapshot) {
  const group = document.getElementById('quotaGroup');
  if (!group) return;
  const showQuota = Boolean(snapshot.showQuota);
  group.hidden = !showQuota;
  group.style.display = showQuota ? '' : 'none';
  if (!showQuota) return;
  const hourRow = document.getElementById('quotaHourRow');
  if (hourRow) hourRow.hidden = !snapshot.showHourlyQuota;
  renderSidebarQuotaWindow('quotaHourBar', 'quotaHour', snapshot.quota?.primary, snapshot.rateLimitDisplayMode);
  const weekRow = document.getElementById('quotaWeekRow');
  if (weekRow) weekRow.hidden = !snapshot.showWeeklyQuota;
  renderSidebarQuotaWindow('quotaWeekBar', 'quotaWeek', snapshot.quota?.secondary, snapshot.rateLimitDisplayMode);
}

function renderSidebarQuotaWindow(barId, valueId, window, mode) {
  const bar = document.getElementById(barId);
  const value = document.getElementById(valueId);
  if (!bar || !value) return;
  const used = Math.min(100, Math.max(0, Number(window?.usedPercent || 0)));
  const display = mode === 'used' ? used : 100 - used;
  bar.style.width = window ? display + '%' : '0%';
  bar.classList.remove('warn');
  value.textContent = window ? Math.round(display) + '%' : '—';
}

function renderSidebarDevFlow(snapshot) {
  const df = snapshot && snapshot.devFlow ? snapshot.devFlow : null;
  const showDashboard = Boolean(snapshot && snapshot.showDevFlowDashboard);
  sidebarDevFlow = df;
  sidebarLastDevFlowLayoutKey = '';
  const group = document.getElementById('devFlowSidebarGroup');
  if (!group) return;
  const showSummary = Boolean(df) && df.availability === 'ready' && df.showSidebarStatus;
  group.hidden = !showSummary;
  if (!showSummary) {
    updateDevFlowDashboardButton(df, showDashboard);
    return;
  }
  const summary = document.getElementById('devFlowSidebarSummary');
  if (summary) {
    summary.textContent = devFlowCountLabel(df.openTasks, 'Task') + ' · ' + devFlowCountLabel(df.openIssues, 'Issue');
    summary.classList.toggle('stale', Boolean(df.stale));
    wireDevFlowDetailTrigger(summary, { kind: 'summary' });
  }
  renderDevFlowClaimedRows();
  updateDevFlowDashboardButton(df, showDashboard);
}

function devFlowCountLabel(count, noun) {
  return count === 1 ? '1 ' + noun : count + ' ' + noun + 's';
}

function renderDevFlowClaimedRows() {
  const container = document.getElementById('devFlowSidebarClaimed');
  const more = document.getElementById('devFlowSidebarMore');
  if (!container || !sidebarDevFlow) return;
  const rows = Array.isArray(sidebarDevFlow.claimed) ? sidebarDevFlow.claimed : [];
  const layout = computeDevFlowRowLayout(rows.length);
  const layoutKey = layout.visible + ':' + layout.hidden + ':' + layout.showMore;
  if (layoutKey === sidebarLastDevFlowLayoutKey) return;
  sidebarLastDevFlowLayoutKey = layoutKey;
  const visible = rows.slice(0, layout.visible);
  container.innerHTML = visible.map(item => window.RozsaSidebar.claimedMarkup({ ...item, id: item.shortId || item.id })).join('');
  container.querySelectorAll('.dev-flow-claimed-row').forEach((row, index) => {
    wireDevFlowDetailTrigger(row, { kind: 'item', id: visible[index].id });
  });
  if (more) {
    more.hidden = !layout.showMore;
    if (layout.showMore) {
      more.textContent = 'more ' + layout.hidden;
      wireDevFlowDetailTrigger(more, { kind: 'more' });
    }
  }
}

function wireDevFlowDetailTrigger(element, target) {
  if (!element) return;
  element.onpointerenter = () => {
    clearTimeout(sidebarDevFlowHoverTimer);
    sidebarDevFlowDetailPinned = false;
    sidebarDevFlowHoverTimer = setTimeout(async () => {
      sidebarDevFlowHoverTimer = null;
      sidebarDevFlowHoverElement = element;
      await openDevFlowDetail(target);
      if (sidebarDevFlowHoverElement !== element && !sidebarDevFlowDetailPinned) {
        dismissDevFlowHoverDetail();
      }
    }, DEV_FLOW_HOVER_DELAY_MS);
  };
  element.onpointerleave = () => {
    clearTimeout(sidebarDevFlowHoverTimer);
    sidebarDevFlowHoverTimer = null;
    if (sidebarDevFlowHoverElement === element) {
      sidebarDevFlowHoverElement = null;
      if (!sidebarDevFlowDetailPinned) dismissDevFlowHoverDetail();
    }
  };
  element.onclick = () => {
    clearTimeout(sidebarDevFlowHoverTimer);
    sidebarDevFlowHoverTimer = null;
    const alreadyVisible = sidebarDevFlowHoverElement === element;
    sidebarDevFlowDetailPinned = true;
    sidebarDevFlowHoverElement = null;
    if (!alreadyVisible) void openDevFlowDetail(target);
  };
}

function dismissDevFlowHoverDetail() {
  if (sidebarEmit) void sidebarEmit('dev-flow-detail-dismiss');
}

function computeDevFlowRowLayout(totalRows) {
  const rowHeight = measureDevFlowRowHeight();
  if (rowHeight <= 0) return { visible: 0, hidden: totalRows, showMore: false };
  const statusSection = document.querySelector('[data-od-id="sidebar-status"]');
  if (!statusSection) return { visible: 0, hidden: totalRows, showMore: false };
  const top = document.body.classList.contains('native-fullscreen') ? 0 : 24;
  const bottom = document.querySelector('.sidebar-bottom')?.getBoundingClientRect().height || 0;
  let base = statusSection.getBoundingClientRect().height;
  const claimed = document.getElementById('devFlowSidebarClaimed');
  const more = document.getElementById('devFlowSidebarMore');
  if (claimed) base -= claimed.getBoundingClientRect().height;
  if (more && !more.hidden) base -= more.getBoundingClientRect().height;
  const budget = window.innerHeight - top - base - bottom - SIDEBAR_SESSIONS_MIN_PX;
  return calculateDevFlowRowLayout(budget, rowHeight, totalRows);
}

function calculateDevFlowRowLayout(budget, rowHeight, totalRows) {
  const slots = Math.max(0, Math.floor(budget / rowHeight));
  if (totalRows <= slots) {
    return { visible: totalRows, hidden: 0, showMore: false };
  }
  if (slots === 0) {
    return { visible: 0, hidden: totalRows, showMore: false };
  }
  const visible = Math.min(totalRows, slots - 1);
  return { visible, hidden: totalRows - visible, showMore: true };
}

function measureDevFlowRowHeight() {
  if (sidebarDevFlowRowHeight > 0) return sidebarDevFlowRowHeight;
  const probe = ensureDevFlowMeasureProbe();
  sidebarDevFlowRowHeight = probe.getBoundingClientRect().height;
  return sidebarDevFlowRowHeight;
}

function ensureDevFlowMeasureProbe() {
  if (sidebarDevFlowMeasureProbe) return sidebarDevFlowMeasureProbe;
  const template = document.createElement('template');
  template.innerHTML = window.RozsaSidebar.claimedMarkup({ id: 'T001', title: 'Measure' });
  const probe = template.content.firstElementChild;
  probe.style.cssText = 'position:absolute;visibility:hidden;pointer-events:none;left:-10000px;top:0;';
  document.getElementById('mainSidebarScene')?.appendChild(probe);
  sidebarDevFlowMeasureProbe = probe;
  sidebarDevFlowResizeObserver.observe(probe);
  return probe;
}

function invalidateDevFlowRowMetrics() {
  sidebarDevFlowRowHeight = 0;
  sidebarLastDevFlowLayoutKey = '';
  if (sidebarDevFlowResizePending) return;
  sidebarDevFlowResizePending = true;
  requestAnimationFrame(() => {
    sidebarDevFlowResizePending = false;
    if (sidebarDevFlow && sidebarDevFlow.availability === 'ready') {
      renderDevFlowClaimedRows();
    }
  });
}

function setupDevFlowResponsiveLayout() {
  const main = document.getElementById('mainSidebarScene');
  const bottom = document.querySelector('.sidebar-bottom');
  if (main) sidebarDevFlowResizeObserver.observe(main);
  if (bottom) sidebarDevFlowResizeObserver.observe(bottom);
  ensureDevFlowMeasureProbe();
  document.fonts?.addEventListener('loadingdone', invalidateDevFlowRowMetrics);
}

async function openDevFlowDetail(target) {
  const df = sidebarDevFlow;
  if (!df || df.availability !== 'ready') return;
  try {
    await sidebarInvoke('dev_flow_detail', {
      request: {
        projectKey: df.project.projectKey,
        revision: df.revision,
        target,
      },
    });
  } catch (error) {
    // Stale or cross-project request: the fresh sidebar state is already in
    // flight, so the main view simply never renders this detail.
    console.debug('[rozsa-gui][dev-flow] detail request rejected:', String(error));
  }
}

function updateDevFlowDashboardButton(df, visible = true) {
  const button = document.getElementById('sidebarDevFlowDashboard');
  if (!button) return;
  button.hidden = !visible;
  const enabled = Boolean(df) && df.availability === 'ready';
  button.disabled = !enabled;
  button.title = !df
    ? 'Dev-flow is disabled'
    : enabled
      ? 'Open dev-flow dashboard'
      : (df.availabilityMessage || 'Dev-flow dashboard unavailable');
}

async function openDevFlowDashboard() {
  try {
    await sidebarInvoke('open_dev_flow_dashboard');
  } catch (error) {
    // Failures surface through the main-view notification center as
    // resolvable errors; the button remains available for retry.
    console.debug('[rozsa-gui][dev-flow] dashboard open failed:', String(error));
  }
}

function applySidebarThemeState(snapshot) {
  return window.RozsaGuiShared.applyThemeSnapshot(sidebarThemeState, snapshot, renderSidebarTheme);
}

function renderSidebarTheme(snapshot) {
  window.RozsaGuiShared.applyThemeTokens(document.documentElement, snapshot);
  invalidateDevFlowRowMetrics();
}

function setSidebarText(id, text) {
  const element = document.getElementById(id);
  if (element) element.textContent = text;
}

async function switchSidebarSession(path) {
  try {
    await sidebarInvoke('switch_session', { path });
  } catch (error) {
    renderSidebarError(String(error));
  }
}

async function newSidebarSession() {
  try {
    await sidebarInvoke('new_session');
  } catch (error) {
    renderSidebarError(String(error));
  }
}

function renderSidebarError(error) {
  if (window.RozsaSidebarFeedback) {
    void window.RozsaSidebarFeedback(String(error)).catch(deliveryError => {
      renderSidebarBootstrapError(String(error) + '\nFailed to deliver notification: ' + String(deliveryError));
    });
    return;
  }
  renderSidebarBootstrapError(error);
}

function renderSidebarBootstrapError(error) {
  const container = document.getElementById('sessionList');
  if (!container) return;
  const item = document.createElement('div');
  item.className = 'error';
  item.textContent = String(error);
  container.replaceChildren(item);
}

function formatSidebarSessionDate(value) {
  if (!value) return '';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return '';
  const days = Math.floor((Date.now() - date.getTime()) / 86400000);
  if (days < 1) return '';
  if (days < 7) return days + 'd';
  if (days < 35) return Math.floor(days / 7) + 'w';
  return Math.floor(days / 30) + 'm';
}
