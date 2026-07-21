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

document.documentElement.addEventListener('pointerleave', () => {
  if (!sidebarInvoke) return;
  void sidebarInvoke('set_native_sidebar_overlay_visible', { visible: false })
    .catch(error => console.error('[rozsa-gui][sidebar] failed to hide overlay', error));
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
  void requestSidebarScene('settings', 'appearance').catch(renderSidebarError);
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
  renderSidebarSessions();
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
