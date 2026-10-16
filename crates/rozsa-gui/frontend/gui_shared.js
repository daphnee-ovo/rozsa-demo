"use strict";

// Shared by the persistent main and sidebar WebViews.
// Structure: platform selection, root visibility, monotonic scene/theme snapshots.
// Scene snapshots are complete state: only a higher revision may change a
// pre-created scene root.
// Design: ../../../.dev-doc/main/SPEC.md#3-scene-与状态边界
(function installGuiShared(global) {
  function isNativeSplitPlatform() {
    return /Macintosh|Mac OS X/.test(global.navigator?.userAgent || '');
  }

  function setSceneRootVisible(root, visible) {
    if (!root) return;
    root.hidden = !visible;
    root.inert = !visible;
    root.setAttribute('aria-hidden', String(!visible));
  }

  function applySceneSnapshot(state, snapshot, render) {
    const revision = Number(snapshot?.revision);
    if (!Number.isSafeInteger(revision) || revision <= state.revision) return false;
    if (!['main', 'settings'].includes(snapshot.scene)) return false;
    render(snapshot);
    state.revision = revision;
    state.scene = snapshot.scene;
    state.selectedPane = snapshot.selectedPane || null;
    return true;
  }

  function applyThemeSnapshot(state, snapshot, render) {
    const revision = Number(snapshot?.revision);
    if (!Number.isSafeInteger(revision) || revision <= state.revision) return false;
    if (!['light', 'dark', 'system'].includes(snapshot.themeMode)) return false;
    if (!snapshot.lightTheme || !snapshot.darkTheme) return false;
    render(snapshot);
    state.revision = revision;
    return true;
  }

  function effectiveThemeMode(themeMode) {
    if (themeMode !== 'system') return themeMode;
    return global.matchMedia?.('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  }

  function resolveTheme(snapshot) {
    return effectiveThemeMode(snapshot.themeMode) === 'dark' ? snapshot.darkTheme : snapshot.lightTheme;
  }

  function applyThemeTokens(root, snapshot) {
    const theme = resolveTheme(snapshot);
    root.dataset.themeMode = effectiveThemeMode(snapshot.themeMode);
    root.dataset.themeId = theme.id;
    root.dataset.themeTranslucentSidebar = String(Boolean(theme.translucentSidebar && snapshot.isMacos));
    Object.entries(theme.variables || {}).forEach(([key, value]) => root.style.setProperty(key, value));
    root.style.setProperty('--accent', theme.accent);
    root.style.setProperty('--fg', theme.foreground);
    root.style.setProperty('--font-ui', theme.uiFont);
    root.style.setProperty('--font-mono', theme.codeFont);
    root.style.setProperty('--surface', theme.variables?.['--surface'] || theme.background);
  }

  global.RozsaGuiShared = Object.freeze({
    applySceneSnapshot,
    applyThemeSnapshot,
    applyThemeTokens,
    effectiveThemeMode,
    isNativeSplitPlatform,
    resolveTheme,
    setSceneRootVisible,
  });
})(window);
