"use strict";

// Persistent sidebar WebView.
// Structure: initialization, revisioned scene switching, session/status rendering.
// It owns only MainSidebar and SettingsSidebar; stateful chat and settings-form
// roots remain in the main WebView.
// Design: ../../../.dev-doc/main/SPEC.md#2-两个持久-webview
let sidebarInvoke;
let sidebarListen;
let sidebarSessions = [];
let sidebarActiveSessionId = null;
let sidebarDevFlow = null;
let sidebarLastDevFlowRowLimit = -1;
const SIDEBAR_SESSIONS_MIN_PX = 96;
const sidebarDevFlowResizeObserver = new ResizeObserver(() => {
  if (sidebarDevFlow && sidebarDevFlow.availability === 'ready') {
    renderDevFlowClaimedRows();
  }
});
const sidebarSceneState = { revision: 0, scene: 'main', selectedPane: null };
const sidebarThemeState = { revision: 0 };

window.addEventListener('DOMContentLoaded', async () => {
  let retries = 0;
  while (!window.__TAURI__ && retries < 30) {
    await new Promise(resolve => setTimeout(resolve, 100));
    retries++;
  }
  if (!window.__TAURI__) {
    renderSidebarError('Tauri API not loaded');
    return;
  }

  sidebarInvoke = window.__TAURI__.core.invoke;
  sidebarListen = window.__TAURI__.event.listen;
  await sidebarListen('gui-scene-snapshot', event => applySidebarSceneSnapshot(event.payload));
  await sidebarListen('sidebar-state', event => renderSidebarState(event.payload));
  await sidebarListen('theme-state', event => applySidebarThemeState(event.payload));
  await sidebarListen('native-fullscreen', event => {
    document.body.classList.toggle('native-fullscreen', Boolean(event.payload?.fullscreen));
  });
  sidebarDevFlowResizeObserver.observe(document.body);

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
  void requestSidebarScene('settings', 'skills').catch(renderSidebarError);
}

function closeSidebarSettings() {
  void requestSidebarScene('main').catch(renderSidebarError);
}

function selectSidebarSettingsPane(pane) {
  void requestSidebarScene('settings', pane).catch(renderSidebarError);
}

function renderSidebarSessions() {
  const container = document.getElementById('sidebarSessionList');
  if (!container) return;
  container.replaceChildren();
  if (!sidebarSessions.length) {
    const empty = document.createElement('div');
    empty.className = 'empty';
    empty.textContent = 'No sessions';
    container.appendChild(empty);
    return;
  }
  sidebarSessions.forEach(session => {
    const item = document.createElement('div');
    item.className = 'session-item' + (session.id === sidebarActiveSessionId ? ' active' : '');
    item.tabIndex = 0;
    item.onclick = () => switchSidebarSession(session.path);
    const status = document.createElement('span');
    status.className = 'session-status ' + (session.activity || 'idle');
    const name = document.createElement('span');
    name.className = 'session-name';
    name.textContent = session.name || 'Untitled';
    const meta = document.createElement('span');
    meta.className = 'session-meta';
    meta.textContent = formatSidebarSessionDate(session.modified);
    item.append(status, name, meta);
    container.appendChild(item);
  });
}

function renderSidebarState(snapshot) {
  if (!snapshot) return;
  sidebarSessions = Array.isArray(snapshot.sessions) ? snapshot.sessions : [];
  sidebarActiveSessionId = snapshot.activeSessionId || null;
  if (snapshot.git) {
    setSidebarText('sidebarGitBranch', snapshot.git.label || snapshot.git.projectName || '—');
    setSidebarText('sidebarGitAdd', '+' + Number(snapshot.git.added || 0));
    setSidebarText('sidebarGitDel', '-' + Number(snapshot.git.deleted || 0));
    setSidebarText('sidebarGitFiles', Number(snapshot.git.files || 0) + ' files');
  }
  renderSidebarQuota(snapshot);
  renderSidebarDevFlow(snapshot);
  renderSidebarSessions();
}

function renderSidebarQuota(snapshot) {
  const group = document.getElementById('sidebarQuotaGroup');
  if (!group) return;
  const showQuota = Boolean(snapshot.showQuota);
  group.hidden = !showQuota;
  if (!showQuota) return;
  const hourRow = document.getElementById('sidebarQuotaHourRow');
  if (hourRow) hourRow.hidden = !snapshot.showHourlyQuota;
  renderSidebarQuotaWindow('sidebarQuotaHourBar', 'sidebarQuotaHour', snapshot.quota?.primary, snapshot.rateLimitDisplayMode);
  const weekRow = document.getElementById('sidebarQuotaWeekRow');
  if (weekRow) weekRow.hidden = !snapshot.showWeeklyQuota;
  renderSidebarQuotaWindow('sidebarQuotaWeekBar', 'sidebarQuotaWeek', snapshot.quota?.secondary, snapshot.rateLimitDisplayMode);
}

function renderSidebarQuotaWindow(barId, valueId, window, mode) {
  const bar = document.getElementById(barId);
  const value = document.getElementById(valueId);
  if (!bar || !value) return;
  const used = Math.min(100, Math.max(0, Number(window?.usedPercent || 0)));
  const display = mode === 'used' ? used : 100 - used;
  bar.style.width = window ? display + '%' : '0%';
  bar.classList.toggle('warn', mode === 'used' ? used >= 80 : display <= 20);
  value.textContent = window ? Math.round(display) + '%' : '—';
}

function renderSidebarDevFlow(snapshot) {
  const df = snapshot && snapshot.devFlow ? snapshot.devFlow : null;
  sidebarDevFlow = df;
  sidebarLastDevFlowRowLimit = -1;
  const group = document.getElementById('sidebarDevFlowGroup');
  if (!group) return;
  const showSummary = Boolean(df) && df.availability === 'ready' && df.showSidebarStatus;
  group.hidden = !showSummary;
  if (!showSummary) {
    updateDevFlowDashboardButton(df);
    return;
  }
  const summary = document.getElementById('sidebarDevFlowSummary');
  if (summary) {
    summary.textContent = devFlowCountLabel(df.openTasks, 'Task') + ' · ' + devFlowCountLabel(df.openIssues, 'Issue');
    summary.classList.toggle('stale', Boolean(df.stale));
    summary.onclick = () => openDevFlowDetail({ kind: 'summary' });
  }
  renderDevFlowClaimedRows();
  updateDevFlowDashboardButton(df);
}

function devFlowCountLabel(count, noun) {
  return count === 1 ? '1 ' + noun : count + ' ' + noun + 's';
}

function renderDevFlowClaimedRows() {
  const container = document.getElementById('sidebarDevFlowClaimed');
  const more = document.getElementById('sidebarDevFlowMore');
  if (!container || !sidebarDevFlow) return;
  const rows = Array.isArray(sidebarDevFlow.claimed) ? sidebarDevFlow.claimed : [];
  const limit = computeDevFlowRowLimit();
  if (limit === sidebarLastDevFlowRowLimit) return;
  sidebarLastDevFlowRowLimit = limit;
  container.replaceChildren();
  const visible = rows.slice(0, limit);
  visible.forEach(item => {
    const row = document.createElement('button');
    row.type = 'button';
    row.className = 'dev-flow-claimed-row';
    row.title = item.title || '';
    row.onclick = () => openDevFlowDetail({ kind: 'item', id: item.id });
    const dot = document.createElement('span');
    dot.className = 'dev-flow-claimed-dot';
    dot.setAttribute('aria-hidden', 'true');
    const id = document.createElement('span');
    id.className = 'dev-flow-claimed-id';
    id.textContent = item.shortId || item.id;
    const title = document.createElement('span');
    title.className = 'dev-flow-claimed-title';
    title.textContent = item.title || '';
    row.append(dot, id, title);
    container.appendChild(row);
  });
  const hiddenCount = rows.length - visible.length;
  if (more) {
    more.hidden = hiddenCount <= 0;
    if (hiddenCount > 0) {
      more.textContent = 'more ' + hiddenCount;
      more.onclick = () => openDevFlowDetail({ kind: 'more' });
    }
  }
}

function computeDevFlowRowLimit() {
  const rowHeight = measureDevFlowRowHeight();
  if (rowHeight <= 0) return 0;
  const statusSection = document.getElementById('sidebarStatusSection');
  if (!statusSection) return 0;
  const top = document.body.classList.contains('native-fullscreen') ? 0 : 24;
  const bottom = document.querySelector('.sidebar-bottom')?.getBoundingClientRect().height || 0;
  let base = statusSection.getBoundingClientRect().height;
  const claimed = document.getElementById('sidebarDevFlowClaimed');
  const more = document.getElementById('sidebarDevFlowMore');
  if (claimed) base -= claimed.getBoundingClientRect().height;
  if (more && !more.hidden) base -= more.getBoundingClientRect().height;
  const budget = window.innerHeight - top - base - bottom - SIDEBAR_SESSIONS_MIN_PX;
  return Math.max(0, Math.floor(budget / rowHeight));
}

function measureDevFlowRowHeight() {
  const probe = document.createElement('button');
  probe.type = 'button';
  probe.className = 'dev-flow-claimed-row';
  probe.style.cssText = 'position:absolute;visibility:hidden;left:-10000px;top:0;';
  const dot = document.createElement('span');
  dot.className = 'dev-flow-claimed-dot';
  const id = document.createElement('span');
  id.className = 'dev-flow-claimed-id';
  id.textContent = 'T001';
  const title = document.createElement('span');
  title.className = 'dev-flow-claimed-title';
  title.textContent = 'Measure';
  probe.append(dot, id, title);
  document.body.appendChild(probe);
  const height = probe.getBoundingClientRect().height;
  probe.remove();
  return height;
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

function updateDevFlowDashboardButton(df) {
  const button = document.getElementById('sidebarDevFlowDashboard');
  if (!button) return;
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
  const container = document.getElementById('sidebarSessionList');
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
