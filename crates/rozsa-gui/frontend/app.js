"use strict";

// ===================================================================
// Rozsa GUI — Tauri IPC frontend
//
// Internal Framework:
// app.js
// +-- Initialization (DOMContentLoaded, Tauri API binding, event listeners)
// +-- State Rendering (renderState, updateHeader, updateSidebar, renderMessages)
// +-- Message Rendering (renderMessage, extractText, extractThinking)
// +-- Markdown Engine (renderMarkdown, inlineMd, codeBlock, renderTable)
// +-- Tool Events (handleToolEvent, trackTool, renderToolChips, toolIcon)
// +-- Permissions (showPermission, respondPermission, hidePermPanel)
// +-- Agent Questions (showUserQuestion, submitUserQuestion, hideQuestionPanel)
// +-- Messaging (sendMessage, abortAgent, dispatchSlashCommand)
// +-- Sessions (renderSessionList, doSwitchSession, newSession)
// +-- Models (renderModelSelector, onModelChange)
// +-- Settings (toggleSettings, closeSettings, switchSettingsTab, loadSettings, saveSetting)
// |   +-- Appearance switches and theme persistence
// +-- Slash Command Autocomplete (updateAutocomplete, selectSlashCmd, navigateAutocomplete)
// +-- Transient Popups (outside click and Escape dismissal)
// +-- Input Composition (IME lifecycle and input refresh)
// +-- Native File Drag (Finder paths to @path composer references)
// +-- Keyboard Shortcuts (global keydown handler)
// +-- UI Helpers (toggleToolCall, toggleThinking, copyCode, autoResize, escapeHtml)
// +-- Native Scene Routing (persistent MainContent/SettingsContent roots)
// Design: ../../../.dev-doc/main/SPEC.md#6-frontend-迁移边界
// ===================================================================

let invoke, listen;
let sessions = [];
let models = [];
let currentPermissionId = null;
let currentPermissionSessionId = null;
let currentPermissionTrustGroups = [];
let currentPermissionTrustIndex = -1;
let currentPermissionTrustKeys = [];
let permissionDisplayInFlight = false;
// 权限请求按 session id 分队列，避免后台 tab 覆盖当前 tab 的审批。
let pendingPermissions = {};
let pendingUserQuestions = {};
let currentQuestionId = null;
let currentQuestionSessionId = null;
let currentQuestionIndex = 0;
let currentQuestionAnswers = {};
let questionDisplayInFlight = false;
let toolCounts = {};
let currentSettings = null;
let capabilitySettings = null;
let permissionSettings = null;
const capabilityScope = { skills: 'global', tools: 'global' };
let permissionScope = 'global';
let pendingPermissionRuleKind = 'allow';
let permissionToolOptions = [];
let permissionToolActiveIndex = -1;
let keyBindingDefinitions = [
  { action: 'toggleThinking', title: 'Toggle thinking', description: 'Expand or collapse thinking blocks', defaultBinding: 'Ctrl+T', binding: 'Ctrl+T', scope: 'global' },
  { action: 'openModelPicker', title: 'Choose model', description: 'Open the model picker', defaultBinding: 'Ctrl+P', binding: 'Ctrl+P', scope: 'global' },
  { action: 'newSession', title: 'New session', description: 'Start a new session', defaultBinding: 'Ctrl+N', binding: 'Ctrl+N', scope: 'global' },
  { action: 'openSettings', title: 'Open settings', description: 'Open or close Settings', defaultBinding: 'Ctrl+,', binding: 'Ctrl+,', scope: 'global' },
  { action: 'sendMessage', title: 'Send message', description: 'Send from the focused composer', defaultBinding: 'Enter', binding: 'Enter', scope: 'composer' },
  { action: 'insertNewline', title: 'Insert new line', description: 'Add a line in the focused composer', defaultBinding: 'Shift+Enter', binding: 'Shift+Enter', scope: 'composer' },
  { action: 'focusComposer', title: 'Focus composer', description: 'Move focus to the message composer', defaultBinding: '/', binding: '/', scope: 'outsideComposer' },
];
let capturingKeyBindingAction = null;
let availableThemes = [];
let themeDefinitions = { light: null, dark: null };
let themeSaveQueues = { light: Promise.resolve(), dark: Promise.resolve() };
let systemThemeMediaQuery = null;
let nativeFullscreenTransitioning = false;
let isStreaming = false;
let acSelectedIndex = -1;
let acRequestSeq = 0;
let acPrefix = '';
let acItems = [];
let inputHighlightRanges = [];
let isInputComposing = false;
let activeSessionIdx = 0;
let activeSessionId = null;
// 跟踪每个 session 的 streaming 状态（path → bool）
let sessionStreamingState = {};
let quotaEligible = false;
let quotaModelKey = '';
let quotaLoaded = false;
let quotaLoading = false;
let quotaDisplayEnabled = true;
let weeklyQuotaDisplayEnabled = true;
let hourlyQuotaDisplayEnabled = true;
let rateLimitDisplayMode = 'remained';
let quotaSnapshot = null;
let chatAutoScrollPaused = false;
let renderedMessageSessionId = null;
let renderedMessageKeys = [];
let renderedRawMessageCount = 0;
let renderedTurnActivityKey = '';
let expandedToolCallsBySession = {};
let expandedThinkingBySession = {};
let thinkingStartTimes = {};
let thinkingDurations = {};
let renderedQueueKey = '';
let renderedSteeringKey = '';
let renderedSessionListKey = '';
let sessionViewState = {};
let sessionDraftState = {};
let permissionUiStateBySession = {};
let restoringSessionScroll = false;
let scrollStateFrame = 0;
let sidebarCollapsed = false;
let sidebarAutoCollapsed = false;
let nativeSplitMode = false;
let nativeSidebarOverlayVisible = false;
let nativeSidebarOverlayWidth = 0;
let nativeSidebarOverlayRequest = 0;
let nativeSidebarOverlayRevealInFlight = false;
let nativePointerClientX = null;
const guiSceneState = { revision: 0, scene: 'main', selectedPane: null };
const mainThemeState = { revision: 0 };
let pendingGuiSceneSnapshot = null;
let pendingGuiSceneIntent = null;
let mainSceneContinuity = null;
const TRANSIENT_POPUP_IDS = [
  'autocomplete',
  'forkPicker',
  'subagentPanel',
  'quotaTooltip',
  'thinkingEffortPopover',
];
const THINKING_EFFORT_OPTIONS = Object.freeze(
  ['off', 'low', 'medium', 'high', 'xhigh', 'max'].map(value => Object.freeze({
    value,
    label: value === 'xhigh' ? 'XHigh' : value[0].toUpperCase() + value.slice(1),
  }))
);
const DOUBLE_ESCAPE_WINDOW_MS = 1000;
const COMPOSER_HINT_ROTATION_MS = 30_000;
const COMPOSER_HINTS = [
  'Message Rózsa, supports Markdown…',
  '⏎ Send · ⇧⏎ New line',
  '⌃T Toggle thinking',
];
let lastStreamingEscapeAt = 0;
let composerHintIndex = 0;
let composerHintTimer = null;
let composerHintsDismissed = false;

// =============== Slash Commands Registry ===============

const slashCommands = [
  // Session Management
  { cmd: '/new', desc: 'New session', category: 'session' },
  { cmd: '/clear', desc: 'Clear current session', category: 'session' },
  { cmd: '/name', desc: 'Set session name', category: 'session' },
  { cmd: '/session', desc: 'Show session info', category: 'session' },
  { cmd: '/resume', desc: 'Resume session (session list)', category: 'session' },
  { cmd: '/clone', desc: 'Clone current session', category: 'session' },
  { cmd: '/fork', desc: 'Fork session from a message', category: 'session' },
  { cmd: '/tree', desc: 'View session entry tree', category: 'session' },
  { cmd: '/graph', desc: 'Visualize session timeline', category: 'session' },
  { cmd: '/gc', desc: 'Clean up expired session files', category: 'session' },

  // Model & Settings
  { cmd: '/model', desc: 'Switch model (open picker)', category: 'model' },
  { cmd: '/scoped-models', desc: 'List all available models', category: 'model' },
  { cmd: '/thinking', desc: 'Set thinking effort (off/low/medium/high/xhigh/max)', category: 'model' },
  { cmd: '/settings', desc: 'Open settings', category: 'settings' },
  { cmd: '/lsp', desc: 'Configure LSP diagnostics mode', category: 'settings' },

  // Context Management
  { cmd: '/compact', desc: 'Compact session context', category: 'context' },
  { cmd: '/permissions', desc: 'Show permission mode and stats', category: 'context' },
  { cmd: '/subagents', desc: 'List subagents', category: 'context' },
  { cmd: '/main', desc: 'Switch to main agent view', category: 'context' },

  // Data Operations
  { cmd: '/export', desc: 'Export session (html/md/jsonl)', category: 'data' },
  { cmd: '/import', desc: 'Import JSONL session file', category: 'data' },
  { cmd: '/share', desc: 'Share session (gh gist)', category: 'data' },
  { cmd: '/copy', desc: 'Copy last assistant message', category: 'data' },
  { cmd: '/search', desc: 'Search session content', category: 'data' },

  // Authentication
  { cmd: '/login', desc: 'OAuth login', category: 'auth' },
  { cmd: '/logout', desc: 'Log out', category: 'auth' },
  { cmd: '/usage', desc: 'Query rate limits', category: 'auth' },

  // Help & Utilities
  { cmd: '/help', desc: 'Show help', category: 'help' },
  { cmd: '/hotkeys', desc: 'Show keyboard shortcuts', category: 'help' },
  { cmd: '/changelog', desc: 'Show changelog', category: 'help' },
  { cmd: '/reload', desc: 'Reload configuration', category: 'help' },
  { cmd: '/quit', desc: 'Quit application', category: 'help' },
];

// Commands intercepted locally (not sent to backend as chat)
const LOCAL_COMMANDS = new Set([
  'model', 'settings', 'thinking', 'clear', 'new', 'help', 'hotkeys', 'quit',
]);

// =============== Initialization ===============

window.addEventListener('DOMContentLoaded', async () => {
  nativeSplitMode = window.RozsaGuiShared.isNativeSplitPlatform();
  preparePlatformSceneDom();
  setupChatScrollLock();
  if (!nativeSplitMode) {
    syncMainSidebarViewport();
    syncChromeBackgroundGeometry();
  }

  let retries = 0;
  while (!window.__TAURI__ && retries < 30) {
    await new Promise(r => setTimeout(r, 100));
    retries++;
  }
  if (!window.__TAURI__) {
    document.getElementById('chatMessages').innerHTML =
      '<div class="chat-empty"><div class="chat-empty-icon">!</div>' +
      '<div class="chat-empty-title">Tauri API not loaded</div></div>';
    return;
  }
  invoke = window.__TAURI__.core.invoke;
  listen = window.__TAURI__.event.listen;
  configureAttachmentPicker();
  await configureNativeFileDrag();
  setupComposerHints();
  setupNotificationErrorTray();

  await listen('gui-scene-snapshot', ev => applyGuiSceneSnapshot(ev.payload));
  await listen('theme-state', ev => applyMainThemeState(ev.payload));
  await listen('ui-state', ev => renderState(ev.payload));
  await listen('tool-event', ev => handleToolEvent(ev.payload));
  await listen('permission-request', ev => showPermission(ev.payload));
  await listen('question-request', ev => showUserQuestion(ev.payload));
  await listen('error', ev => showError(typeof ev.payload === 'string' ? ev.payload : JSON.stringify(ev.payload)));
  await listen('dev-flow-detail', ev => showDevFlowDetail(ev.payload));
  await listen('dev-flow-detail-dismiss', () => closeDevFlowDetail());
  await listen('app-notification', ev => {
    const payload = ev.payload;
    if (payload && payload.type === 'upsert') {
      upsertNotification(payload);
    } else if (payload && payload.type === 'resolve') {
      resolveNotification(payload.id);
    }
  });
  await listen('notification', ev => showNotification(typeof ev.payload === 'string' ? ev.payload : JSON.stringify(ev.payload)));
  await listen('models-updated', async () => {
    try {
      models = await invoke('list_models');
      renderModelSelector();
    } catch (e) {
      showError('list_models failed after refresh: ' + String(e));
    }
  });
  await listen('native-sidebar-toggle', () => {
    if (!nativeSplitMode) toggleMainSidebar();
  });
  await listen('native-sidebar-state', ev => {
    if (nativeSplitMode) {
      const collapsed = Boolean(ev.payload);
      if (!collapsed) {
        nativeSidebarOverlayRequest += 1;
        nativeSidebarOverlayRevealInFlight = false;
        nativeSidebarOverlayVisible = false;
      }
      setMainSidebarCollapsed(collapsed, false);
    }
  });
  await listen('native-fullscreen', ev => {
    console.debug('[rozsa-gui][fullscreen] native event', ev.payload);
    const payload = ev.payload;
    const fullscreen = typeof payload === 'object' ? Boolean(payload?.fullscreen) : Boolean(payload);
    nativeFullscreenTransitioning = Boolean(payload?.transitioning);
    setNativeFullscreen(fullscreen);
    if (!nativeFullscreenTransitioning) scheduleNativeFullscreenSync('native-event');
  });
  scheduleNativeFullscreenSync('startup');

  if (nativeSplitMode) {
    try {
      setMainSidebarCollapsed(Boolean(await invoke('native_sidebar_collapsed')), false);
      const snapshot = await invoke('gui_webview_ready', {
        webview: 'main',
        lastRevision: guiSceneState.revision,
      });
      applyGuiSceneSnapshot(snapshot);
    } catch (e) {
      showError('gui_webview_ready failed: ' + String(e));
    }
  }

  try { const s = await invoke('get_state'); renderState(s); } catch (e) { showError('get_state failed: ' + String(e)); }
  if (!nativeSplitMode) {
    try { sessions = await invoke('get_sessions'); renderSessionList(); } catch (e) { showSidebarError('sessionList', 'get_sessions failed: ' + String(e)); }
  }
  try { models = await invoke('list_models'); renderModelSelector(); } catch (e) { showError('list_models failed: ' + String(e)); }
  loadSettings().catch(() => {});
  loadKeyBindings().catch(() => {});
  refreshRateLimits(false);
});

window.addEventListener('resize', syncMainSidebarViewport);
window.addEventListener('resize', syncChromeBackgroundGeometry);
window.addEventListener('resize', scheduleNativeFullscreenSync);
window.addEventListener('resize', positionThinkingEffortPopover);
window.addEventListener('pointermove', handleSidebarEdgeReveal);
window.addEventListener('pointerdown', handleSidebarEdgeReveal);
document.documentElement.addEventListener('pointerenter', handleSidebarEdgeReveal);

function setupComposerHints() {
  const input = document.getElementById('msgInput');
  if (!input) return;
  input.dataset.placeholder = COMPOSER_HINTS[composerHintIndex];
  composerHintTimer = window.setInterval(rotateComposerHint, COMPOSER_HINT_ROTATION_MS);
  input.addEventListener('pointerdown', dismissComposerHints, { once: true });
}

function rotateComposerHint() {
  const input = document.getElementById('msgInput');
  if (!input || composerHintsDismissed) return;
  composerHintIndex = (composerHintIndex + 1) % COMPOSER_HINTS.length;
  input.dataset.placeholder = COMPOSER_HINTS[composerHintIndex];
}

function dismissComposerHints() {
  composerHintsDismissed = true;
  if (composerHintTimer !== null) {
    window.clearInterval(composerHintTimer);
    composerHintTimer = null;
  }
  const input = document.getElementById('msgInput');
  if (input) input.dataset.placeholder = '';
}

// =============== State Rendering ===============

function renderState(snap) {
  if (!snap) return;
  const previousSessionId = activeSessionId;
  const sessionChanged = !!snap.sessionId && previousSessionId !== snap.sessionId;
  if (sessionChanged && previousSessionId) captureSessionDraft(previousSessionId);
  if (sessionChanged && previousSessionId) capturePermissionUiState(previousSessionId);
  const wasStreaming = isStreaming;
  isStreaming = !!snap.isStreaming;
  if (wasStreaming && !isStreaming) chatAutoScrollPaused = false;
  if (!isStreaming) lastStreamingEscapeAt = 0;
  // 记录当前活跃 session 的 streaming 状态
  if (snap.sessionId) {
    activeSessionId = snap.sessionId;
    const approvals = pendingPermissions[snap.sessionId] || [];
    const questions = pendingUserQuestions[snap.sessionId] || [];
    sessionStreamingState[snap.sessionId] = approvals.length
      ? 'approval'
      : (questions.length ? 'question' : (isStreaming ? 'running' : 'idle'));
  }
  if (snap.streamUpdate) {
    renderMessages(snap.messages, true, snap.sessionId || null, snap.turnActivity, snap.turnSummaries);
    return;
  }
  updateHeader(snap);
  updateContextUsage(snap.contextUsage);
  updateQuotaVisibility(snap.model);
  if (!nativeSplitMode) updateSidebar(snap);
  renderMessages(snap.messages, snap.isStreaming, snap.sessionId || null, snap.turnActivity, snap.turnSummaries);
  renderRunningMessages(snap.queuedMessages, snap.steeringConversation);
  updateAbortButton();
  if (!nativeSplitMode) renderSessionList();
  if (sessionChanged) restoreSessionDraft(snap.sessionId);
  schedulePermPanelDisplay();
  scheduleQuestionPanelDisplay();
}

function updateHeader(snap) {
  const nameEl = document.getElementById('currentSessionName');
  if (nameEl && !snap.streamUpdate) nameEl.textContent = snap.sessionName || 'Rózsa';

  const modelBtn = document.getElementById('modelSelector');
  if (modelBtn && snap.model) modelBtn.textContent = snap.model.id;

  const thinkingEffort = document.getElementById('thinkingEffort');
  if (thinkingEffort && snap.thinkingEffort) {
    thinkingEffort.textContent = snap.thinkingEffort;
    renderThinkingEffortPicker(snap.thinkingEffort);
  }
}

function updateQuotaBars(snapshot) {
  const hourBar = document.getElementById('quotaHourBar');
  const hourVal = document.getElementById('quotaHour');
  const weekBar = document.getElementById('quotaWeekBar');
  const weekVal = document.getElementById('quotaWeek');
  updateQuotaWindow(hourBar, hourVal, snapshot && snapshot.primary, '5 hours');
  updateQuotaWindow(weekBar, weekVal, snapshot && snapshot.secondary, 'This week');
  const weekRow = weekBar && weekBar.closest('.quota-row');
  if (weekRow) weekRow.style.display = weeklyQuotaDisplayEnabled ? '' : 'none';
  const hourRow = hourBar && hourBar.closest('.quota-row');
  if (hourRow) hourRow.style.display = hourlyQuotaDisplayEnabled ? '' : 'none';
}

function updateQuotaWindow(bar, valueEl, window, label) {
  if (!bar || !valueEl) return;
  const row = bar.closest('.quota-row');
  if (!window) {
    bar.style.width = '0%';
    bar.classList.remove('warn');
    valueEl.textContent = '—';
    setQuotaTooltip(row, bar, valueEl, '');
    return;
  }
  const used = clampPercent(Number(window.usedPercent || 0));
  const display = rateLimitDisplayMode === 'used' ? used : 100 - used;
  bar.style.width = display + '%';
  bar.classList.toggle('warn', rateLimitDisplayMode === 'used' ? used >= 80 : display <= 20);
  valueEl.textContent = Math.round(display) + '%';
  setQuotaTooltip(row, bar, valueEl, formatResetTitle(label, window));
}

function setQuotaTooltip(row, bar, valueEl, text) {
  for (const el of [row, bar, valueEl]) {
    if (!el) continue;
    el.removeAttribute('title');
    if (text) el.dataset.quotaTooltip = text;
    else delete el.dataset.quotaTooltip;
  }
}

async function refreshRateLimits(showResult) {
  if (!quotaEligible) {
    updateQuotaBars(null);
    if (showResult) showNotification('Current model has no subscription quota');
    return;
  }
  if (quotaLoading) return;
  quotaLoading = true;
  try {
    const snapshot = await invoke('get_rate_limits');
    quotaSnapshot = snapshot;
    updateQuotaBars(snapshot);
    quotaLoaded = true;
    if (showResult) showNotification(formatRateLimitSnapshot(snapshot));
  } catch (e) {
    quotaSnapshot = null;
    updateQuotaBars(null);
    quotaLoaded = true;
    if (showResult) showError('Rate limit query failed: ' + String(e));
  } finally {
    quotaLoading = false;
  }
}

function updateSidebar(snap) {
  updateQuotaVisibility(snap.model);

  const branchEl = document.getElementById('gitBranch');
  const addEl = document.getElementById('gitAdd');
  const delEl = document.getElementById('gitDel');
  const filesEl = document.getElementById('gitFiles');
  if (snap.git) {
    if (branchEl) branchEl.textContent = snap.git.label || snap.git.projectName || '—';
    if (addEl) addEl.textContent = '+' + (snap.git.added || 0);
    if (delEl) delEl.textContent = '-' + (snap.git.deleted || 0);
    if (filesEl) filesEl.textContent = (snap.git.files || 0) + ' files';
  } else if (snap.cwd) {
    const parts = snap.cwd.split('/');
    if (branchEl) branchEl.textContent = parts[parts.length - 1] || snap.cwd;
    if (addEl) addEl.textContent = '—';
    if (delEl) delEl.textContent = '—';
    if (filesEl) filesEl.textContent = '—';
  }
}

function updateContextUsage(contextUsage) {
  if (contextUsage) {
    const pct = clampPercent(Number(contextUsage.percent || 0));
    const tokens = Number(contextUsage.tokens || 0);
    const ctxEl = document.getElementById('contextTokens');
    if (ctxEl) ctxEl.textContent = formatCompactTokens(tokens);
    const ring = document.querySelector('.context-ring circle:last-child');
    if (ring) ring.setAttribute('stroke-dashoffset', 44 - (44 * pct / 100));
    const ringWrap = document.querySelector('.context-ring');
    if (ringWrap) {
      ringWrap.removeAttribute('title');
      ringWrap.dataset.quotaTooltip = formatContextTooltip(contextUsage);
    }
  }
}

function updateQuotaVisibility(model) {
  const nextEligible = modelHasRateLimit(model);
  const nextKey = modelKey(model);
  const group = document.getElementById('quotaGroup');
  quotaEligible = nextEligible;
  if (group) group.style.display = nextEligible && quotaDisplayEnabled ? '' : 'none';
  if (!nextEligible) {
    quotaLoaded = false;
    quotaLoading = false;
    quotaModelKey = nextKey;
    updateQuotaBars(null);
    hideQuotaTooltip();
    return;
  }
  if (quotaModelKey !== nextKey) {
    quotaModelKey = nextKey;
    quotaLoaded = false;
    updateQuotaBars(null);
  }
  if (!quotaLoaded && nextEligible && quotaDisplayEnabled) refreshRateLimits(false);
}

function modelHasRateLimit(model) {
  return modelProviderKey(model) === 'codex-oauth';
}

function modelKey(model) {
  if (!model) return '';
  return modelProviderKey(model) + '/' + String(model.id || '');
}

function modelProviderKey(model) {
  if (!model || !model.provider) return '';
  const raw = String(model.provider);
  const custom = raw.match(/^Custom\("(.+)"\)$/);
  return (custom ? custom[1] : raw).toLowerCase();
}

function updateAbortButton() {
  const sendBtn = document.querySelector('[data-od-id="send-btn"]');
  const mode = document.getElementById('runningSendMode');
  const input = document.getElementById('msgInput');
  if (!sendBtn) return;
  if (isStreaming) {
    const hasText = !!getInputText(input).trim();
    if (mode) mode.hidden = !hasText;
    if (hasText) {
      if (mode && !mode.dataset.initialized) {
        mode.value = currentSettings?.running_send_mode || 'queue';
        mode.dataset.initialized = 'true';
      }
      const selected = mode ? mode.value : 'queue';
      sendBtn.textContent = selected === 'steer' ? 'Steer' : 'Queue';
      sendBtn.onclick = sendMessage;
    } else {
      sendBtn.textContent = 'Stop';
      sendBtn.onclick = abortAgent;
    }
  } else {
    if (mode) mode.hidden = true;
    sendBtn.textContent = 'Send';
    sendBtn.onclick = sendMessage;
  }
}

// =============== Message Rendering ===============

function renderMessages(messages, streaming, sessionId = null, turnActivity = null, turnSummaries = []) {
  const container = document.getElementById('chatMessages');
  if (!container) return;
  const sessionChanged = renderedMessageSessionId !== sessionId;
  if (sessionChanged && renderedMessageSessionId) {
    persistSessionViewState(renderedMessageSessionId, container);
  }
  const savedView = sessionId ? sessionViewState[sessionId] : null;
  if (sessionChanged) chatAutoScrollPaused = savedView?.autoScrollPaused === true;
  const restoringScroll = sessionChanged && savedView && Number.isFinite(savedView.scrollTop);
  const shouldStickToBottom = !restoringScroll && !chatAutoScrollPaused &&
    (sessionChanged ? !savedView : isChatNearBottom(container));

  if (!messages || messages.length === 0) {
    container.innerHTML = '<div class="chat-empty"><div class="chat-empty-icon">R</div>' +
      '<div class="chat-empty-title">Start a new conversation</div>' +
      '<div class="chat-empty-hint">Describe your coding task to Rózsa' +
      '<div class="chat-empty-kbd"><kbd>Enter</kbd> Send <kbd>Shift+Enter</kbd> New line</div></div></div>';
    renderedMessageSessionId = sessionId;
    renderedMessageKeys = [];
    renderedRawMessageCount = 0;
    renderedTurnActivityKey = '';
    if (sessionId) restoreSessionViewState(sessionId, container, savedView);
    return;
  }

  // 预建 toolResult 索引: toolCallId → { output, isError, toolName }
  // 每个 toolResult 通过 toolCallId 严格对应一个 toolCall
  const toolResultMap = {};
  for (const raw of messages) {
    if (raw.kind === 'standard' && raw.message && raw.message.role === 'toolResult') {
      const m = raw.message;
      const id = m.toolCallId;
      if (id) {
        const text = (m.content || []).filter(b => b.type === 'text').map(b => b.text).join('\n');
        toolResultMap[id] = { output: text, isError: !!m.isError, toolName: m.toolName || '', details: m.details || {} };
      }
    }
  }

  const visibleMessages = messages.filter(raw =>
    !(raw.kind === 'standard' && raw.message && raw.message.role === 'toolResult')
  );
  const lastAssistantIndex = streaming ? -1 : visibleMessages.reduce((last, raw, index) =>
    raw.kind === 'standard' && raw.message && raw.message.role === 'assistant' ? index : last, -1);
  const turnActivityKey = !streaming && turnActivity ? JSON.stringify(turnActivity) : '';
  const summariesByRawIndex = new Map((Array.isArray(turnSummaries) ? turnSummaries : [])
    .map(summary => [summary.assistantMessageIndex, summary.activity]));
  const activityForVisibleIndex = index => {
    const rawIndex = messages.indexOf(visibleMessages[index]);
    return summariesByRawIndex.get(rawIndex) || (index === lastAssistantIndex ? turnActivity : null);
  };

  const activeStreamIndex = activeStreamMessageIndex(messages, visibleMessages, streaming);
  const thinkingDurationForIndex = updateThinkingTimings(
    messages,
    visibleMessages,
    activeStreamIndex,
    sessionId,
  );
  const keys = visibleMessages.map((raw, index) => JSON.stringify(raw) +
    JSON.stringify(activityForVisibleIndex(index)) + ':' + (thinkingDurationForIndex(index) ?? '') +
    bashPresentationRenderKey(raw, toolResultMap));
  const sameSession = renderedMessageSessionId === sessionId;
  let firstChanged = -1;
  if (sameSession) {
    const shared = Math.min(renderedMessageKeys.length, keys.length);
    for (let i = 0; i < shared; i++) {
      if (renderedMessageKeys[i] !== keys[i]) { firstChanged = i; break; }
    }
    if (firstChanged < 0 && renderedMessageKeys.length !== keys.length) firstChanged = shared;
    // Tool results are hidden rows but change the preceding assistant tool card.
    if (firstChanged < 0 && renderedRawMessageCount !== messages.length) {
      firstChanged = Math.max(0, visibleMessages.length - 1);
    }
    if (firstChanged < 0 && turnActivityKey && turnActivityKey !== renderedTurnActivityKey) {
      firstChanged = lastAssistantIndex;
    }
  }

  const needsFullRender = !sameSession || container.children.length !== renderedMessageKeys.length || firstChanged === 0 && renderedMessageKeys.length === 0;
  if (needsFullRender) {
    container.replaceChildren();
    firstChanged = 0;
  }
  let patchedThinking = false;
  if (sameSession && firstChanged === activeStreamIndex && activeStreamIndex >= 0) {
    patchedThinking = patchStreamingThinking(
      container,
      activeStreamIndex,
      visibleMessages[activeStreamIndex],
      thinkingDurationForIndex(activeStreamIndex),
    );
    if (patchedThinking) firstChanged = -1;
  }

  let preservedThinkingExpanded = false;
  if (firstChanged >= 0 && container.children[firstChanged]) {
    preservedThinkingExpanded = container.children[firstChanged]
      .querySelector('.thinking-block')?.classList.contains('expanded') === true;
    if (preservedThinkingExpanded) {
      expandedThinkingBySession[thinkingStateKey(sessionId, firstChanged)] = true;
    }
  }

  if (firstChanged >= 0) {
    while (container.children.length > firstChanged) container.lastChild.remove();
    for (let i = firstChanged; i < visibleMessages.length; i++) {
      container.appendChild(renderMessage(
        visibleMessages[i],
        toolResultMap,
        i === activeStreamIndex,
        activityForVisibleIndex(i),
        thinkingDurationForIndex(i),
        isThinkingExpanded(sessionId, i),
      ));
    }
  }

  container.querySelectorAll('.stream-cursor').forEach(cursor => cursor.remove());
  if (activeStreamIndex >= 0) {
    const active = container.children[activeStreamIndex];
    if (active) attachStreamCursor(active);
  }

  toolCounts = countTools(visibleMessages);
  renderToolChips();
  renderedMessageSessionId = sessionId;
  renderedMessageKeys = keys;
  renderedRawMessageCount = messages.length;
  if (turnActivityKey) renderedTurnActivityKey = turnActivityKey;
  if (restoringScroll) restoreSessionViewState(sessionId, container, savedView);
  else if (shouldStickToBottom) scrollChatToBottom(container);
}

function bashPresentationRenderKey(raw, toolResultMap) {
  const content = raw && raw.message && raw.message.content;
  if (!Array.isArray(content)) return '';
  return JSON.stringify(content
    .filter(block => block.type === 'toolCall' && block.id)
    .map(block => parseDevFlowBashPresentation(
      block,
      toolResultMap[block.id] || null,
      devFlowTitleForItem,
    )));
}

function countTools(messages) {
  const counts = {};
  for (const raw of messages) {
    const content = raw && raw.message && raw.message.content;
    if (!Array.isArray(content)) continue;
    for (const block of content) {
      if (block.type === 'toolCall' && block.name) counts[block.name] = (counts[block.name] || 0) + 1;
    }
  }
  return counts;
}

function isChatNearBottom(container) {
  const distance = container.scrollHeight - container.scrollTop - container.clientHeight;
  return distance <= 48;
}

function scrollChatToBottom(container) {
  container.scrollTop = container.scrollHeight;
}

function setupChatScrollLock() {
  const container = document.getElementById('chatMessages');
  if (!container) return;
  container.addEventListener('wheel', ev => {
    if (ev.deltaY < 0) {
      chatAutoScrollPaused = true;
      return;
    }
    if (ev.deltaY > 0) {
      requestAnimationFrame(() => {
        if (isChatNearBottom(container)) chatAutoScrollPaused = false;
      });
    }
  }, { passive: true });
  container.addEventListener('scroll', () => {
    if (restoringSessionScroll || !renderedMessageSessionId) return;
    chatAutoScrollPaused = !isChatNearBottom(container);
    if (scrollStateFrame) cancelAnimationFrame(scrollStateFrame);
    scrollStateFrame = requestAnimationFrame(() => {
      scrollStateFrame = 0;
      persistSessionViewState(renderedMessageSessionId, container);
    });
  }, { passive: true });
}

function persistSessionViewState(sessionId, container) {
  if (!sessionId || !container) return;
  sessionViewState[sessionId] = {
    scrollTop: container.scrollTop,
    autoScrollPaused: chatAutoScrollPaused,
  };
}

function restoreSessionViewState(sessionId, container, savedView) {
  const saved = savedView || (sessionId ? sessionViewState[sessionId] : null);
  if (!saved || !Number.isFinite(saved.scrollTop)) {
    chatAutoScrollPaused = false;
    scrollChatToBottom(container);
    return;
  }
  restoringSessionScroll = true;
  requestAnimationFrame(() => {
    const maximum = Math.max(0, container.scrollHeight - container.clientHeight);
    container.scrollTop = Math.min(saved.scrollTop, maximum);
    chatAutoScrollPaused = saved.autoScrollPaused === true || !isChatNearBottom(container);
    restoringSessionScroll = false;
    persistSessionViewState(sessionId, container);
  });
}

function renderMessage(raw, toolResultMap, isActiveStream = false, turnActivity = null, thinkingDurationMs = null, thinkingExpanded = false) {
  const div = document.createElement('div');

  if (raw.kind === 'custom') {
    div.className = 'msg msg-assistant';
    const cm = raw.message;
    const msgType = cm.message_type || cm.messageType || 'system';
    let displayText = '';
    if (msgType === 'bashExecution' && cm.payload) {
      displayText = '$ ' + (cm.payload.command || '') + '\n' + (cm.payload.output || '');
    } else {
      displayText = typeof cm.payload === 'string' ? cm.payload : JSON.stringify(cm.payload, null, 2);
    }
    div.innerHTML = '<div class="msg-avatar">R</div><div class="msg-body">' +
      '<div class="msg-role">' + escapeHtml(msgType) + '</div>' +
      '<div class="msg-content"><pre style="font-family:var(--font-mono);font-size:12px;white-space:pre-wrap">' +
      escapeHtml(displayText) + '</pre></div></div>';
    return div;
  }

  const msg = raw.message;
  if (!msg) { div.textContent = JSON.stringify(raw); return div; }
  const role = msg.role;
  const content = msg.content || [];

  if (role === 'user') {
    div.className = 'msg msg-user';
    const text = extractText(content);
    div.innerHTML = '<div class="msg-avatar">U</div><div class="msg-body">' +
      '<div class="msg-role">You</div>' +
      '<div class="msg-content markdown-body">' + renderMarkdown(text) + '</div></div>';

  } else if (role === 'assistant') {
    div.className = 'msg msg-assistant';
    let body = '<div class="msg-avatar">R</div><div class="msg-body"><div class="msg-role">Rozsa</div>';
    const errorMessage = msg.errorMessage || '';
    if (errorMessage) {
      body += '<div class="msg-content msg-error"><pre>' + escapeHtml(errorMessage) + '</pre></div>';
    }

    const thinking = extractThinking(content);
    const latestType = content.length ? content[content.length - 1].type : '';
    if (thinking) {
      const thinkingActive = isActiveStream && latestType === 'thinking';
      const thinkingLabel = thinkingActive ? 'THINKING' : 'THINKED';
      const thinkingDuration = thinkingActive || thinkingDurationMs === null
        ? ''
        : formatThinkingDuration(thinkingDurationMs);
      body += '<div class="thinking-block' + (thinkingActive ? ' active' : '') + (thinkingExpanded ? ' expanded' : '') + '"><div class="thinking-header" role="button" tabindex="0" aria-expanded="' + String(thinkingExpanded) + '" onclick="toggleThinking(this)" onkeydown="if(event.key===\'Enter\'||event.key===\' \'){event.preventDefault();toggleThinking(this)}">' +
        '<svg class="thinking-icon" width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M8 1.5C5 1.5 3 3.5 3 6c0 1.5.8 2.7 2 3.5V12a1 1 0 001 1h4a1 1 0 001-1V9.5c1.2-.8 2-2 2-3.5 0-2.5-2-4.5-5-4.5z"/><path d="M6 14.5h4"/></svg>' +
        '<span class="thinking-label">' + thinkingLabel + '</span>' +
        (thinkingDuration ? '<span class="thinking-duration">' + thinkingDuration + '</span>' : '') +
        '<span class="thinking-chevron">▸</span></div>' +
        '<div class="thinking-content"' + (thinkingActive ? ' data-stream-cursor-target="thinking"' : '') + '>' +
        '<div class="thinking-markdown markdown-body">' + renderMarkdown(thinking) + '</div></div></div>';
    }

    const toolCalls = content.filter(b => b.type === 'toolCall');
    for (const tc of toolCalls) {
      trackTool(tc.name);
      // 通过 tc.id 查找对应的 toolResult
      const result = toolResultMap && tc.id ? toolResultMap[tc.id] : null;
      const devFlowPresentation = parseDevFlowBashPresentation(tc, result, devFlowTitleForItem);
      if (devFlowPresentation && tc.id) requestDevFlowTitles(tc.id, devFlowPresentation);
      const tcStatus = result ? (result.isError ? 's-error' : 's-success') : 's-running';
      const toolTitle = devFlowPresentation
        ? formatBashDevFlowTitle(devFlowPresentation)
        : formatToolTitle(tc);
      const resultOutput = result ? result.output : '';
      const bodyOutput = resultOutput || formatToolArgs(tc);
      const delta = result && Array.isArray(result.details.file_deltas) ? result.details.file_deltas[0] : null;
      const writeContent = delta && typeof delta.after === 'string'
        ? delta.after
        : (tc.arguments && typeof tc.arguments.content === 'string' ? tc.arguments.content : null);
      let toolBody = escapeHtml(bodyOutput);
      let toolBodyClass = '';
      if (tc.name.toLowerCase() === 'write' && writeContent !== null) {
        toolBody = renderCodeView(writeContent);
        toolBodyClass = ' code-view';
      } else if (delta && tc.name.toLowerCase() === 'edit' && typeof delta.patch === 'string') {
        toolBody = renderDiffView(delta.patch);
        toolBodyClass = ' diff-view';
      } else if (devFlowPresentation && result) {
        toolBody = renderBashToolEvidence(tc, result);
        toolBodyClass = ' dev-flow-tool-evidence';
      }

      body += '<div class="tool-call' + (isToolCallExpanded(tc.id) ? ' expanded' : '') +
        (devFlowPresentation ? ' dev-flow-tool-call' : '') +
        '" data-tool-call-id="' + escapeHtml(tc.id || '') + '" data-session-id="' +
        escapeHtml(activeSessionId || '') + '" onclick="toggleToolCall(this)">' +
        '<div class="tool-track"><div class="tool-icon">' + toolIcon(tc.name) + '</div>' +
        '</div>' +
        '<div class="tool-content"><div class="tool-header">' +
        '<span class="tool-call-status ' + tcStatus + '"></span>' +
        '<span class="tool-name">' + escapeHtml(toolTitle.name) + '</span>' +
        '<span class="tool-call-args">' + escapeHtml(toolTitle.arg) + '</span>' +
        '<span class="tool-call-toggle">▸</span></div></div>' +
        '<div class="tool-call-body' + toolBodyClass + '">' +
        (toolBodyClass ? toolBody : '<pre style="white-space:pre-wrap;margin:0;font-size:11.5px">' + toolBody + '</pre>') +
        '</div></div>';
    }

    const text = extractText(content);
    if (text) {
      const textActive = isActiveStream && latestType === 'text';
      body += '<div class="msg-content markdown-body"' + (textActive ? ' data-stream-cursor-target="text"' : '') +
        '>' + renderMarkdown(text) + '</div>';
    }

    if (turnActivity && ((turnActivity.changedFiles && turnActivity.changedFiles.length) || turnActivity.verification)) {
      body += renderTurnActivityCard(turnActivity);
    }

    body += '</div>';
    div.innerHTML = body;

  } else if (role === 'toolResult') {
    div.className = 'msg msg-assistant';
    const text = extractText(content);
    const toolName = msg.toolName || 'tool';
    const status = msg.isError ? 's-error' : 's-success';
    trackTool(toolName);
    const preview = (text || '').slice(0, 150);
    div.innerHTML = '<div class="msg-avatar" style="visibility:hidden">R</div><div class="msg-body">' +
      '<div class="tool-call" onclick="toggleToolCall(this)"><div class="tool-track">' +
      '<div class="tool-icon">' + toolIcon(toolName) + '</div>' +
      '<span class="tool-name">' + escapeHtml(toolName) + '</span></div>' +
      '<div class="tool-content"><div class="tool-header">' +
      '<span class="tool-call-status ' + status + '"></span>' +
      '<span class="tool-call-args">' + escapeHtml(preview) + '</span>' +
      '<span class="tool-call-toggle">▸</span></div></div>' +
      '<div class="tool-call-body"><pre style="white-space:pre-wrap;margin:0">' + escapeHtml(text || '') + '</pre></div></div></div>';

  } else {
    div.className = 'msg msg-assistant';
    div.innerHTML = '<div class="msg-avatar">R</div><div class="msg-body"><div class="msg-content"><pre>' +
      escapeHtml(JSON.stringify(msg, null, 2)) + '</pre></div></div>';
  }

  return div;
}

function parseDevFlowBashPresentation(toolCall, result, titleLookup = null) {
  if (!toolCall || !result || typeof toolCall.name !== 'string' ||
      toolCall.name.toLowerCase() !== 'bash') return null;
  if (result.toolName && typeof result.toolName === 'string' &&
      result.toolName.toLowerCase() !== 'bash') return null;
  const details = result.details && typeof result.details === 'object' ? result.details : {};
  if (result.isError || details.success === false || details.exit_code !== 0 || details.truncated === true) {
    return null;
  }
  const command = toolCall.arguments && typeof toolCall.arguments.command === 'string'
    ? toolCall.arguments.command
    : '';
  const parsed = parseDevFlowBashCommand(command);
  if (!parsed) return null;
  const { words, stageCount } = parsed;
  const operation = parseDevFlowAction(words, result.output);
  if (!operation || (stageCount > 1 && operation.action !== 'created')) return null;
  if (!operation.ids.length && !operation.allowEmpty) return null;
  return {
    action: operation.action,
    items: operation.ids.map(({ kind, id }) => ({
      kind: operation.expectedKind || kind,
      id,
      shortId: devFlowShortId(id),
      title: typeof titleLookup === 'function' ? titleLookup(operation.expectedKind || kind, id) : null,
    })),
  };
}

const DEV_FLOW_RESOURCE_VALUE_OPTIONS = {
  task: {
    update: ['--title', '--task-type', '--priority', '--refs', '--file', '--depends-on', '--parallel', '--complexity', '--done-when'],
    remove: ['--confirm'],
    reopen: ['--confirm'],
  },
  issue: {
    update: ['--title', '--severity', '--location', '--desc', '--reproduce', '--fix', '--file'],
    remove: ['--confirm'],
    reopen: ['--confirm'],
  },
};

function parseDevFlowAction(words, output) {
  const invocation = parseDevFlowInvocation(words);
  if (!invocation) return null;
  if (invocation.command === 'claim') {
    const claim = parseDevFlowClaimArgs(invocation.args);
    if (!claim) return null;
    return {
      action: claim.revoke ? 'released' : 'claimed',
      expectedKind: null,
      ids: claim.ids,
      allowEmpty: claim.revoke && claim.ids.length === 0,
    };
  }

  const { kind, operation, args } = invocation;
  if (operation === 'create') {
    const ids = parseCreatedDevFlowIds(output, kind);
    return ids ? { action: 'created', expectedKind: kind, ids, allowEmpty: false } : null;
  }

  const actionByOperation = {
    update: 'updated',
    remove: 'removed',
    done: 'completed',
    close: 'closed',
    reopen: 'reopened',
  };
  const action = actionByOperation[operation];
  if (!action) return null;
  if ((kind === 'task' && operation === 'close') ||
      (kind === 'issue' && operation === 'done')) return null;

  const ids = parseDevFlowResourceIds(
    args,
    kind,
    DEV_FLOW_RESOURCE_VALUE_OPTIONS[kind]?.[operation] || [],
    operation === 'done' || operation === 'close',
  );
  return ids ? { action, expectedKind: kind, ids, allowEmpty: false } : null;
}

function parseDevFlowInvocation(words) {
  if (!Array.isArray(words) || !words.length || !isDevFlowExecutable(words[0])) return null;
  const args = words.slice(1);
  let index = 0;
  while (isDevFlowFormatOption(args[index])) index++;
  if (args[index] === 'claim') {
    return { command: 'claim', args: args.slice(index + 1) };
  }
  if (args[index] !== 'task' && args[index] !== 'issue') return null;
  const kind = args[index++];
  while (isDevFlowFormatOption(args[index])) index++;
  const operation = args[index++];
  if (!operation) return null;
  return { kind, operation, args: args.slice(index) };
}

function isDevFlowFormatOption(value) {
  return value === '-H' || value === '--human';
}

function splitDevFlowOption(value) {
  const text = String(value || '');
  const separator = text.indexOf('=');
  return separator < 0
    ? { name: text, inlineValue: null }
    : { name: text.slice(0, separator), inlineValue: text.slice(separator + 1) };
}

function parseDevFlowClaimArgs(args) {
  const ids = [];
  let revoke = false;
  for (let index = 0; index < args.length; index++) {
    const option = splitDevFlowOption(args[index]);
    if (isDevFlowFormatOption(args[index])) continue;
    if (option.name === '--revoke') {
      if (option.inlineValue !== null) return null;
      revoke = true;
      continue;
    }
    if (option.name === '--timeout') {
      const value = option.inlineValue !== null ? option.inlineValue : args[++index];
      const timeout = Number(value);
      if (!Number.isInteger(timeout) || timeout <= 0) return null;
      continue;
    }
    if (option.name.startsWith('-')) return null;
    const parsed = normalizeDevFlowId(args[index]);
    if (!parsed) return null;
    ids.push(parsed);
  }
  return { revoke, ids };
}

function parseDevFlowResourceIds(args, expectedKind, valueOptions, multiple) {
  const ids = [];
  for (let index = 0; index < args.length; index++) {
    const raw = args[index];
    if (isDevFlowFormatOption(raw)) continue;
    const option = splitDevFlowOption(raw);
    if (valueOptions.includes(option.name)) {
      const value = option.inlineValue !== null ? option.inlineValue : args[++index];
      if (value === undefined || value === '') return null;
      continue;
    }
    if (option.name.startsWith('-')) return null;
    const parsed = normalizeDevFlowId(raw);
    if (!parsed || parsed.kind !== expectedKind) return null;
    ids.push(parsed);
  }
  if (!ids.length || (!multiple && ids.length !== 1)) return null;
  return ids;
}

function parseDevFlowBashCommand(command) {
  if (typeof command !== 'string' || !command.trim()) return null;
  const withoutStderrRedirect = command.replace(/\s+2>&1\s*$/, '').trim();
  if (!withoutStderrRedirect || /[;&`]/.test(withoutStderrRedirect) || withoutStderrRedirect.includes('$(')) {
    return null;
  }
  const stages = splitDevFlowPipeline(withoutStderrRedirect);
  if (!stages) return null;
  const tokens = tokenizeDevFlowStage(stages[stages.length - 1]);
  if (!tokens) return null;
  const words = [];
  for (let index = 0; index < tokens.length; index++) {
    if (tokens[index] === '<') {
      if (typeof tokens[index + 1] !== 'string' || !tokens[index + 1]) return null;
      index++;
    } else {
      words.push(tokens[index]);
    }
  }
  return { words, stageCount: stages.length };
}

function splitDevFlowPipeline(command) {
  const stages = [''];
  let quote = null;
  let escaped = false;
  for (const char of command) {
    if (escaped) {
      stages[stages.length - 1] += char;
      escaped = false;
      continue;
    }
    if (quote === '\'') {
      stages[stages.length - 1] += char;
      if (char === '\'') quote = null;
      continue;
    }
    if (quote === '"') {
      stages[stages.length - 1] += char;
      if (char === '"') quote = null;
      else if (char === '\\') escaped = true;
      continue;
    }
    if (char === '\\') {
      stages[stages.length - 1] += char;
      escaped = true;
    } else if (char === '\'' || char === '"') {
      stages[stages.length - 1] += char;
      quote = char;
    } else if (char === '|') {
      if (!stages[stages.length - 1].trim()) return null;
      stages.push('');
    } else {
      stages[stages.length - 1] += char;
    }
  }
  if (quote || escaped || !stages[stages.length - 1].trim()) return null;
  return stages;
}

function tokenizeDevFlowStage(stage) {
  const tokens = [];
  let word = '';
  let started = false;
  let quote = null;
  let escaped = false;
  const flush = () => {
    if (started) {
      tokens.push(word);
      word = '';
      started = false;
    }
  };
  for (const char of stage) {
    if (escaped) {
      word += char;
      started = true;
      escaped = false;
      continue;
    }
    if (quote === '\'') {
      if (char === '\'') quote = null;
      else word += char;
      started = true;
      continue;
    }
    if (quote === '"') {
      if (char === '"') quote = null;
      else if (char === '\\') escaped = true;
      else word += char;
      started = true;
      continue;
    }
    if (char === '\\') {
      escaped = true;
    } else if (char === '\'' || char === '"') {
      quote = char;
      started = true;
    } else if (char === '<') {
      flush();
      tokens.push('<');
    } else if (char === '>' || char === '&') {
      return null;
    } else if (/\s/.test(char)) {
      flush();
    } else {
      word += char;
      started = true;
    }
  }
  if (quote || escaped) return null;
  flush();
  return tokens;
}

function isDevFlowExecutable(value) {
  return value === 'dow' || /(?:^|[\\/])dow$/.test(value);
}

function parseCreatedDevFlowIds(output, expectedKind) {
  const matches = String(output || '').match(/\b(?:TASK-T|ISSUE-I)\d+\b/gi) || [];
  const ids = matches.map(normalizeDevFlowId).filter(Boolean);
  if (!ids.length || ids.some(item => item.kind !== expectedKind)) return null;
  return ids;
}

function normalizeDevFlowId(value) {
  const match = String(value || '').match(/^(TASK-T|T|ISSUE-I|I)(\d+)$/i);
  if (!match || Number(match[2]) <= 0) return null;
  const kind = match[1].toUpperCase().startsWith('I') ? 'issue' : 'task';
  const prefix = kind === 'issue' ? 'ISSUE-I' : 'TASK-T';
  const id = prefix + String(Number(match[2])).padStart(3, '0');
  return { kind, id };
}

function devFlowShortId(id) {
  return String(id).replace(/^(?:TASK|ISSUE)-/, '');
}

function formatBashDevFlowTitle(presentation) {
  const action = {
    created: 'Created',
    updated: 'Updated',
    removed: 'Removed',
    claimed: 'Claimed',
    released: 'Released',
    completed: 'Completed',
    closed: 'Closed',
    reopened: 'Reopened',
  }[presentation.action] || 'Dev-flow';
  const items = Array.isArray(presentation.items) ? presentation.items : [];
  if (!items.length && presentation.action === 'released') {
    return { name: action, arg: 'all claims' };
  }
  const arg = items.map(item => {
    const kind = item.kind === 'issue' ? 'Issue' : 'Task';
    return kind + ' ' + (item.shortId || item.id || '') + ' ' + (item.title || 'Details unavailable');
  }).join(' · ');
  return { name: action, arg };
}

function renderBashToolEvidence(toolCall, result) {
  const details = result.details || {};
  const command = toolCall.arguments && toolCall.arguments.command
    ? toolCall.arguments.command
    : '';
  const facts = [
    details.exit_code !== undefined && details.exit_code !== null ? 'exit ' + details.exit_code : 'exit unavailable',
    Number.isFinite(details.duration_ms) ? details.duration_ms + 'ms' : 'duration unavailable',
    Number.isFinite(details.timeout_ms) ? 'timeout ' + details.timeout_ms + 'ms' : 'timeout unavailable',
    details.truncated ? 'truncated' : 'not truncated',
  ];
  const deltas = Array.isArray(details.file_deltas) ? details.file_deltas : [];
  return '<div class="dev-flow-tool-command">$ ' + escapeHtml(command) + '</div>' +
    '<div class="dev-flow-tool-meta">' + facts.map(escapeHtml).join(' · ') + '</div>' +
    '<pre>' + escapeHtml(result.output || '') + '</pre>' +
    (deltas.length
      ? '<div class="dev-flow-tool-deltas"><span>File delta</span><pre>' + escapeHtml(JSON.stringify(deltas, null, 2)) + '</pre></div>'
      : '');
}

const DEV_FLOW_TITLE_CACHE_LIMIT = 64;
const DEV_FLOW_TITLE_RESPONSE_LIMIT = 2 * 1024 * 1024;
const DEV_FLOW_TITLE_TIMEOUT_MS = 1500;
const devFlowTitleCache = new Map();
const devFlowTitleRequests = new Map();
let devFlowTitleSettingsRequest = null;

function devFlowTitleCacheKey(kind, id) {
  const dashboardUrl = typeof devFlowSettings !== 'undefined'
    ? devFlowSettings?.project?.dashboardUrl || ''
    : '';
  return dashboardUrl + '|' + kind + ':' + id;
}

function devFlowTitleForItem(kind, id) {
  const key = devFlowTitleCacheKey(kind, id);
  return devFlowTitleCache.has(key) ? devFlowTitleCache.get(key) : null;
}

function rememberDevFlowTitle(key, title) {
  devFlowTitleCache.delete(key);
  devFlowTitleCache.set(key, title || null);
  while (devFlowTitleCache.size > DEV_FLOW_TITLE_CACHE_LIMIT) {
    devFlowTitleCache.delete(devFlowTitleCache.keys().next().value);
  }
}

function devFlowTitleEndpoint(kind, id) {
  const rawUrl = typeof devFlowSettings !== 'undefined'
    ? devFlowSettings?.project?.dashboardUrl
    : null;
  if (!rawUrl || !id) return null;
  try {
    const base = new URL(rawUrl);
    const loopback = ['localhost', '127.0.0.1', '::1', '[::1]'].includes(base.hostname);
    if (base.protocol !== 'http:' || !loopback) return null;
    const collection = kind === 'issue' ? 'issues' : 'tasks';
    const path = 'api/v1/' + collection + '/' + encodeURIComponent(id);
    return new URL(path, rawUrl.endsWith('/') ? rawUrl : rawUrl + '/');
  } catch (_) {
    return null;
  }
}

function ensureDevFlowTitleSettings() {
  if (typeof devFlowSettings !== 'undefined' && devFlowSettings !== null) return null;
  if (typeof invoke !== 'function') return null;
  if (!devFlowTitleSettingsRequest) {
    devFlowTitleSettingsRequest = invoke('get_dev_flow_settings')
      .then(snapshot => {
        devFlowSettings = snapshot;
        return snapshot;
      })
      .catch(() => null)
      .finally(() => {
        devFlowTitleSettingsRequest = null;
      });
  }
  return devFlowTitleSettingsRequest;
}

function fetchDevFlowTitle(endpoint) {
  if (typeof fetch !== 'function') return Promise.resolve(null);
  const controller = typeof AbortController === 'function' ? new AbortController() : null;
  const timer = controller ? setTimeout(() => controller.abort(), DEV_FLOW_TITLE_TIMEOUT_MS) : null;
  return fetch(endpoint.toString(), {
    method: 'GET',
    signal: controller ? controller.signal : undefined,
  }).then(response => {
    if (!response.ok) throw new Error('Dev Flow title lookup failed: ' + response.status);
    return response.text();
  }).then(text => {
    if (text.length > DEV_FLOW_TITLE_RESPONSE_LIMIT) throw new Error('Dev Flow title response is too large');
    const payload = JSON.parse(text);
    return typeof payload?.title === 'string' ? payload.title : null;
  }).catch(() => null).finally(() => {
    if (timer) clearTimeout(timer);
  });
}

function requestDevFlowTitles(toolCallId, presentation) {
  if (presentation.action === 'removed') return;
  const missing = presentation.items.filter(item => !item.title);
  for (const item of missing) {
    const cacheKey = devFlowTitleCacheKey(item.kind, item.id);
    if (devFlowTitleCache.has(cacheKey)) {
      item.title = devFlowTitleCache.get(cacheKey);
      continue;
    }
    const endpoint = devFlowTitleEndpoint(item.kind, item.id);
    if (!endpoint) {
      const settingsRequest = ensureDevFlowTitleSettings();
      if (settingsRequest) {
        settingsRequest.then(() => {
          if (devFlowTitleEndpoint(item.kind, item.id)) requestDevFlowTitles(toolCallId, presentation);
        });
      }
      continue;
    }
    const requestKey = endpoint.toString();
    let request = devFlowTitleRequests.get(requestKey);
    if (!request) {
      request = fetchDevFlowTitle(endpoint);
      devFlowTitleRequests.set(requestKey, request);
      request.finally(() => devFlowTitleRequests.delete(requestKey));
    }
    request.then(title => {
      rememberDevFlowTitle(cacheKey, title);
      item.title = title;
      updateDevFlowToolCard(toolCallId, presentation);
    });
  }
}

function updateDevFlowToolCard(toolCallId, presentation) {
  const card = [...document.querySelectorAll('.dev-flow-tool-call')]
    .find(element => element.dataset.toolCallId === toolCallId);
  if (!card) return;
  const title = formatBashDevFlowTitle(presentation);
  const name = card.querySelector('.tool-name');
  const arg = card.querySelector('.tool-call-args');
  if (name) name.textContent = title.name;
  if (arg) arg.textContent = title.arg;
}

function formatToolArgs(tc) {
  if (!tc.arguments) return '';
  if (typeof tc.arguments === 'string') return tc.arguments;
  // Show key fields for known tools
  const args = tc.arguments;
  const toolName = typeof tc.name === 'string' ? tc.name.toLowerCase() : '';
  if (toolName === 'bash' && args.command) return args.command;
  if (tc.name === 'Read' && args.file_path) return args.file_path;
  if (tc.name === 'Write' && args.file_path) return args.file_path;
  if (tc.name === 'Edit' && args.file_path) return args.file_path + ' (edit)';
  return JSON.stringify(args);
}

function formatToolTitle(tc) {
  const name = tc.name || 'Tool';
  const args = tc.arguments || {};
  if (typeof args === 'string') return { name, arg: args };
  if (typeof name === 'string' && name.toLowerCase() === 'bash' && args.command) {
    return { name, arg: args.command };
  }
  if (name === 'Read') return { name, arg: args.file_path || args.path || '' };
  if (name === 'Write') return { name, arg: args.file_path || args.path || '' };
  if (name === 'Edit') return { name, arg: args.file_path || args.path || '' };
  if (name === 'Find') {
    const parts = [];
    if (args.pattern) parts.push(args.pattern);
    if (args.path && args.path !== '.') parts.push(args.path);
    return { name, arg: parts.join(' ') };
  }
  if (name === 'Grep') {
    const parts = [];
    if (args.pattern) parts.push(args.pattern);
    if (args.path) parts.push(args.path);
    return { name, arg: parts.join(' ') };
  }
  return { name, arg: formatToolArgs(tc) };
}

function renderCodeView(content) {
  return content.split('\n').map((line, index) =>
    '<div class="code-line"><span class="code-ln">' + (index + 1) + '</span>' +
    '<span class="code-text">' + escapeHtml(line) + '</span></div>'
  ).join('');
}

function renderDiffView(patch) {
  let oldLine = 1;
  let newLine = 1;
  const rows = [];
  for (const line of patch.split('\n')) {
    const hunk = line.match(/^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/);
    if (hunk) {
      oldLine = Number.parseInt(hunk[1], 10);
      newLine = Number.parseInt(hunk[2], 10);
      continue;
    }
    if (line.startsWith('---') || line.startsWith('+++') || !line) continue;
    if (line.startsWith('-')) {
      rows.push('<div class="diff-line diff-del"><span class="diff-sign">−</span><span class="diff-ln">' +
        oldLine++ + '</span><span class="diff-text">' + escapeHtml(line.slice(1)) + '</span></div>');
    } else if (line.startsWith('+')) {
      rows.push('<div class="diff-line diff-add"><span class="diff-sign">+</span><span class="diff-ln">' +
        newLine++ + '</span><span class="diff-text">' + escapeHtml(line.slice(1)) + '</span></div>');
    }
  }
  return rows.join('');
}

// =============== Content Block Extraction ===============

function extractText(content) {
  if (!Array.isArray(content)) return typeof content === 'string' ? content : '';
  return content.filter(b => b.type === 'text').map(b => b.text).join('\n');
}

function extractThinking(content) {
  if (!Array.isArray(content)) return null;
  const blocks = content.filter(b => b.type === 'thinking');
  return blocks.length > 0 ? blocks.map(b => b.thinking).join('\n') : null;
}

function activeStreamMessageIndex(messages, visibleMessages, streaming) {
  if (!streaming) return -1;
  const latest = messages[messages.length - 1];
  if (!latest || latest.kind !== 'standard' || !latest.message || latest.message.role !== 'assistant') return -1;
  const content = latest.message.content || [];
  const latestBlock = content[content.length - 1];
  if (!latestBlock || (latestBlock.type !== 'text' && latestBlock.type !== 'thinking')) return -1;
  return visibleMessages.lastIndexOf(latest);
}

function thinkingStateKey(sessionId, index) {
  return String(sessionId || '') + ':' + String(index);
}

function isThinkingExpanded(sessionId, index) {
  return expandedThinkingBySession[thinkingStateKey(sessionId, index)] === true;
}

function patchStreamingThinking(container, index, raw, thinkingDurationMs) {
  const messageEl = container.children[index];
  const block = messageEl && messageEl.querySelector('.thinking-block');
  const message = raw && raw.message;
  if (!block || !message || message.role !== 'assistant') return false;
  const content = message.content || [];
  const thinking = extractThinking(content);
  const latestType = content.length ? content[content.length - 1].type : '';
  if (!thinking || latestType !== 'thinking') return false;

  const label = block.querySelector('.thinking-label');
  const duration = block.querySelector('.thinking-duration');
  const markdown = block.querySelector('.thinking-markdown');
  if (label) label.textContent = 'THINKING';
  if (duration) duration.remove();
  if (thinkingDurationMs !== null && thinkingDurationMs !== undefined) {
    const nextDuration = document.createElement('span');
    nextDuration.className = 'thinking-duration';
    nextDuration.textContent = formatThinkingDuration(thinkingDurationMs);
    const header = block.querySelector('.thinking-header');
    const chevron = block.querySelector('.thinking-chevron');
    if (header && chevron) header.insertBefore(nextDuration, chevron);
  }
  if (markdown) markdown.innerHTML = renderMarkdown(thinking);
  block.classList.add('active');
  const contentEl = block.querySelector('.thinking-content');
  if (contentEl) contentEl.setAttribute('data-stream-cursor-target', 'thinking');
  return true;
}

function attachStreamCursor(messageEl) {
  if (messageEl.querySelector('.stream-cursor')) return;
  const markedTarget = messageEl.querySelector('[data-stream-cursor-target]');
  if (!markedTarget) return;
  const cursor = document.createElement('span');
  cursor.className = 'stream-cursor';
  cursor.textContent = '▌';
  appendCursorAfterLastText(markedTarget, cursor);
}

function appendCursorAfterLastText(target, cursor) {
  const textNode = lastVisibleTextNode(target);
  if (textNode && textNode.parentNode) {
    textNode.parentNode.insertBefore(cursor, textNode.nextSibling);
    return;
  }
  target.appendChild(cursor);
}

function lastVisibleTextNode(target) {
  const walker = document.createTreeWalker(
    target,
    4,
    {
      acceptNode(node) {
        return node.nodeValue && node.nodeValue.trim() ? 1 : 3;
      },
    }
  );
  let last = null;
  let node = walker.nextNode();
  while (node) {
    last = node;
    node = walker.nextNode();
  }
  return last;
}

function updateThinkingTimings(messages, visibleMessages, activeStreamIndex, sessionId) {
  const durationsByIndex = new Map();
  visibleMessages.forEach((raw, index) => {
    const msg = raw && raw.message;
    if (!msg || msg.role !== 'assistant' || !extractThinking(msg.content || [])) return;
    const rawIndex = messages.indexOf(raw);
    const key = String(sessionId || '') + ':' + rawIndex + ':' + String(msg.timestamp || 0);
    const content = msg.content || [];
    const latest = content.length ? content[content.length - 1].type : '';
    const thinkingActive = index === activeStreamIndex && latest === 'thinking';
    if (thinkingActive) {
      if (thinkingStartTimes[key] === undefined) thinkingStartTimes[key] = Date.now();
    } else if (thinkingStartTimes[key] !== undefined && thinkingDurations[key] === undefined) {
      thinkingDurations[key] = Math.max(0, Date.now() - thinkingStartTimes[key]);
      delete thinkingStartTimes[key];
    }
    if (thinkingDurations[key] !== undefined) durationsByIndex.set(index, thinkingDurations[key]);
  });
  return index => durationsByIndex.has(index) ? durationsByIndex.get(index) : null;
}

function formatThinkingDuration(ms) {
  const seconds = Math.max(0, Math.round(ms / 1000));
  if (seconds < 60) return seconds + 's';
  const minutes = Math.floor(seconds / 60);
  const rest = seconds % 60;
  return minutes + 'm' + (rest ? ' ' + rest + 's' : '');
}

// =============== Tool Events ===============

function handleToolEvent(ev) {
  if (ev.type === 'Start') trackTool(ev.name);
}

function renderTurnActivityCard(activity) {
  const changes = activity && Array.isArray(activity.fileChanges) ? activity.fileChanges : [];
  const files = changes.length ? changes.map(change => change.path) : (activity && activity.changedFiles ? activity.changedFiles : []);
  const verification = activity && activity.verification;
  const rows = files.map(path => {
    const change = changes.find(item => item.path === path);
    const status = change ? change.status : 'modified';
    const icon = status === 'added' ? '+' : (status === 'deleted' ? '−' : '~');
    const label = status === 'added' ? 'added' : (status === 'deleted' ? 'deleted' : 'modified');
    const payload = change ? escapeHtml(JSON.stringify(change)) : '';
    return '<div class="change-entry"><div class="change-row"><span class="change-icon ' + (status === 'added' ? 'new' : 'mod') +
      '" title="' + label + '">' + icon + '</span>' +
      '<button class="change-name" aria-expanded="false" ' + (change ? 'data-turn-diff="' + payload + '" onclick="toggleTurnDiff(this)"' : '') + '>' +
      escapeHtml(path) + '</button>' +
      (change ? '<span class="change-add">+' + change.added + '</span><span class="change-del">-' + change.deleted + '</span>' : '') +
      (change ? '<span class="change-toggle">›</span>' : '') + '</div>' +
      (change ? '<div class="turn-diff-inline" hidden></div>' : '') + '</div>';
  }).join('');
  const verificationSummary = verification
    ? '<div class="changes-footer"><span class="' + (verification.success ? 'change-add' : 'change-del') + '">' +
      (verification.success ? 'Verified' : 'Verification failed') + '</span><span class="changes-runtime">' +
      escapeHtml(verification.command) +
      (verification.exitCode !== null && verification.exitCode !== undefined ? ' · exit ' + verification.exitCode : '') +
      (verification.timedOut ? ' · timed out' : '') +
      (verification.truncated ? ' · truncated' : '') +
      (verification.durationMs ? ' · ' + verification.durationMs + 'ms' : '') + '</span></div>'
    : '';
  const limitation = activity && activity.captureComplete === false
    ? '<div class="changes-footer"><span class="change-del">Diff incomplete</span><span class="changes-runtime">' +
      escapeHtml(activity.captureLimitation || 'workspace capture limit reached') + '</span></div>'
    : '';
  return '<div class="changes-card"><div class="changes-header"><span>Changes: ' + files.length + ' file' + (files.length !== 1 ? 's' : '') + '</span></div>' +
    (rows ? '<div class="changes-list">' + rows + '</div>' : '') + verificationSummary + limitation + '</div>';
}

function toggleTurnDiff(button) {
  const entry = button.closest('.change-entry');
  const panel = entry && entry.querySelector('.turn-diff-inline');
  if (!panel) return;
  const change = JSON.parse(button.dataset.turnDiff || '{}');
  const opening = panel.hidden;
  panel.hidden = !opening;
  button.setAttribute('aria-expanded', String(opening));
  entry.classList.toggle('expanded', opening);
  panel.innerHTML = opening
    ? '<div class="diff-view">' + renderDiffView(change.patch || '') + '</div>'
    : '';
}

function renderRunningMessages(queuedMessages, steeringConversation) {
  const queue = document.getElementById('queuedMessages');
  const steering = document.getElementById('steeringConversation');
  const queued = Array.isArray(queuedMessages) ? queuedMessages : [];
  const steeringMessages = Array.isArray(steeringConversation) ? steeringConversation : [];
  const queueKey = JSON.stringify(queued);
  const steeringKey = JSON.stringify(steeringMessages);

  if (queue && queueKey !== renderedQueueKey) {
    queue.hidden = queued.length === 0;
    queue.innerHTML = queued.length
      ? '<div class="running-messages-title">Queue <span>' + queued.length + '</span></div><ol>' +
        queued.map(message => '<li>' + escapeHtml(message) + '</li>').join('') + '</ol>'
      : '';
    renderedQueueKey = queueKey;
  }

  if (steering && steeringKey !== renderedSteeringKey) {
    steering.hidden = steeringMessages.length === 0;
    steering.innerHTML = steeringMessages.length
      ? '<div class="running-messages-title">Steering conversation</div><ol>' +
        steeringMessages.map(message => '<li><span>' + escapeHtml(message.text || '') +
          '</span><em>Awaiting tool result</em></li>').join('') + '</ol>'
      : '';
    renderedSteeringKey = steeringKey;
  }
}

function trackTool(name) {
  if (!name) return;
  toolCounts[name] = (toolCounts[name] || 0) + 1;
}

function renderToolChips() {
  const el = document.getElementById('toolChips');
  if (!el) return;
  const entries = Object.entries(toolCounts);
  if (entries.length === 0) {
    el.innerHTML = '<span style="font-size:10.5px;color:var(--muted)">No tool calls yet</span>';
    return;
  }
  el.innerHTML = entries
    .map(([name, count]) => '<span class="tool-chip"><span class="tool-chip-name">' +
      escapeHtml(name) + '</span><span class="tool-chip-count">' + count + '</span></span>')
    .join('');
}

function toolIcon(name) {
  if (name === 'Bash') return '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="4 5.5 1.5 8 4 10.5"/><line x1="8" y1="10" x2="13" y2="10"/></svg>';
  if (name === 'Edit') return '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M12.5 2.5a1.4 1.4 0 012 2L5 14l-3 1 1-3 9.5-9.5z"/><path d="M11 4l2 2"/></svg>';
  if (name === 'Write') return '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M12.5 2.5a1.4 1.4 0 012 2L5 14l-3 1 1-3 9.5-9.5z"/><path d="M11 4l2 2"/></svg>';
  if (name === 'Read') return '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3 2.5h6l4 4v7a1 1 0 01-1 1H3a1 1 0 01-1-1V3.5a1 1 0 011-1z"/><path d="M9 2.5v4h4"/><path d="M5 8.5h6M5 11h4"/></svg>';
  // Generic file icon for other tools
  return '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3 2.5h6l4 4v7a1 1 0 01-1 1H3a1 1 0 01-1-1V3.5a1 1 0 011-1z"/><path d="M9 2.5v4h4"/></svg>';
}

// =============== Agent Questions ===============

const QUESTION_OTHER_VALUE = '__rozsa_other__';

function showUserQuestion(ev) {
  if (!ev || !ev.sessionId || !ev.requestId || !Array.isArray(ev.questions) || !ev.questions.length) return;
  const queue = pendingUserQuestions[ev.sessionId] || [];
  queue.push(ev);
  pendingUserQuestions[ev.sessionId] = queue;
  sessionStreamingState[ev.sessionId] = 'question';
  scheduleQuestionPanelDisplay();
}

function scheduleQuestionPanelDisplay() {
  void displayQuestionPanelIfNeeded();
}

async function displayQuestionPanelIfNeeded() {
  if (questionDisplayInFlight) return;
  questionDisplayInFlight = true;
  try {
    const sessionId = activeSessionId || (sessions[activeSessionIdx] && sessions[activeSessionIdx].id);
    const queue = sessionId ? pendingUserQuestions[sessionId] : null;
    const ev = queue && queue[0];
    if (!ev) {
      hideQuestionPanel();
      return;
    }
    if (currentQuestionId !== ev.requestId || currentQuestionSessionId !== ev.sessionId) {
      currentQuestionId = ev.requestId;
      currentQuestionSessionId = ev.sessionId;
      currentQuestionIndex = 0;
      currentQuestionAnswers = {};
    }
    renderQuestionPage(ev);
    const panel = document.getElementById('questionPanel');
    if (!panel) return;
    panel.classList.add('visible');
    const input = document.getElementById('msgInput');
    if (input) input.style.display = 'none';
  } finally {
    questionDisplayInFlight = false;
  }
}

function renderQuestionPage(ev) {
  const question = ev.questions[currentQuestionIndex];
  if (!question) return;
  const title = document.getElementById('questionPanelTitle');
  const options = document.getElementById('questionPanelOptions');
  const otherInput = document.getElementById('questionPanelOtherInput');
  const error = document.getElementById('questionPanelError');
  const submit = document.getElementById('questionPanelSubmit');
  if (!title || !options || !otherInput || !error || !submit) return;

  const questionText = question.question || '';
  title.textContent = '[' + (currentQuestionIndex + 1) + '/' + ev.questions.length + '] ' + questionText;
  title.title = questionText;
  error.textContent = '';
  options.replaceChildren();
  const inputType = question.multiSelect ? 'checkbox' : 'radio';
  const name = 'question-' + ev.requestId + '-' + currentQuestionIndex;
  const addOption = (labelText, description, value, isOther = false, optionNumber) => {
    const row = document.createElement('label');
    row.className = 'question-panel-option';
    const input = document.createElement('input');
    input.type = inputType;
    input.name = name;
    input.value = value;
    input.dataset.other = isOther ? 'true' : 'false';
    input.dataset.optionNumber = String(optionNumber);
    const key = document.createElement('span');
    key.className = 'question-panel-option-key';
    key.textContent = String(optionNumber);
    const copy = document.createElement('span');
    copy.className = 'question-panel-option-copy';
    const label = document.createElement('span');
    label.className = 'question-panel-option-label';
    label.textContent = labelText;
    copy.appendChild(label);
    if (description) {
      const detail = document.createElement('span');
      detail.className = 'question-panel-option-description';
      detail.textContent = description;
      copy.appendChild(detail);
    }
    row.append(key, input, copy);
    input.addEventListener('change', () => {
      if (isOther) {
        otherInput.hidden = !input.checked;
        if (input.checked) otherInput.focus();
      } else if (!question.multiSelect) {
        otherInput.hidden = true;
        otherInput.value = '';
      }
      error.textContent = '';
    });
    options.appendChild(row);
    return input;
  };

  (Array.isArray(question.options) ? question.options : []).forEach((option, index) => {
    addOption(option.label || 'Option', option.description || '', option.label || '', false, index + 1);
  });
  const other = addOption(
    'Other',
    'Type a custom answer.',
    QUESTION_OTHER_VALUE,
    true,
    (Array.isArray(question.options) ? question.options.length : 0) + 1,
  );
  otherInput.value = '';
  otherInput.hidden = true;
  otherInput.oninput = () => {
    if (otherInput.value.trim()) {
      other.checked = true;
      otherInput.hidden = false;
    }
    error.textContent = '';
  };
  const isLastQuestion = currentQuestionIndex + 1 >= ev.questions.length;
  submit.replaceChildren();
  const submitKey = document.createElement('span');
  submitKey.className = 'question-panel-submit-key';
  submitKey.textContent = isLastQuestion ? 'D' : 'N';
  const submitLabel = document.createElement('span');
  submitLabel.textContent = isLastQuestion ? 'Done' : 'Next';
  submit.append(submitKey, submitLabel);
  submit.setAttribute('aria-label', isLastQuestion ? 'Done (D)' : 'Next (N)');
  submit.disabled = false;
  const first = options.querySelector('input');
  if (first) first.focus();
}

function activeQuestionEvent() {
  const queue = currentQuestionSessionId
    ? pendingUserQuestions[currentQuestionSessionId] || []
    : [];
  return queue[0] || null;
}

function selectQuestionOption(number) {
  const option = document.querySelector(
    '#questionPanelOptions input[data-option-number="' + number + '"]',
  );
  if (!option) return false;
  option.checked = option.type === 'checkbox' ? !option.checked : true;
  option.dispatchEvent(new Event('change', { bubbles: true }));
  return true;
}

function clearQuestionOtherInput() {
  const otherInput = document.getElementById('questionPanelOtherInput');
  const other = document.querySelector('#questionPanelOptions input[data-other="true"]');
  if (other) {
    other.checked = false;
    other.focus();
  }
  if (otherInput) {
    otherInput.value = '';
    otherInput.hidden = true;
  }
  const error = document.getElementById('questionPanelError');
  if (error) error.textContent = '';
}

function collectQuestionAnswer(question) {
  const options = document.getElementById('questionPanelOptions');
  const otherInput = document.getElementById('questionPanelOtherInput');
  const error = document.getElementById('questionPanelError');
  if (!options || !otherInput || !error) return null;
  const checked = Array.from(options.querySelectorAll('input:checked'));
  const values = [];
  let otherSelected = false;
  for (const input of checked) {
    if (input.dataset.other === 'true') {
      otherSelected = true;
    } else {
      values.push(input.value);
    }
  }
  if (otherSelected) {
    const custom = otherInput.value.trim();
    if (!custom) {
      error.textContent = 'Type your answer in the Other field.';
      otherInput.focus();
      return null;
    }
    values.push(custom);
  }
  if (!values.length) {
    error.textContent = 'Select an option or choose Other.';
    return null;
  }
  return question.multiSelect ? values : values[0];
}

async function submitUserQuestion() {
  if (!currentQuestionId || !currentQuestionSessionId) return;
  const queue = pendingUserQuestions[currentQuestionSessionId] || [];
  const ev = queue[0];
  const question = ev && ev.questions[currentQuestionIndex];
  if (!ev || !question) return;
  const answer = collectQuestionAnswer(question);
  if (answer === null) return;
  currentQuestionAnswers[question.header] = answer;
  if (currentQuestionIndex + 1 < ev.questions.length) {
    currentQuestionIndex += 1;
    renderQuestionPage(ev);
    return;
  }

  const submit = document.getElementById('questionPanelSubmit');
  if (submit) submit.disabled = true;
  try {
    await invoke('respond_user_question', {
      sessionId: currentQuestionSessionId,
      id: currentQuestionId,
      answers: currentQuestionAnswers,
    });
  } catch (e) {
    if (submit) submit.disabled = false;
    showError('User question response failed: ' + String(e));
    return;
  }
  pendingUserQuestions[currentQuestionSessionId] = queue.filter(item => item.requestId !== currentQuestionId);
  if (!pendingUserQuestions[currentQuestionSessionId].length) {
    delete pendingUserQuestions[currentQuestionSessionId];
    sessionStreamingState[currentQuestionSessionId] = 'running';
  }
  hideQuestionPanel();
  scheduleQuestionPanelDisplay();
}

function hideQuestionPanel() {
  const panel = document.getElementById('questionPanel');
  if (panel) panel.classList.remove('visible');
  const input = document.getElementById('msgInput');
  if (input) { input.style.display = ''; input.focus(); }
  currentQuestionId = null;
  currentQuestionSessionId = null;
  currentQuestionIndex = 0;
  currentQuestionAnswers = {};
}

function discardCurrentQuestionUi() {
  if (!currentQuestionSessionId) return;
  delete pendingUserQuestions[currentQuestionSessionId];
  hideQuestionPanel();
}

// =============== Permissions ===============

function showPermission(ev) {
  if (!ev.session_id) return;
  const queue = pendingPermissions[ev.session_id] || [];
  queue.push(ev);
  pendingPermissions[ev.session_id] = queue;
  sessionStreamingState[ev.session_id] = 'approval';
  schedulePermPanelDisplay();
}

function schedulePermPanelDisplay() {
  void displayPermPanelIfNeeded();
}

async function displayPermPanelIfNeeded() {
  if (permissionDisplayInFlight) return;
  permissionDisplayInFlight = true;
  try {
    while (true) {
      const currentSessionId = activeSessionId || (sessions[activeSessionIdx] && sessions[activeSessionIdx].id);
      const queue = currentSessionId ? pendingPermissions[currentSessionId] : null;
      const ev = queue && queue[0];
      if (!ev) {
        hidePermPanel();
        return;
      }
      if (ev.needsRecheck) {
        let refreshed;
        try {
          refreshed = await invoke('prepare_permission', { sessionId: ev.session_id, id: ev.request_id });
        } catch (e) {
          console.error('prepare_permission:', e);
          return;
        }
        if (!refreshed) {
          pendingPermissions[ev.session_id] = queue.filter(item => item.request_id !== ev.request_id);
          if (!pendingPermissions[ev.session_id].length) delete pendingPermissions[ev.session_id];
          continue;
        }
        ev.trust_key = refreshed.trust_key;
        ev.trust_levels = refreshed.trust_levels;
        ev.needsRecheck = false;
      }

      currentPermissionId = ev.request_id;
      currentPermissionSessionId = ev.session_id;
      const trustLevels = Array.isArray(ev.trust_levels)
        ? ev.trust_levels
        : (Array.isArray(ev.trustLevels) ? ev.trustLevels : []);
      currentPermissionTrustGroups = Array.isArray(ev.trust_groups)
        ? ev.trust_groups
        : (Array.isArray(ev.trustGroups) ? ev.trustGroups : []);
      if (!currentPermissionTrustGroups.length && trustLevels.length) {
        currentPermissionTrustGroups = [{ target: ev.command || ev.summary || ev.tool, levels: trustLevels }];
      }
      const savedPermission = permissionUiStateBySession[ev.session_id];
      const canRestore = savedPermission && savedPermission.requestId === ev.request_id;
      currentPermissionTrustIndex = canRestore ? savedPermission.trustIndex : -1;
      currentPermissionTrustKeys = canRestore ? [...savedPermission.trustKeys] : [];
      const panel = document.getElementById('permPanel');
      if (!panel) return;
      const tool = document.getElementById('permTool');
      const cmd = document.getElementById('permCmd');
      const cmdToggle = document.getElementById('permCmdToggle');
      const desc = document.getElementById('permDesc');
      if (tool) tool.textContent = ev.tool || '—';
      const command = ev.command || ev.summary || '—';
      if (cmd) {
        cmd.innerHTML = renderPermissionCommand(command, ev.tool || '');
        cmd.classList.remove('expanded');
        cmd.classList.add('collapsed');
      }
      if (cmdToggle) {
        cmdToggle.hidden = true;
        cmdToggle.setAttribute('aria-expanded', 'false');
        cmdToggle.textContent = 'Show full command';
      }
      if (desc) desc.textContent = ev.description || '';
      if (cmd && cmdToggle) {
        requestAnimationFrame(() => {
          const overflowing = cmd.scrollHeight > cmd.clientHeight + 1;
          cmdToggle.hidden = !overflowing;
        });
      }
      if (currentPermissionTrustIndex >= 0) renderPermissionTrustPage();
      else showPermissionMainPage();
      panel.classList.add('visible');
      document.getElementById('msgInput').style.display = 'none';
      focusPermissionAction(canRestore ? savedPermission.focusIndex : 0);
      capturePermissionUiState(ev.session_id);
      return;
    }
  } finally {
    permissionDisplayInFlight = false;
  }
}

function togglePermissionCommand() {
  const cmd = document.getElementById('permCmd');
  const toggle = document.getElementById('permCmdToggle');
  if (!cmd || !toggle) return;
  const expanded = cmd.classList.toggle('expanded');
  cmd.classList.toggle('collapsed', !expanded);
  toggle.setAttribute('aria-expanded', String(expanded));
  toggle.textContent = expanded ? 'Collapse' : 'Show full command';
}

function renderPermissionCommand(command, toolName) {
  if (!command) return '—';
  const isBash = toolName.toLowerCase() === 'bash';
  const prompt = isBash ? '<span class="perm-syn-prompt">$ </span>' : '';
  if (!isBash) return escapeHtml(command);
  return prompt + command
    .split(/([;\n|&]+)/)
    .map(part => {
      if (/^[;\n|&]+$/.test(part)) return escapeHtml(part);
      const match = /^(\s*)([A-Za-z_][\w.-]*)([\s\S]*)$/.exec(part);
      if (!match) return escapeHtml(part);
      const [, leading, executable, rest] = match;
      const flags = escapeHtml(rest).replace(/(^|\s)(--?[A-Za-z0-9][\w-]*)/g, '$1<span class="perm-syn-flag">$2</span>');
      return escapeHtml(leading) + '<span class="perm-syn-command">' + escapeHtml(executable) + '</span>' + flags;
    })
    .join('');
}

function showPermissionMainPage() {
  document.getElementById('permPanelContext').hidden = false;
  document.getElementById('permPanelMain').hidden = false;
  document.getElementById('permPanelTrust').hidden = true;
  document.getElementById('permPanelHint').hidden = true;
  if (currentPermissionSessionId) capturePermissionUiState(currentPermissionSessionId);
}

const PERMISSION_HINT_PREFIX = 'Deny, ';

function enterPermissionHint() {
  document.getElementById('permPanelContext').hidden = false;
  document.getElementById('permPanelMain').hidden = true;
  document.getElementById('permPanelTrust').hidden = true;
  document.getElementById('permPanelHint').hidden = false;
  const input = document.getElementById('permHintInput');
  if (!input) return;
  input.value = PERMISSION_HINT_PREFIX;
  input.focus();
  input.setSelectionRange(PERMISSION_HINT_PREFIX.length, PERMISSION_HINT_PREFIX.length);
}

function normalizePermissionHint(input) {
  if (!input) return;
  const raw = input.value || '';
  const prefixMatch = /^Deny,\s*/i.exec(raw);
  const suffix = prefixMatch ? raw.slice(prefixMatch[0].length) : raw;
  input.value = PERMISSION_HINT_PREFIX + suffix;
  const start = input.selectionStart || 0;
  const end = input.selectionEnd || 0;
  if (start < PERMISSION_HINT_PREFIX.length || end < PERMISSION_HINT_PREFIX.length) {
    const cursor = Math.max(PERMISSION_HINT_PREFIX.length, end);
    input.setSelectionRange(cursor, cursor);
  }
}

function handlePermissionHintKeydown(event) {
  const input = event.currentTarget || document.getElementById('permHintInput');
  if (!input) return;
  const start = input.selectionStart || 0;
  const end = input.selectionEnd || 0;
  if ((event.key === 'Backspace' || event.key === 'Delete') && start <= PERMISSION_HINT_PREFIX.length && end <= PERMISSION_HINT_PREFIX.length) {
    event.preventDefault();
    input.setSelectionRange(PERMISSION_HINT_PREFIX.length, PERMISSION_HINT_PREFIX.length);
    return;
  }
  if (event.key === 'Enter' && !event.shiftKey) {
    event.preventDefault();
    submitPermissionHint();
    return;
  }
  if (event.key === 'Escape') {
    event.preventDefault();
    showPermissionMainPage();
  }
}

function submitPermissionHint() {
  const input = document.getElementById('permHintInput');
  if (!input) return;
  normalizePermissionHint(input);
  const hint = input.value.slice(PERMISSION_HINT_PREFIX.length).trim();
  void respondPermission('deny-hint', hint);
}

function enterPermissionTrust() {
  if (!currentPermissionTrustGroups.length) {
    void respondPermission('allow');
    return;
  }
  currentPermissionTrustIndex = 0;
  currentPermissionTrustKeys = [];
  renderPermissionTrustPage();
  capturePermissionUiState(currentPermissionSessionId);
}

function renderPermissionTrustPage() {
  const group = currentPermissionTrustGroups[currentPermissionTrustIndex];
  if (!group) {
    void respondPermission('allow-session');
    return;
  }
  document.getElementById('permPanelContext').hidden = true;
  document.getElementById('permPanelMain').hidden = true;
  document.getElementById('permPanelTrust').hidden = false;
  document.getElementById('permPanelHint').hidden = true;
  const actions = document.getElementById('permTrustActions');
  actions.replaceChildren();
  const levels = Array.isArray(group.levels) ? group.levels : [];
  levels.forEach((level, index) => {
    const button = document.createElement('button');
    button.className = 'perm-panel-opt';
    button.innerHTML = `<span class="perm-panel-opt-key">${index + 1}</span>` +
      `<span class="perm-panel-opt-label">Trust ${escapeHtml(level.label || level.key)}</span>`;
    button.onclick = () => choosePermissionTrust(level.key);
    actions.appendChild(button);
  });
  const skip = document.createElement('button');
  skip.className = 'perm-panel-opt';
  skip.innerHTML = `<span class="perm-panel-opt-key">${levels.length + 1}</span>` +
    '<span class="perm-panel-opt-label">Skip Trust</span>';
  skip.onclick = () => choosePermissionTrust(null);
  actions.appendChild(skip);
  focusPermissionAction(0);
  capturePermissionUiState(currentPermissionSessionId);
}

function choosePermissionTrust(trustKey) {
  if (trustKey) currentPermissionTrustKeys.push(trustKey);
  currentPermissionTrustIndex += 1;
  renderPermissionTrustPage();
}

function activePermissionActions() {
  const id = currentPermissionTrustIndex >= 0 ? 'permTrustActions' : 'permPanelActions';
  const container = document.getElementById(id);
  return container ? Array.from(container.querySelectorAll('.perm-panel-opt')) : [];
}

function movePermissionSelection(delta) {
  const actions = activePermissionActions();
  if (!actions.length) return;
  const current = actions.indexOf(document.activeElement);
  const next = current < 0
    ? (delta > 0 ? 0 : actions.length - 1)
    : (current + delta + actions.length) % actions.length;
  actions[next].focus();
  capturePermissionUiState(currentPermissionSessionId);
}

function focusPermissionAction(index) {
  const actions = activePermissionActions();
  if (!actions.length) return;
  actions[Math.max(0, Math.min(index || 0, actions.length - 1))].focus();
}

function capturePermissionUiState(sessionId) {
  if (!sessionId || currentPermissionSessionId !== sessionId || !currentPermissionId) return;
  const actions = activePermissionActions();
  const focusIndex = Math.max(0, actions.indexOf(document.activeElement));
  permissionUiStateBySession[sessionId] = {
    requestId: currentPermissionId,
    trustIndex: currentPermissionTrustIndex,
    trustKeys: [...currentPermissionTrustKeys],
    focusIndex,
  };
}

function confirmPermissionSelection() {
  const actions = activePermissionActions();
  if (!actions.length) return;
  const current = actions.includes(document.activeElement) ? document.activeElement : actions[0];
  current.click();
}

async function respondPermission(choice, hint = null) {
  if (!currentPermissionId) return;
  try {
    await invoke('respond_permission', {
      sessionId: currentPermissionSessionId,
      id: currentPermissionId,
      choice: choice,
      trustKey: null,
      trustKeys: choice === 'allow-session' ? currentPermissionTrustKeys : null,
      hint: choice === 'deny-hint' ? hint || null : null,
    });
  } catch (e) {
    console.error('respond_permission:', e);
    showError('Permission response failed: ' + String(e));
    return;
  }
  // 清除该 session 的 pending permission
  if (currentPermissionSessionId) {
    delete permissionUiStateBySession[currentPermissionSessionId];
    const queue = pendingPermissions[currentPermissionSessionId] || [];
    pendingPermissions[currentPermissionSessionId] = queue.filter(ev => ev.request_id !== currentPermissionId);
    if (!pendingPermissions[currentPermissionSessionId].length) {
      delete pendingPermissions[currentPermissionSessionId];
      sessionStreamingState[currentPermissionSessionId] = 'running';
    } else {
      pendingPermissions[currentPermissionSessionId][0].needsRecheck = true;
    }
  }
  hidePermPanel();
  schedulePermPanelDisplay();
}

function hidePermPanel() {
  const panel = document.getElementById('permPanel');
  if (panel) panel.classList.remove('visible');
  const input = document.getElementById('msgInput');
  if (input) { input.style.display = ''; input.focus(); }
  currentPermissionId = null;
  currentPermissionSessionId = null;
  currentPermissionTrustGroups = [];
  currentPermissionTrustIndex = -1;
  currentPermissionTrustKeys = [];
  showPermissionMainPage();
}

// =============== Send Message & Slash Command Dispatch ===============

function getInputText(input) {
  return input ? (input.textContent || '') : '';
}

function handleInput(input, event) {
  autoResize(input);
  if (isInputComposing || event?.isComposing || event?.inputType === 'insertCompositionText') {
    updateAbortButton();
    return;
  }
  updateAutocomplete();
  updateAbortButton();
}

function handleCompositionStart() {
  isInputComposing = true;
  // Invalidate an autocomplete request that started before the IME event.
  acRequestSeq++;
  hideAutocomplete(false);
}

function handleCompositionUpdate(input) {
  isInputComposing = true;
  autoResize(input);
}

function handleCompositionEnd(input) {
  isInputComposing = false;
  autoResize(input);
  updateAbortButton();
  // Let the browser commit the final composition text before highlight DOM
  // replacement. A new composition invalidates this deferred refresh.
  setTimeout(() => {
    if (!isInputComposing) {
      updateAutocomplete();
      flushPendingGuiSceneTransition();
    }
  }, 0);
}

function setInputText(input, text) {
  if (!input) return;
  if (!text) {
    input.replaceChildren();
    return;
  }
  input.replaceChildren(document.createTextNode(text));
}

function getInputSelection(input) {
  const text = getInputText(input);
  const sel = window.getSelection();
  if (!input || !sel || sel.rangeCount === 0) return { start: text.length, end: text.length };
  const range = sel.getRangeAt(0);
  if (!input.contains(range.startContainer) || !input.contains(range.endContainer)) {
    return { start: text.length, end: text.length };
  }
  const startRange = range.cloneRange();
  startRange.selectNodeContents(input);
  startRange.setEnd(range.startContainer, range.startOffset);
  const endRange = range.cloneRange();
  endRange.selectNodeContents(input);
  endRange.setEnd(range.endContainer, range.endOffset);
  return { start: startRange.toString().length, end: endRange.toString().length };
}

function getInputCursor(input) {
  return getInputSelection(input).end;
}

function textPositionAt(input, offset) {
  const walker = document.createTreeWalker(input, NodeFilter.SHOW_TEXT);
  let remaining = Math.max(0, offset);
  let node = walker.nextNode();
  while (node) {
    const length = node.nodeValue.length;
    if (remaining <= length) return { node, offset: remaining };
    remaining -= length;
    node = walker.nextNode();
  }
  return { node: input, offset: input.childNodes.length };
}

function setInputSelection(input, start, end = start) {
  if (!input) return;
  const range = document.createRange();
  const startPos = textPositionAt(input, start);
  const endPos = textPositionAt(input, end);
  range.setStart(startPos.node, startPos.offset);
  range.setEnd(endPos.node, endPos.offset);
  const sel = window.getSelection();
  if (!sel) return;
  sel.removeAllRanges();
  sel.addRange(range);
}

function captureSessionDraft(sessionId) {
  if (!sessionId) return;
  const input = document.getElementById('msgInput');
  const mode = document.getElementById('runningSendMode');
  const selection = getInputSelection(input);
  sessionDraftState[sessionId] = {
    text: getInputText(input),
    selectionStart: selection.start,
    selectionEnd: selection.end,
    runningSendMode: mode ? mode.value : 'queue',
  };
}

function restoreSessionDraft(sessionId) {
  const input = document.getElementById('msgInput');
  if (!input) return;
  const saved = sessionDraftState[sessionId] || {
    text: '',
    selectionStart: 0,
    selectionEnd: 0,
    runningSendMode: currentSettings?.running_send_mode || 'queue',
  };
  setInputText(input, saved.text || '');
  const mode = document.getElementById('runningSendMode');
  if (mode) mode.value = saved.runningSendMode || 'queue';
  autoResize(input);
  updateInputHighlight([]);
  if (document.getElementById('permPanel')?.classList.contains('visible')) return;
  input.focus();
  setInputSelection(input, saved.selectionStart || 0, saved.selectionEnd || saved.selectionStart || 0);
  updateAbortButton();
}

async function sendMessage() {
  const input = document.getElementById('msgInput');
  if (!input) return;
  const text = getInputText(input).trim();
  if (!text) return;
  setInputText(input, '');
  input.style.height = 'auto';
  updateInputHighlight([]);
  hideAutocomplete();

  if (isStreaming) {
    const mode = document.getElementById('runningSendMode');
    try { await invoke('send_running_message', { mode: mode ? mode.value : 'queue', message: text }); }
    catch (e) { showError(String(e)); }
    updateAbortButton();
    return;
  }

  if (text.includes('/')) {
    const handled = await dispatchSlashCommand(text);
    if (handled) return;
  }

  try {
    await invoke('send_message', { message: text });
    // 发消息后刷新 session 列表（新会话首条消息会创建 .jsonl）
    if (!nativeSplitMode) {
      sessions = await invoke('get_sessions');
      renderSessionList();
    }
  } catch (e) {
    showError(String(e));
  }
}

async function dispatchSlashCommand(text) {
  try {
    const result = await invoke('dispatch_slash_command', { text });
    if (!result || !result.handled) return false;
    await handleSlashAction(result.action, result.value);
    return true;
  } catch (e) {
    showError(String(e));
    return true;
  }
}

async function handleSlashAction(action, value) {
  switch (action) {
    case 'modelPicker':
      await showModelPicker();
      return;
    case 'settings':
      toggleSettings();
      return;
    case 'help':
      showHelp(value || '');
      return;
    case 'hotkeys':
      showHotkeys();
      return;
    case 'refreshSessions':
      if (!nativeSplitMode) {
        sessions = await invoke('get_sessions');
        renderSessionList();
      }
      return;
    case 'refreshModels':
      models = await invoke('list_models');
      renderModelSelector();
      try { updateQuotaVisibility((await invoke('get_state')).model); } catch (e) { showError('get_state failed: ' + String(e)); }
      await refreshRateLimits(false);
      return;
    case 'copy':
      await copyText(value || '');
      showNotification('Copied last assistant message');
      return;
    case 'forkPicker':
      await showForkPicker();
      return;
    case 'subagentPanel':
      await showSubagentPanel();
      return;
    default:
      return;
  }
}

async function showForkPicker() {
  const panel = document.getElementById('forkPicker');
  if (!panel) return;
  try {
    const points = await invoke('get_fork_points');
    panel.hidden = false;
    panel.innerHTML = '<div class="running-messages-title">Fork conversation</div><ol>' +
      (points.length ? points.map(point => '<li><button class="turn-activity-file" onclick="forkAtMessage(' + point.messageIndex + ')">' +
        escapeHtml(point.label) + '</button></li>').join('') : '<li>No user messages available.</li>') + '</ol>';
  } catch (error) { showError(String(error)); }
}

async function forkAtMessage(messageIndex) {
  try {
    await invoke('fork_session', { messageIndex });
    sessions = await invoke('get_sessions');
    renderSessionList();
    document.getElementById('forkPicker').hidden = true;
    showNotification('Created forked session');
  } catch (error) { showError(String(error)); }
}

async function showSubagentPanel() {
  const panel = document.getElementById('subagentPanel');
  if (!panel) return;
  try {
    const subagents = await invoke('get_subagents');
    panel.hidden = false;
    panel.innerHTML = '<div class="running-messages-title">Subagents</div><ol>' +
      (subagents.length ? subagents.map(agent => '<li><strong>' + escapeHtml(agent.name) +
        '</strong> <em>' + escapeHtml(agent.status) + '</em><div class="subagent-meta">' +
        escapeHtml(agent.model_id || agent.modelId || '') + ' · ' + agent.message_count + ' messages</div></li>').join('')
        : '<li>No subagents.</li>') + '</ol>';
  } catch (error) { showError(String(error)); }
}

async function copyText(text) {
  if (!text) return;
  if (navigator.clipboard && navigator.clipboard.writeText) {
    await navigator.clipboard.writeText(text);
    return;
  }
  const ta = document.createElement('textarea');
  ta.value = text;
  ta.style.position = 'fixed';
  ta.style.opacity = '0';
  document.body.appendChild(ta);
  ta.focus();
  ta.select();
  document.execCommand('copy');
  ta.remove();
}

async function abortAgent() {
  try { await invoke('abort'); } catch (e) { console.error('abort:', e); }
  if (currentQuestionId) discardCurrentQuestionUi();
}

// =============== Session Management ===============

function renderSessionList() {
  const el = document.getElementById('sessionList');
  if (!el) return;
  const renderKey = JSON.stringify(sessions) + ':' + activeSessionIdx + ':' + JSON.stringify(sessionStreamingState);
  if (renderKey === renderedSessionListKey) return;
  renderedSessionListKey = renderKey;
  if (!sessions.length) {
    el.innerHTML = '<div style="padding:12px;font-size:11px;color:var(--muted)">No sessions</div>';
    return;
  }
  el.innerHTML = sessions.map((s, i) =>
    '<div class="session-item' + (i === activeSessionIdx ? ' active' : '') + '" data-path="' + escapeHtml(s.path) +
    '" onclick="doSwitchSession(' + i + ')">' +
    '<span class="session-status ' + (sessionStreamingState[s.id] || 'idle') + '"></span>' +
    '<div class="session-name">' + escapeHtml(s.name || s.first_message || 'Untitled') + '</div>' +
    '<div class="session-meta"><span>' + escapeHtml(formatSessionDate(s.modified || s.last_modified)) + '</span>' +
    '</div></div>'
  ).join('');
}

function showSidebarError(id, message) {
  const el = document.getElementById(id);
  if (!el) return;
  el.innerHTML = '<div style="padding:12px;font-size:11px;color:var(--danger);line-height:1.4">' +
    escapeHtml(message) + '</div>';
}

function formatSessionDate(dateStr) {
  if (!dateStr) return '';
  try {
    const date = new Date(dateStr);
    if (isNaN(date.getTime())) return '';
    const now = Date.now();
    const diffMs = now - date.getTime();
    const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));
    if (diffDays < 1) return ''; // 1天内不显示
    if (diffDays < 7) return diffDays + 'd';
    const diffWeeks = Math.floor(diffDays / 7);
    if (diffWeeks < 5) return diffWeeks + 'w';
    const diffMonths = Math.floor(diffDays / 30);
    return diffMonths + 'm';
  } catch (e) { return ''; }
}

async function doSwitchSession(idx) {
  const s = sessions[idx];
  if (!s) return;
  const chat = document.getElementById('chatMessages');
  if (chat && renderedMessageSessionId) {
    persistSessionViewState(renderedMessageSessionId, chat);
  }
  if (activeSessionId) captureSessionDraft(activeSessionId);
  if (activeSessionId) capturePermissionUiState(activeSessionId);
  activeSessionIdx = idx;
  document.querySelectorAll('.session-item').forEach(el => el.classList.remove('active'));
  const items = document.querySelectorAll('.session-item');
  if (items[idx]) items[idx].classList.add('active');
  try {
    await invoke('switch_session', { path: s.path });
    const state = await invoke('get_state');
    renderState(state);
  } catch (e) { showError(String(e)); }
  // 切换后检查该 session 是否有 pending 权限请求
  displayPermPanelIfNeeded();
}

async function newSession() {
  try {
    await invoke('new_session');
    sessions = await invoke('get_sessions');
    renderSessionList();
    const state = await invoke('get_state');
    renderState(state);
  } catch (e) { showError(String(e)); }
}

// =============== Model Management ===============

function renderModelSelector() {
  const select = document.getElementById('settingsModelSelect');
  if (select && models.length) {
    select.innerHTML = models.map(m =>
      '<option value="' + escapeHtml(m.id) + '">' + escapeHtml(m.name || m.id) +
      ' (' + escapeHtml(m.provider || '') + ')</option>'
    ).join('');
    const currentModelId = currentSettings?.model_id || document.getElementById('modelSelector')?.textContent || '';
    if (currentModelId && models.some(m => m.id === currentModelId)) select.value = currentModelId;
  }
  const smallSelect = document.getElementById('settingsSmallModelSelect');
  if (smallSelect) {
    smallSelect.innerHTML = '<option value="">Disabled</option>' + models.map(model =>
      '<option value="' + escapeHtml(model.id) + '">' + escapeHtml(model.name || model.id) +
      ' (' + escapeHtml(model.provider || '') + ')</option>'
    ).join('');
    smallSelect.value = currentSettings?.small_model || '';
    smallSelect.onchange = () => saveSetting('small_model', smallSelect.value);
  }
  renderThinkingEffortPicker();
}

async function onModelChange(modelId) {
  try {
    await invoke('switch_model', { modelId: modelId });
    let state = await invoke('get_state');
    renderState(state);
    currentSettings = await invoke('get_settings');
    const model = models.find(entry => entry.id === modelId);
    if (model && !model.reasoning && state.thinkingEffort !== 'off') {
      await saveSetting('thinking', 'off');
      state = await invoke('get_state');
      renderState(state);
    }
    renderSettingsPane(currentSettings);
    const btn = document.getElementById('modelSelector');
    if (btn) btn.textContent = modelId;
    renderThinkingEffortPicker(state.thinkingEffort);
  } catch (e) { showError(String(e)); }
}

async function showModelPicker() {
  if (!models.length) {
    try {
      models = await invoke('list_models');
      renderModelSelector();
    } catch (e) {
      showError('list_models failed: ' + String(e));
      return;
    }
  }
  if (!models.length) {
    showError('No models available');
    return;
  }
  // 使用 autocomplete popup 显示模型列表
  const popup = document.getElementById('autocomplete');
  if (!popup) return;
  popup.innerHTML = models.map((m, i) =>
    '<div class="ac-item" onmousedown="selectModel(' + i + ')">' +
    '<div class="ac-cmd">' + escapeHtml(m.name || m.id) + '</div>' +
    '<div class="ac-desc">' + escapeHtml(m.provider || '') + '</div></div>'
  ).join('');
  popup.classList.add('visible');
}

function selectModel(idx) {
  const m = models[idx];
  if (!m) return;
  hideAutocomplete();
  onModelChange(m.id);
}

function thinkingModel() {
  const modelId = document.getElementById('modelSelector')?.textContent?.trim()
    || currentSettings?.model_id
    || '';
  return models.find(model => model.id === modelId);
}

function supportedThinkingEfforts() {
  const model = thinkingModel();
  if (!model || !model.reasoning) return THINKING_EFFORT_OPTIONS.slice(0, 1);
  const unavailable = model.thinkingEffortMap || {};
  return THINKING_EFFORT_OPTIONS.filter(option =>
    option.value === 'off' || unavailable[option.value] !== null
  );
}

function normalizeThinkingEffort(options, requested) {
  if (!options.length) return THINKING_EFFORT_OPTIONS[0];
  const requestedIndex = THINKING_EFFORT_OPTIONS.findIndex(option => option.value === requested);
  const rank = requestedIndex < 0 ? 0 : requestedIndex;
  for (let index = options.length - 1; index >= 0; index -= 1) {
    const optionRank = THINKING_EFFORT_OPTIONS.findIndex(option => option.value === options[index].value);
    if (optionRank <= rank) return options[index];
  }
  return options[0];
}

function renderThinkingEffortPicker(effort) {
  const popover = document.getElementById('thinkingEffortPopover');
  const slider = document.getElementById('thinkingEffortSlider');
  const value = document.getElementById('thinkingEffortPickerValue');
  const title = document.getElementById('thinkingEffortPickerTitle');
  const marks = document.getElementById('thinkingEffortMarks');
  if (!popover || !slider || !value || !title || !marks) return;

  const options = supportedThinkingEfforts();
  const requested = String(
    effort
      || currentSettings?.thinkingEffort
      || document.getElementById('thinkingEffort')?.textContent
      || 'off'
  ).toLowerCase();
  const selected = normalizeThinkingEffort(options, requested);
  const selectedIndex = Math.max(0, options.indexOf(selected));
  const progress = options.length > 1 ? (selectedIndex / (options.length - 1)) * 100 : 0;

  title.textContent = options.length > 1 ? 'Thinking effort' : 'Thinking unavailable for this model';
  value.textContent = selected.label;
  slider.min = '0';
  slider.max = String(Math.max(0, options.length - 1));
  slider.value = String(selectedIndex);
  slider.disabled = options.length === 1;
  slider.setAttribute('aria-valuetext', selected.label);
  slider.style.setProperty('--thinking-progress', progress + '%');
  marks.innerHTML = options.map((option, index) =>
    '<span class="thinking-level-mark' + (index === selectedIndex ? ' active' : '') +
    '" title="' + escapeHtml(option.label) + '"></span>'
  ).join('');
  if (!popover.hidden) positionThinkingEffortPopover();
}

function positionThinkingEffortPopover() {
  const popover = document.getElementById('thinkingEffortPopover');
  const trigger = document.getElementById('thinkingEffort');
  if (!popover || !trigger || popover.hidden) return;
  const triggerRect = trigger.getBoundingClientRect();
  const margin = 16;
  const gap = 12;
  if (triggerRect.left >= window.innerWidth - triggerRect.right) {
    popover.style.left = 'auto';
    popover.style.right = Math.round(Math.max(margin, window.innerWidth - triggerRect.right)) + 'px';
  } else {
    popover.style.right = 'auto';
    popover.style.left = Math.round(Math.max(margin, triggerRect.left)) + 'px';
  }
  if (triggerRect.top - margin >= window.innerHeight - triggerRect.bottom - margin) {
    popover.style.top = 'auto';
    popover.style.bottom = Math.round(Math.max(margin, window.innerHeight - triggerRect.top + gap)) + 'px';
  } else {
    popover.style.bottom = 'auto';
    popover.style.top = Math.round(Math.max(margin, triggerRect.bottom + gap)) + 'px';
  }
}

function toggleThinkingEffortPicker(event) {
  event?.stopPropagation();
  const popover = document.getElementById('thinkingEffortPopover');
  const trigger = document.getElementById('thinkingEffort');
  if (!popover || !trigger) return;
  const opening = popover.hidden;
  dismissTransientPopups();
  if (!opening) return;
  renderThinkingEffortPicker();
  popover.hidden = false;
  positionThinkingEffortPopover();
  trigger.setAttribute('aria-expanded', 'true');
  document.getElementById('thinkingEffortSlider')?.focus();
}

function hideThinkingEffortPicker() {
  const popover = document.getElementById('thinkingEffortPopover');
  if (popover) popover.hidden = true;
  document.getElementById('thinkingEffort')?.setAttribute('aria-expanded', 'false');
}

function previewThinkingEffort(index) {
  const options = supportedThinkingEfforts();
  const selectedIndex = Math.max(0, Math.min(options.length - 1, Number(index)));
  const option = options[selectedIndex];
  const slider = document.getElementById('thinkingEffortSlider');
  if (!option || !slider) return;
  const progress = options.length > 1 ? (selectedIndex / (options.length - 1)) * 100 : 0;
  document.getElementById('thinkingEffortPickerValue').textContent = option.label;
  slider.setAttribute('aria-valuetext', option.label);
  slider.style.setProperty('--thinking-progress', progress + '%');
  document.querySelectorAll('#thinkingEffortMarks .thinking-level-mark').forEach((mark, markIndex) => {
    mark.classList.toggle('active', markIndex === selectedIndex);
  });
}

async function selectThinkingEffort(index) {
  const options = supportedThinkingEfforts();
  const selectedIndex = Math.max(0, Math.min(options.length - 1, Number(index)));
  const option = options[selectedIndex];
  if (!option) return;
  previewThinkingEffort(selectedIndex);
  if (await saveSetting('thinking', option.value)) {
    const badge = document.getElementById('thinkingEffort');
    if (badge) badge.textContent = option.value;
    renderThinkingEffortPicker(option.value);
  }
}

// =============== Settings Panel ===============

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
    renderSettingsSelection(snapshot.selectedPane || 'skills');
    loadSettings().catch(() => {});
    loadKeyBindings().catch(() => {});
  } else if (returningMain) {
    requestAnimationFrame(restoreMainSceneContinuity);
  }
}

async function requestGuiScene(scene, selectedPane = null, allowRetry = true) {
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
  const shouldAutoCollapse = window.innerWidth <= 1100;
  if (shouldAutoCollapse !== sidebarAutoCollapsed) {
    sidebarAutoCollapsed = shouldAutoCollapse;
    updateSidebarLayout(isSidebarCollapsed());
  } else {
    updateSidebarToggleButtons(isSidebarCollapsed());
    syncChromeBackgroundGeometry();
  }
}

function sidebarChromeBoundary(element, collapsed) {
  if (!element || collapsed) return 0;
  // getBoundingClientRect() is in the post-zoom viewport coordinate space,
  // while gradients and CSS variables are resolved in layout CSS pixels.
  // offsetLeft/offsetWidth stay in that layout space and follow the actual
  // sidebar width after the responsive grid has resolved.
  return Math.max(0, element.offsetLeft + element.offsetWidth);
}

function syncChromeBackgroundGeometry() {
  if (nativeSplitMode) return;
  const root = document.documentElement;
  const collapsed = isSidebarCollapsed();
  root.style.setProperty(
    '--chrome-sidebar-boundary',
    `${sidebarChromeBoundary(document.querySelector('[data-od-id="sidebar"]'), collapsed)}px`
  );
  root.style.setProperty(
    '--settings-chrome-sidebar-boundary',
    `${sidebarChromeBoundary(document.querySelector('.settings-tabs'), collapsed)}px`
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
}

function currentTauriWindow() {
  return window.__TAURI__?.window?.getCurrentWindow?.();
}

async function syncNativeFullscreen(source) {
  if (nativeFullscreenTransitioning) {
    console.debug('[rozsa-gui][fullscreen] calibration suppressed during transition', source);
    return;
  }
  const nativeWindow = currentTauriWindow();
  if (!nativeWindow?.isFullscreen) {
    console.error('[rozsa-gui][fullscreen] isFullscreen unavailable', source);
    return;
  }
  const fullscreen = await nativeWindow.isFullscreen();
  console.debug('[rozsa-gui][fullscreen] calibrated', source, fullscreen);
  setNativeFullscreen(fullscreen);
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
    void requestGuiScene(scene, scene === 'settings' ? 'skills' : null)
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
    loadSettings().catch(() => {});
    loadKeyBindings().catch(() => {});
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
  if (nativeSplitMode) {
    void requestGuiScene('settings', tabId)
      .catch(error => showError('Failed to switch Settings pane: ' + String(error)));
    return;
  }
  renderSettingsSelection(tabId, btn);
}

function renderSettingsSelection(tabId, btn = null) {
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
  control.onclick = () => {
    const enabled = !isSettingSwitchOn(control);
    setSettingSwitch(control, enabled);
    onChange(enabled);
  };
}

async function loadSettings() {
  try {
    [currentSettings, availableThemes, capabilitySettings, permissionSettings] = await Promise.all([
      invoke('get_settings'),
      invoke('list_themes'),
      invoke('get_capability_settings'),
      invoke('get_permission_settings'),
    ]);
    renderSettingsPane(currentSettings);
    renderCapabilitySettings();
    renderPermissionSettings();
    loadDevFlowSettings();
    await applySelectedTheme();
  } catch (e) {
    console.warn('settings:', e);
    currentSettings = {};
    availableThemes = [];
    capabilitySettings = null;
    permissionSettings = null;
    devFlowSettings = null;
    showError('Failed to load settings: ' + String(e));
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
  const s = devFlowSettings;
  const enabled = document.getElementById('devFlowEnabled');
  const sidebarStatus = document.getElementById('devFlowSidebarStatus');
  const dashboardButton = document.getElementById('devFlowDashboardButton');
  const dashboardAddress = document.getElementById('devFlowDashboardAddress');
  const pathInput = document.getElementById('devFlowExecutablePath');
  const pickExecutable = document.getElementById('devFlowPickExecutable');
  if (!s) return;
  setSettingSwitch(enabled, s.enabled);
  setSettingSwitch(sidebarStatus, s.showSidebarStatus);
  setSettingSwitch(dashboardButton, s.showDashboardButton);
  setDevFlowText('devFlowVersion', s.cli.available ? (s.cli.version || 'unknown') : 'unknown');
  const availability = !s.cli.available
    ? 'Unavailable'
    : s.project
      ? s.project.availability.charAt(0).toUpperCase() + s.project.availability.slice(1)
      : 'No active project';
  setDevFlowText('devFlowDashboardAvailability', availability);
  const statusElement = document.getElementById('devFlowDashboardStatus');
  if (statusElement) statusElement.classList.toggle('is-ready', availability.toLowerCase() === 'ready');
  const dashboardUrl = s.project?.dashboardUrl || '';
  setDevFlowText('devFlowDashboardAddressText', dashboardUrl.replace(/^https?:\/\//, '') || 'Unavailable');
  if (dashboardAddress) {
    dashboardAddress.disabled = !dashboardUrl;
    dashboardAddress.title = dashboardUrl ? 'Open Dev Flow dashboard' : 'Dashboard unavailable';
  }
  const memoryParts = formatDevFlowMemory(s.project?.memoryUseBytes).split(' ');
  setDevFlowText('devFlowMemoryAmount', memoryParts[0] || 'Unavailable');
  setDevFlowText('devFlowMemoryUnit', memoryParts[1] || '');
  if (pathInput) pathInput.value = s.executablePath || s.cli.executable || 'Not detected';
  const dependentDisabled = devFlowDependentControlsDisabled(s);
  // The master switch must remain available even when the disabled runtime has
  // not performed CLI discovery yet; enabling it is what triggers discovery.
  setDevFlowDependentControlDisabled(enabled, false);
  setDevFlowDependentControlDisabled(sidebarStatus, dependentDisabled);
  setDevFlowDependentControlDisabled(dashboardButton, dependentDisabled);
  setDevFlowDependentControlDisabled(pickExecutable, false);
  setDevFlowDependentControlDisabled(pathInput, false);
  const missing = document.getElementById('devFlowMissing');
  if (missing) missing.hidden = s.cli.available;
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

function renderCapabilitySettings() {
  if (!capabilitySettings) return;
  renderCapabilityPane('tools', capabilitySettings.tools || []);
  const skills = capabilityScope.skills === 'project'
    ? mergeCapabilityItems(
        capabilitySettings.globalSkills || [],
        capabilitySettings.projectSkills || []
      )
    : capabilitySettings.globalSkills || [];
  renderCapabilityPane('skills', skills);
}

function mergeCapabilityItems(base, overlay) {
  const merged = new Map(base.map(item => [item.name, item]));
  for (const item of overlay) merged.set(item.name, item);
  return [...merged.values()].sort((left, right) => left.label.localeCompare(right.label));
}

function renderCapabilityPane(kind, items) {
  const scope = capabilityScope[kind];
  const scopeHost = document.getElementById(kind + 'Scope');
  const list = document.getElementById(kind === 'tools' ? 'settingsToolList' : 'settingsSkillList');
  if (!scopeHost || !list) return;
  scopeHost.replaceChildren();
  for (const candidate of ['global', 'project']) {
    const button = document.createElement('button');
    button.type = 'button';
    button.textContent = candidate === 'global' ? 'Global' : 'Project';
    button.classList.toggle('active', scope === candidate);
    button.onclick = () => {
      capabilityScope[kind] = candidate;
      renderCapabilitySettings();
    };
    scopeHost.appendChild(button);
  }

  list.replaceChildren();
  if (!items.length) {
    const empty = document.createElement('p');
    empty.className = 'capability-description';
    empty.textContent = kind === 'skills' ? 'No skills found in this scope.' : 'No tools registered.';
    list.appendChild(empty);
    return;
  }
  for (const item of items) {
    const row = document.createElement('div');
    row.className = 'setting-item capability-row';
    const copy = document.createElement('div');
    copy.className = 'capability-copy';
    copy.innerHTML = `<div class="capability-name">${escapeHtml(item.label)}</div>` +
      `<div class="capability-description">${escapeHtml(item.description || item.name)}</div>`;
    const override = scope === 'global' ? item.globalOverride : item.projectOverride;
    const controls = document.createElement('div');
    controls.className = 'capability-controls';
    if (override == null) {
      const inherited = document.createElement('span');
      inherited.className = 'capability-inherited';
      inherited.textContent = scope === 'project' ? 'Inherited' : 'Default';
      controls.appendChild(inherited);
    } else {
      const reset = document.createElement('button');
      reset.type = 'button';
      reset.className = 'capability-reset';
      reset.textContent = 'Reset';
      reset.title = scope === 'project' ? 'Restore global inheritance' : 'Restore default';
      reset.onclick = async () => {
        try {
          capabilitySettings = await invoke('update_capability_setting', {
            kind, scope, name: item.name, enabled: null,
          });
          renderCapabilitySettings();
        } catch (error) {
          showError(`Failed to update ${kind}: ${String(error)}`);
        }
      };
      controls.appendChild(reset);
    }
    const toggle = document.createElement('button');
    toggle.type = 'button';
    toggle.className = 'setting-toggle';
    toggle.setAttribute('role', 'switch');
    toggle.setAttribute('aria-label', `${item.label} ${scope}`);
    setSettingSwitch(toggle, item.effective);
    toggle.onclick = async () => {
      try {
        capabilitySettings = await invoke('update_capability_setting', {
          kind, scope, name: item.name, enabled: !item.effective,
        });
        renderCapabilitySettings();
      } catch (error) {
        showError(`Failed to update ${kind}: ${String(error)}`);
        renderCapabilitySettings();
      }
    };
    controls.appendChild(toggle);
    row.append(copy, controls);
    list.appendChild(row);
  }
}

function permissionLayerRules(kind) {
  if (!permissionSettings) return null;
  return permissionSettings[`${permissionScope}${kind[0].toUpperCase()}${kind.slice(1)}`];
}

function effectivePermissionRules(kind) {
  return permissionSettings?.[`effective${kind[0].toUpperCase()}${kind.slice(1)}`] || [];
}

function defaultPermissionRules(kind) {
  return permissionSettings?.[`default${kind[0].toUpperCase()}${kind.slice(1)}`] || [];
}

function displayedPermissionRules(kind) {
  const local = permissionLayerRules(kind);
  if (local != null) return [...local];
  return permissionScope === 'global'
    ? [...defaultPermissionRules(kind)]
    : [...effectivePermissionRules(kind)];
}

function renderPermissionSettings() {
  if (!permissionSettings) return;
  const scopeHost = document.getElementById('permissionScope');
  if (scopeHost) {
    scopeHost.replaceChildren();
    for (const candidate of ['global', 'project']) {
      const button = document.createElement('button');
      button.type = 'button';
      button.textContent = candidate === 'global' ? 'Global' : 'Project';
      button.classList.toggle('active', permissionScope === candidate);
      button.onclick = () => {
        permissionScope = candidate;
        renderPermissionSettings();
      };
      scopeHost.appendChild(button);
    }
  }

  const mode = document.getElementById('settingsPermMode');
  if (mode) {
    mode.innerHTML =
      '<option value="on-request">On request</option>' +
      '<option value="auto-approve">Auto approve (not implemented)</option>' +
      '<option value="yolo">Yolo</option>';
    mode.value = permissionScope === 'global'
      ? permissionSettings.globalEffectiveMode
      : permissionSettings.effectiveMode;
    mode.title = `Effective: ${permissionSettings.effectiveMode}`;
    mode.onchange = async () => {
      try {
        permissionSettings = await invoke('update_permission_mode', {
          scope: permissionScope, mode: mode.value,
        });
        setPermissionSettingsError('');
      } catch (error) {
        setPermissionSettingsError(String(error));
      }
      renderPermissionSettings();
    };
  }

  for (const kind of ['deny', 'ask', 'allow']) renderPermissionRuleList(kind);
}

function setPermissionSettingsError(message) {
  const error = document.getElementById('permissionSettingsError');
  if (!error) return;
  error.textContent = message;
  error.hidden = !message;
}

function renderPermissionRuleList(kind) {
  const host = document.getElementById(`permission${kind[0].toUpperCase()}${kind.slice(1)}List`);
  if (!host) return;
  host.replaceChildren();
  host.dataset.permissionKind = kind;
  const localRules = permissionLayerRules(kind);
  const inherited = localRules == null;
  const rows = displayedPermissionRules(kind);
  if (!rows.length) {
    const empty = document.createElement('div');
    empty.className = 'permission-rule-empty';
    empty.textContent = 'No rules configured';
    host.appendChild(empty);
  }
  for (const rule of rows) {
    const row = document.createElement('div');
    row.className = 'permission-rule-row';
    wirePermissionRulePointerDrag(row, kind, rule);
    const copy = document.createElement('div');
    copy.className = 'permission-rule-copy';
    copy.textContent = rule;
    row.appendChild(copy);
    if (inherited) {
      const badge = document.createElement('span');
      badge.className = 'capability-inherited';
      badge.textContent = permissionScope === 'global' ? 'Default' : 'Inherited';
      row.appendChild(badge);
    }
    const remove = document.createElement('button');
    remove.type = 'button';
    remove.className = 'permission-rule-delete';
    remove.setAttribute('aria-label', `Delete ${rule}`);
    remove.textContent = 'Delete';
    remove.onclick = () => removePermissionRule(kind, rule);
    row.appendChild(remove);
    host.appendChild(row);
  }
  const reset = document.getElementById(`permission${kind[0].toUpperCase()}${kind.slice(1)}Reset`);
  if (reset) {
    reset.hidden = localRules == null;
    reset.textContent = permissionScope === 'global' ? 'Restore defaults' : 'Restore inheritance';
  }
}

function wirePermissionRulePointerDrag(row, kind, rule) {
  let origin = null;
  let dragging = false;
  row.onpointerdown = event => {
    if (event.button !== 0 || event.target.closest('button')) return;
    event.preventDefault();
    origin = { x: event.clientX, y: event.clientY, pointerId: event.pointerId };
    row.setPointerCapture(event.pointerId);
  };
  row.onpointermove = event => {
    if (!origin || origin.pointerId !== event.pointerId) return;
    if (!dragging && Math.hypot(event.clientX - origin.x, event.clientY - origin.y) < 5) return;
    dragging = true;
    row.classList.add('permission-rule-dragging');
    document.querySelectorAll('.permission-rule-drop-target')
      .forEach(element => element.classList.remove('permission-rule-drop-target'));
    const target = document.elementFromPoint(event.clientX, event.clientY)
      ?.closest('.permission-rule-list');
    if (target && target !== row.parentElement) target.classList.add('permission-rule-drop-target');
  };
  const finish = event => {
    if (!origin || origin.pointerId !== event.pointerId) return;
    const target = document.elementFromPoint(event.clientX, event.clientY)
      ?.closest('.permission-rule-list');
    const targetKind = target?.dataset.permissionKind;
    if (dragging && targetKind && targetKind !== kind) {
      void movePermissionRule(kind, targetKind, rule);
    }
    origin = null;
    dragging = false;
    row.classList.remove('permission-rule-dragging');
    document.querySelectorAll('.permission-rule-drop-target')
      .forEach(element => element.classList.remove('permission-rule-drop-target'));
  };
  row.onpointerup = finish;
  row.onpointercancel = finish;
}

function openPermissionRuleEditor(kind) {
  closePermissionRuleEditor();
  pendingPermissionRuleKind = kind;
  const host = document.getElementById(`permission${kind[0].toUpperCase()}${kind.slice(1)}List`);
  const template = document.getElementById('permissionRuleEditorTemplate');
  if (!host || !template) return;
  host.appendChild(template.content.cloneNode(true));
  const tool = document.getElementById('permissionRuleTool');
  if (!tool) return;
  setupPermissionToolCombobox(capabilitySettings?.tools || []);
  const regexp = document.getElementById('permissionRuleRegexp');
  setSettingSwitch(regexp, false);
  regexp.onclick = () => {
    setSettingSwitch(regexp, !isSettingSwitchOn(regexp));
    highlightPermissionRulePattern();
    updatePermissionRuleHint();
  };
  const input = document.getElementById('permissionRuleTarget');
  input.oninput = highlightPermissionRulePattern;
  input.onkeydown = event => {
    if (event.key === 'Enter') event.preventDefault();
  };
  input.onpaste = event => {
    event.preventDefault();
    document.execCommand('insertText', false, event.clipboardData.getData('text/plain').replace(/[\r\n]+/g, ' '));
  };
  updatePermissionRuleHint();
  tool.focus();
}

function closePermissionRuleEditor() {
  const editor = document.getElementById('permissionRuleEditor');
  if (editor) editor.remove();
}

function isPathPermissionTool(tool) {
  return ['read', 'write', 'edit'].includes(String(tool || '').toLowerCase());
}

function setupPermissionToolCombobox(tools) {
  const input = document.getElementById('permissionRuleTool');
  const button = document.getElementById('permissionRuleToolButton');
  const combobox = input?.closest('.permission-tool-combobox');
  if (!input || !button || !combobox) return;
  permissionToolOptions = tools.map(item => ({ name: item.name, label: item.label }));
  permissionToolActiveIndex = -1;
  input.oninput = () => {
    renderPermissionToolOptions(true);
    updatePermissionRuleHint();
  };
  input.onfocus = () => renderPermissionToolOptions(true, true);
  input.onclick = () => renderPermissionToolOptions(true);
  input.onkeydown = event => {
    const matches = matchingPermissionTools();
    if (event.key === 'Tab' && input.value && matches.length) {
      event.preventDefault();
      selectPermissionTool(matches[0]);
      document.getElementById('permissionRuleTarget')?.focus();
      return;
    }
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      if (!matches.length) return;
      const direction = event.key === 'ArrowDown' ? 1 : -1;
      permissionToolActiveIndex =
        (permissionToolActiveIndex + direction + matches.length) % matches.length;
      renderPermissionToolOptions(true);
      return;
    }
    if (event.key === 'Enter' && permissionToolActiveIndex >= 0 && matches[permissionToolActiveIndex]) {
      event.preventDefault();
      selectPermissionTool(matches[permissionToolActiveIndex]);
      document.getElementById('permissionRuleTarget')?.focus();
      return;
    }
    if (event.key === 'Escape') closePermissionToolOptions();
  };
  button.onclick = () => {
    const list = document.getElementById('permissionRuleToolList');
    if (list?.hidden) {
      renderPermissionToolOptions(true, true);
    } else {
      closePermissionToolOptions();
    }
  };
  combobox.onfocusout = event => {
    if (!combobox.contains(event.relatedTarget)) {
      setTimeout(closePermissionToolOptions, 100);
    }
  };
  renderPermissionToolOptions(false, true);
}

function matchingPermissionTools(showAll = false) {
  const query = document.getElementById('permissionRuleTool')?.value.trim().toLowerCase() || '';
  if (showAll || !query) return permissionToolOptions;
  return permissionToolOptions.filter(item =>
    item.name.toLowerCase().startsWith(query) || item.label.toLowerCase().startsWith(query)
  );
}

function renderPermissionToolOptions(open, showAll = false) {
  const input = document.getElementById('permissionRuleTool');
  const list = document.getElementById('permissionRuleToolList');
  if (!input || !list) return;
  const matches = matchingPermissionTools(showAll);
  if (permissionToolActiveIndex >= matches.length) permissionToolActiveIndex = -1;
  list.replaceChildren();
  matches.forEach((item, index) => {
    const option = document.createElement('div');
    option.className = 'permission-tool-option';
    option.classList.toggle('active', index === permissionToolActiveIndex);
    option.setAttribute('role', 'option');
    option.textContent = item.label;
    option.onmousedown = event => {
      event.preventDefault();
      selectPermissionTool(item);
    };
    list.appendChild(option);
  });
  list.hidden = !open || matches.length === 0;
  input.setAttribute('aria-expanded', String(!list.hidden));
}

function closePermissionToolOptions() {
  const input = document.getElementById('permissionRuleTool');
  const list = document.getElementById('permissionRuleToolList');
  if (list) list.hidden = true;
  if (input) input.setAttribute('aria-expanded', 'false');
  permissionToolActiveIndex = -1;
}

function selectPermissionTool(item) {
  const input = document.getElementById('permissionRuleTool');
  if (!input) return;
  input.value = item.label;
  closePermissionToolOptions();
  updatePermissionRuleHint();
}

function updatePermissionRuleHint() {
  const tool = document.getElementById('permissionRuleTool')?.value || '';
  const input = document.getElementById('permissionRuleTarget');
  const hint = document.getElementById('permissionRuleHint');
  const prefix = document.getElementById('permissionRulePathPrefix');
  const regexp = isSettingSwitchOn(document.getElementById('permissionRuleRegexp'));
  if (!input || !hint) return;
  if (prefix) prefix.hidden = permissionScope !== 'global' || !isPathPermissionTool(tool);
  if (isPathPermissionTool(tool)) {
    input.dataset.placeholder = permissionScope === 'global'
      ? 'path/**/*.md'
      : 'docs/**/*.md';
    hint.textContent = permissionScope === 'global'
      ? `$HOME/ is fixed. ${regexp ? 'The regular expression applies only to the Home-relative suffix and must match it fully.' : '* matches one path segment; ** matches recursively.'}`
      : `${regexp ? 'The regular expression must fully match the normalized project-relative path.' : '* matches one path segment; ** matches recursively.'}`;
  } else {
    input.dataset.placeholder = regexp ? '^cargo (test|check)( .*)?$' : '*';
    hint.textContent = regexp
      ? 'The regular expression must fully match each command segment or tool target.'
      : 'Use * for every invocation, or include * inside a command pattern.';
  }
}

function highlightPermissionRulePattern() {
  const input = document.getElementById('permissionRuleTarget');
  if (!input) return;
  const text = getInputText(input);
  const regexp = isSettingSwitchOn(document.getElementById('permissionRuleRegexp'));
  const special = regexp ? /[\\^$.*+?()[\]{}|]/g : /\*/g;
  const ranges = [];
  for (const match of text.matchAll(special)) {
    ranges.push({ start: match.index, end: match.index + match[0].length });
  }
  renderRichInputHighlights(input, ranges);
}

async function savePermissionRule() {
  const toolInput = document.getElementById('permissionRuleTool')?.value.trim() || '';
  const matchedTool = permissionToolOptions.find(item =>
    item.name.toLowerCase() === toolInput.toLowerCase()
      || item.label.toLowerCase() === toolInput.toLowerCase()
  );
  const tool = matchedTool?.name;
  const input = document.getElementById('permissionRuleTarget');
  const rawTarget = input ? getInputText(input).trim() : '';
  const regexp = isSettingSwitchOn(document.getElementById('permissionRuleRegexp'));
  if (!tool) {
    setPermissionSettingsError('Choose a tool from the suggestions before adding the rule.');
    return;
  }
  let target = rawTarget || '*';
  if (!regexp && permissionScope === 'global' && isPathPermissionTool(tool)) {
    target = `$HOME/${target.replace(/^\/+/, '')}`;
  }
  if (regexp) {
    if (!rawTarget) {
      setPermissionSettingsError('Enter a regular expression before adding the rule.');
      return;
    }
    if (permissionScope === 'global' && isPathPermissionTool(tool)) {
      target = `$HOME/regex:${rawTarget.replace(/^\/+/, '')}`;
    } else {
      target = `regex:${rawTarget}`;
    }
  }
  const rule = `${tool}(${target})`;
  if (rule === '*(*)') {
    setPermissionSettingsError('*(*) is not allowed. Use yolo mode to allow every tool.');
    return;
  }
  const local = displayedPermissionRules(pendingPermissionRuleKind);
  if (local.includes(rule)) {
    closePermissionRuleEditor();
    return;
  }
  await replacePermissionRules(pendingPermissionRuleKind, [...local, rule]);
  closePermissionRuleEditor();
}

async function removePermissionRule(kind, rule) {
  const local = displayedPermissionRules(kind);
  await replacePermissionRules(kind, local.filter(candidate => candidate !== rule));
}

async function resetPermissionRules(kind) {
  await replacePermissionRules(kind, null);
}

async function replacePermissionRules(kind, rules) {
  try {
    permissionSettings = await invoke('update_permission_rules', {
      scope: permissionScope, kind, rules,
    });
    setPermissionSettingsError('');
    renderPermissionSettings();
  } catch (error) {
    setPermissionSettingsError(`Failed to update permission rules: ${String(error)}`);
  }
}

async function movePermissionRule(fromKind, toKind, rule) {
  if (fromKind === toKind) return;
  const ruleSet = Object.fromEntries(
    ['deny', 'ask', 'allow'].map(kind => [kind, displayedPermissionRules(kind)])
  );
  ruleSet[fromKind] = ruleSet[fromKind].filter(candidate => candidate !== rule);
  if (!ruleSet[toKind].includes(rule)) ruleSet[toKind].push(rule);
  try {
    permissionSettings = await invoke('update_permission_rule_set', {
      scope: permissionScope,
      deny: ruleSet.deny,
      ask: ruleSet.ask,
      allow: ruleSet.allow,
    });
    setPermissionSettingsError('');
    renderPermissionSettings();
  } catch (error) {
    setPermissionSettingsError(`Failed to move permission rule: ${String(error)}`);
  }
}

const FIXED_KEY_BINDINGS = Object.freeze([
  { binding: 'Escape', title: 'Dismiss the current panel' },
  { binding: 'Double Escape', title: 'Abort the active response' },
  { binding: 'Y / T / N / H', title: 'Choose a permission response' },
  { binding: 'J / K / ↑ / ↓ / Enter', title: 'Move through permission choices' },
  { binding: '1–9 / N / D / Enter', title: 'Choose or advance a question or trust option' },
  { binding: 'Tab / ↑ / ↓ / Enter', title: 'Confirm or navigate autocomplete' },
]);

async function loadKeyBindings() {
  try {
    keyBindingDefinitions = await invoke('get_key_bindings');
    setKeyBindingError('');
    renderKeyBindings(document.getElementById('shortcutSearch')?.value || '');
  } catch (error) {
    setKeyBindingError(String(error));
    showError('Failed to load keyboard shortcuts: ' + String(error));
    throw error;
  }
}

function setKeyBindingError(message) {
  const error = document.getElementById('shortcutError');
  if (!error) return;
  error.textContent = message;
  error.hidden = !message;
}

function bindingFor(action) {
  return keyBindingDefinitions.find(definition => definition.action === action)?.binding || '';
}

function canonicalKeyEvent(event) {
  if (['Control', 'Meta', 'Alt', 'Shift'].includes(event.key)) return '';
  const parts = [];
  if (event.ctrlKey) parts.push('Ctrl');
  if (event.metaKey) parts.push('Meta');
  if (event.altKey) parts.push('Alt');
  if (event.shiftKey) parts.push('Shift');
  let key = event.key;
  if (key === ' ') key = 'Space';
  if (key.length === 1 && /[a-z]/i.test(key)) key = key.toUpperCase();
  parts.push(key);
  return parts.join('+');
}

function matchesKeyBinding(event, action) {
  return canonicalKeyEvent(event).toLowerCase() === bindingFor(action).toLowerCase();
}

function renderKeyBindings(query = '') {
  const list = document.getElementById('keyBindingList');
  const fixedList = document.getElementById('fixedKeyBindingList');
  if (!list || !fixedList) return;
  const normalizedQuery = query.trim().toLowerCase();
  const definitions = keyBindingDefinitions.filter(definition =>
    !normalizedQuery ||
    definition.title.toLowerCase().includes(normalizedQuery) ||
    definition.description.toLowerCase().includes(normalizedQuery) ||
    definition.binding.toLowerCase().includes(normalizedQuery)
  );
  list.replaceChildren();
  for (const definition of definitions) {
    const row = document.createElement('div');
    row.className = 'shortcut-row';
    const copy = document.createElement('div');
    copy.className = 'shortcut-copy';
    copy.innerHTML = `<div class="shortcut-title">${escapeHtml(definition.title)}</div>` +
      `<div class="shortcut-description">${escapeHtml(definition.description)}</div>`;
    const actions = document.createElement('div');
    actions.className = 'shortcut-actions';
    const binding = document.createElement('button');
    binding.type = 'button';
    binding.className = 'shortcut-binding';
    binding.textContent = capturingKeyBindingAction === definition.action
      ? 'Press keys…'
      : definition.binding;
    binding.classList.toggle('capturing', capturingKeyBindingAction === definition.action);
    binding.setAttribute('aria-label', `Change ${definition.title} shortcut`);
    binding.onclick = () => beginKeyBindingCapture(definition.action);
    const reset = document.createElement('button');
    reset.type = 'button';
    reset.className = 'shortcut-reset';
    reset.textContent = '↺';
    reset.title = 'Restore default';
    reset.disabled = definition.binding === definition.defaultBinding;
    reset.onclick = () => resetKeyBinding(definition.action);
    actions.append(binding, reset);
    row.append(copy, actions);
    list.appendChild(row);
  }
  if (!definitions.length) {
    const empty = document.createElement('div');
    empty.className = 'shortcut-empty';
    empty.textContent = 'No matching shortcuts';
    list.appendChild(empty);
  }

  fixedList.replaceChildren();
  for (const definition of FIXED_KEY_BINDINGS.filter(definition =>
    !normalizedQuery ||
    definition.title.toLowerCase().includes(normalizedQuery) ||
    definition.binding.toLowerCase().includes(normalizedQuery)
  )) {
    const row = document.createElement('div');
    row.className = 'shortcut-row';
    row.innerHTML = `<div class="shortcut-copy"><div class="shortcut-title">${escapeHtml(definition.title)}</div></div>` +
      `<span class="setting-value">${escapeHtml(definition.binding)}</span>`;
    fixedList.appendChild(row);
  }
}

function beginKeyBindingCapture(action) {
  setKeyBindingError('');
  capturingKeyBindingAction = action;
  renderKeyBindings(document.getElementById('shortcutSearch')?.value || '');
}

async function saveCapturedKeyBinding(event) {
  if (!capturingKeyBindingAction) return false;
  event.preventDefault();
  event.stopImmediatePropagation();
  if (event.key === 'Escape') {
    capturingKeyBindingAction = null;
    renderKeyBindings(document.getElementById('shortcutSearch')?.value || '');
    return true;
  }
  const binding = canonicalKeyEvent(event);
  if (!binding) return true;
  const action = capturingKeyBindingAction;
  capturingKeyBindingAction = null;
  try {
    keyBindingDefinitions = await invoke('update_key_binding', { action, binding });
    setKeyBindingError('');
  } catch (error) {
    setKeyBindingError(String(error));
    showError('Failed to update keyboard shortcut: ' + String(error));
  }
  renderKeyBindings(document.getElementById('shortcutSearch')?.value || '');
  return true;
}

async function resetKeyBinding(action) {
  try {
    keyBindingDefinitions = await invoke('reset_key_binding', { action });
    setKeyBindingError('');
    renderKeyBindings(document.getElementById('shortcutSearch')?.value || '');
  } catch (error) {
    setKeyBindingError(String(error));
    showError('Failed to reset keyboard shortcut: ' + String(error));
  }
}

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
    range.value = String(appearance.fontSize);
    input.value = String(appearance.fontSize);
    range.oninput = () => {
      input.value = range.value;
      applyFontSize(range.value);
    };
    range.onchange = () => saveSetting('appearance_font_size', range.value);
    input.oninput = () => {
      const value = clampFontSize(input.value);
      range.value = String(value);
      applyFontSize(value);
    };
    input.onchange = () => {
      const value = clampFontSize(input.value);
      input.value = String(value);
      range.value = String(value);
      saveSetting('appearance_font_size', String(value));
    };
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
  document.querySelectorAll('[data-theme-mode-card]').forEach(card => {
    const active = card.dataset.themeModeCard === mode;
    card.classList.toggle('active', active);
    card.setAttribute('aria-pressed', String(active));
  });
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

function usesCombinedAttachmentPicker() {
  return /Macintosh|Mac OS X/.test(navigator.userAgent);
}

function configureAttachmentPicker() {
  const fileButton = document.getElementById('attachFileButton');
  const directoryButton = document.getElementById('attachDirectoryButton');
  const combined = usesCombinedAttachmentPicker();

  if (fileButton) {
    fileButton.title = combined ? 'Attach file or folder' : 'Attach file';
    fileButton.setAttribute('aria-label', fileButton.title);
  }
  if (directoryButton) directoryButton.hidden = combined;
}

const nativeFileDragEvents = {
  'tauri://drag-enter': 'enter',
  'tauri://drag-over': 'over',
  'tauri://drag-drop': 'drop',
  'tauri://drag-leave': 'leave',
};

function handleNativeFileDrag(type, payload = {}) {
  const inputWrapper = document.querySelector('.input-wrapper');
  const active = type === 'enter' || type === 'over';
  inputWrapper?.classList.toggle('file-drop-active', active);

  if (type !== 'drop') return;

  const paths = Array.isArray(payload?.paths)
    ? payload.paths.filter(path => typeof path === 'string' && path.length > 0)
    : [];
  if (paths.length > 0) {
    insertFileReferences(paths);
  }
}

async function configureNativeFileDrag() {
  const input = document.getElementById('msgInput');
  if (!input || typeof listen !== 'function') return;

  // Finder drops are delivered through Tauri's native events. Prevent the
  // contenteditable default from navigating or inserting an opaque File node.
  input.addEventListener('dragover', event => event.preventDefault());
  input.addEventListener('drop', event => event.preventDefault());

  try {
    await Promise.all(Object.entries(nativeFileDragEvents).map(([eventName, type]) => (
      listen(eventName, event => handleNativeFileDrag(type, event.payload))
    )));
  } catch (error) {
    showError('Native file drag listener failed: ' + String(error));
  }
}

async function attachFileReference() {
  const mode = usesCombinedAttachmentPicker() ? 'any' : 'file';
  await attachReference(mode);
}

async function attachDirectoryReference() {
  await attachReference('directory');
}

async function attachReference(mode) {
  try {
    const path = await invoke('pick_attachment', { mode });
    if (!path) return;
    insertFileReferences([path]);
  } catch (e) {
    showError('Attachment picker failed: ' + String(e));
  }
}

function insertSlashCommandPrefix() {
  insertInputText('/');
}

function insertInputText(text) {
  const input = document.getElementById('msgInput');
  if (!input) return;
  const current = getInputText(input);
  const selection = getInputSelection(input);
  const start = selection.start;
  const end = selection.end;
  setInputText(input, current.slice(0, start) + text + current.slice(end));
  const cursor = start + text.length;
  setInputSelection(input, cursor);
  input.focus();
  autoResize(input);
  updateAutocomplete();
}

function insertFileReferences(paths) {
  const input = document.getElementById('msgInput');
  if (!input || !Array.isArray(paths) || paths.length === 0) return;
  const current = getInputText(input);
  const selection = getInputSelection(input);
  const beforeSelection = current.slice(0, selection.start);
  const separator = beforeSelection.length > 0 && !/\s$/.test(beforeSelection) ? ' ' : '';
  insertInputText(separator + paths.map(formatFileReference).join(''));
}

function formatFileReference(path) {
  if (path.includes('"')) return '@' + path + ' ';
  if (/\s/.test(path)) return '@"' + path + '" ';
  return '@' + path + ' ';
}

function autoResize(el) {
  el.style.height = 'auto';
  el.style.height = Math.min(el.scrollHeight, 120) + 'px';
}

// =============== Slash Command Autocomplete ===============

let acVisible = false;

async function updateAutocomplete() {
  const input = document.getElementById('msgInput');
  const popup = document.getElementById('autocomplete');
  if (!input || !popup) return;
  const val = getInputText(input);
  const cursor = getInputCursor(input);
  const seq = ++acRequestSeq;
  let result = null;
  try {
    result = await invoke('autocomplete_input', { text: val, cursor });
  } catch (e) {
    hideAutocomplete();
    return;
  }
  if (isInputComposing || seq !== acRequestSeq) return;
  const highlightRanges = result.highlightRanges || [];
  setInputMatchState(!!result.validMatch || highlightRanges.length > 0);
  updateInputHighlight(highlightRanges);
  if (!result.items || result.items.length === 0 || !result.prefix) {
    hideAutocomplete(false);
    return;
  }

  acPrefix = result.prefix;
  acItems = result.items;
  acSelectedIndex = -1;
  popup.innerHTML = acItems.map((m, i) =>
    '<div class="ac-item" data-index="' + i + '" onmousedown="selectAutocomplete(' + i + ')" ' +
    'onmouseenter="acHighlight(' + i + ')">' +
    '<div class="ac-cmd">' + escapeHtml(m.label || m.value) + '</div>' +
    '<div class="ac-desc">' + escapeHtml(m.description || '') + '</div></div>'
  ).join('');
  popup.classList.add('visible');
  acVisible = true;
}

function acHighlight(index) {
  acSelectedIndex = index;
  const popup = document.getElementById('autocomplete');
  if (!popup) return;
  popup.querySelectorAll('.ac-item').forEach((el, i) => {
    el.classList.toggle('selected', i === index);
  });
}

function navigateAutocomplete(direction) {
  const popup = document.getElementById('autocomplete');
  if (!popup || !acVisible) return false;
  const items = popup.querySelectorAll('.ac-item');
  if (items.length === 0) return false;

  if (direction === 'down') {
    acSelectedIndex = (acSelectedIndex + 1) % items.length;
  } else {
    acSelectedIndex = acSelectedIndex <= 0 ? items.length - 1 : acSelectedIndex - 1;
  }

  items.forEach((el, i) => el.classList.toggle('selected', i === acSelectedIndex));
  items[acSelectedIndex].scrollIntoView({ block: 'nearest' });
  return true;
}

function confirmAutocomplete() {
  if (!acVisible) return false;
  const popup = document.getElementById('autocomplete');
  if (!popup) return false;
  const items = popup.querySelectorAll('.ac-item');
  if (!items.length) return false;
  // 没有选中项时默认选第一个
  const idx = acSelectedIndex >= 0 ? acSelectedIndex : 0;
  if (items[idx]) {
    selectAutocomplete(idx);
    return true;
  }
  return false;
}

function selectAutocomplete(index) {
  const input = document.getElementById('msgInput');
  const item = acItems[index];
  if (!input || !item || !acPrefix) return;
  const text = getInputText(input);
  const cursor = getInputCursor(input);
  const start = Math.max(0, cursor - acPrefix.length);
  setInputText(input, text.slice(0, start) + item.value + text.slice(cursor));
  const nextCursor = start + item.value.length;
  setInputSelection(input, nextCursor);
  input.focus();
  autoResize(input);
  hideAutocomplete(false);
  updateAutocomplete();
}

function hideAutocomplete(clearMatch = true) {
  const popup = document.getElementById('autocomplete');
  if (popup) popup.classList.remove('visible');
  acVisible = false;
  acSelectedIndex = -1;
  acPrefix = '';
  acItems = [];
  if (clearMatch) {
    setInputMatchState(inputHighlightRanges.length > 0);
  }
}

function isTransientPopupVisible(popup) {
  if (!popup) return false;
  if (popup.id === 'autocomplete' || popup.id === 'quotaTooltip') {
    return popup.classList.contains('visible');
  }
  return !popup.hidden;
}

function hideTransientPopup(popup) {
  if (popup.id === 'autocomplete') {
    acRequestSeq++;
    hideAutocomplete();
  } else if (popup.id === 'quotaTooltip') {
    hideQuotaTooltip();
  } else if (popup.id === 'thinkingEffortPopover') {
    hideThinkingEffortPicker();
  } else {
    popup.hidden = true;
  }
}

function dismissTransientPopupsOutside(target) {
  let dismissed = false;
  for (const id of TRANSIENT_POPUP_IDS) {
    const popup = document.getElementById(id);
    if (!isTransientPopupVisible(popup) || popup.contains(target)) continue;
    hideTransientPopup(popup);
    dismissed = true;
  }
  return dismissed;
}

function dismissTransientPopups() {
  let dismissed = false;
  for (const id of TRANSIENT_POPUP_IDS) {
    const popup = document.getElementById(id);
    if (!isTransientPopupVisible(popup)) continue;
    hideTransientPopup(popup);
    dismissed = true;
  }
  return dismissed;
}

function setInputMatchState(valid) {
  const wrapper = document.querySelector('.input-wrapper');
  if (wrapper) wrapper.classList.toggle('valid-token', valid);
}

function updateInputHighlight(ranges) {
  inputHighlightRanges = Array.isArray(ranges) ? ranges : [];
  const input = document.getElementById('msgInput');
  if (!input || isInputComposing) return;
  renderRichInputHighlights(input, inputHighlightRanges, true);
}

function renderRichInputHighlights(input, ranges, resize = false) {
  const text = getInputText(input);
  const selection = getInputSelection(input);
  if (!Array.isArray(ranges) || ranges.length === 0) {
    setInputText(input, text);
    setInputSelection(input, selection.start, selection.end);
    if (resize) autoResize(input);
    return;
  }
  const chars = Array.from(text);
  const fragment = document.createDocumentFragment();
  let cursor = 0;
  const normalized = ranges
    .map(range => ({
      start: Math.max(0, Math.min(chars.length, Number(range.start || 0))),
      end: Math.max(0, Math.min(chars.length, Number(range.end || 0))),
    }))
    .filter(range => range.end > range.start)
    .sort((a, b) => a.start - b.start || a.end - b.end);
  for (const range of normalized) {
    if (range.start < cursor) continue;
    fragment.appendChild(document.createTextNode(chars.slice(cursor, range.start).join('')));
    const span = document.createElement('span');
    span.className = 'valid-token-text';
    span.textContent = chars.slice(range.start, range.end).join('');
    fragment.appendChild(span);
    cursor = range.end;
  }
  fragment.appendChild(document.createTextNode(chars.slice(cursor).join('')));
  input.replaceChildren(fragment);
  setInputSelection(input, selection.start, selection.end);
  if (resize) autoResize(input);
}

function syncInputHighlightScroll() {
  // contenteditable renders highlights directly; no overlay scroll sync is needed.
}

// =============== Keyboard Shortcuts ===============

document.addEventListener('keydown', function(e) {
  const input = document.getElementById('msgInput');

  if (capturingKeyBindingAction) {
    void saveCapturedKeyBinding(e);
    return;
  }

  // IME owns composition keystrokes. Let the browser keep the preedit text
  // intact; Enter/send, autocomplete, and DOM replacement run after commit.
  if (isInputComposing || e.isComposing || e.keyCode === 229) return;

  // Notification error list: Escape closes it (and unpins) before other handling.
  if (e.key === 'Escape' && !notificationErrorList().hidden) {
    e.preventDefault();
    closeNotificationErrorList();
    return;
  }

  // A first Escape keeps its contextual behavior (dismiss, deny, close).
  // A second Escape within the window always stops the active interaction.
  if (e.key === 'Escape' && isStreaming) {
    const now = performance.now();
    const isDoubleEscape = now - lastStreamingEscapeAt <= DOUBLE_ESCAPE_WINDOW_MS;
    lastStreamingEscapeAt = isDoubleEscape ? 0 : now;
    if (isDoubleEscape) {
      e.preventDefault();
      abortAgent();
      return;
    }
  }

  // Agent question panel shortcuts
  if (currentQuestionId) {
    const otherInput = document.getElementById('questionPanelOtherInput');
    if (otherInput && e.target === otherInput) {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        void submitUserQuestion();
      } else if (e.key === 'Escape') {
        e.preventDefault();
        clearQuestionOtherInput();
      }
      return;
    }
    const questionEvent = activeQuestionEvent();
    const number = Number.parseInt(e.key, 10);
    if (number >= 1 && selectQuestionOption(number)) {
      e.preventDefault();
      return;
    }
    if (questionEvent && e.key.toUpperCase() === 'N' && currentQuestionIndex + 1 < questionEvent.questions.length) {
      e.preventDefault();
      void submitUserQuestion();
      return;
    }
    if (questionEvent && e.key.toUpperCase() === 'D' && currentQuestionIndex + 1 >= questionEvent.questions.length) {
      e.preventDefault();
      void submitUserQuestion();
      return;
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      void submitUserQuestion();
      return;
    }
    if (e.key === 'Escape') {
      e.preventDefault();
      discardCurrentQuestionUi();
      void abortAgent();
      return;
    }
    return;
  }

  // Permission panel shortcuts
  if (currentPermissionId) {
    const hintPage = document.getElementById('permPanelHint');
    if (hintPage && !hintPage.hidden) return;
    if (e.key === 'j' || e.key === 'J' || e.key === 'ArrowDown') {
      e.preventDefault();
      movePermissionSelection(1);
      return;
    }
    if (e.key === 'k' || e.key === 'K' || e.key === 'ArrowUp') {
      e.preventDefault();
      movePermissionSelection(-1);
      return;
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      confirmPermissionSelection();
      return;
    }
    if (e.key === 'Tab') {
      const selected = activePermissionActions().find(action => action === document.activeElement);
      const selectedKey = selected?.querySelector('.perm-panel-opt-key')?.textContent;
      if (selectedKey === 'H') {
        e.preventDefault();
        enterPermissionHint();
        return;
      }
    }
    if (currentPermissionTrustIndex >= 0) {
      const group = currentPermissionTrustGroups[currentPermissionTrustIndex];
      const levels = group && Array.isArray(group.levels) ? group.levels : [];
      const number = Number.parseInt(e.key, 10);
      if (number >= 1 && number <= levels.length + 1) {
        e.preventDefault();
        choosePermissionTrust(number <= levels.length ? levels[number - 1].key : null);
        return;
      }
      if (e.key === 'Escape') {
        e.preventDefault();
        currentPermissionTrustIndex = -1;
        currentPermissionTrustKeys = [];
        showPermissionMainPage();
        return;
      }
      return;
    }
    if (e.key === 'y' || e.key === 'Y') { e.preventDefault(); respondPermission('allow'); return; }
    if (e.key === 't' || e.key === 'T') { e.preventDefault(); enterPermissionTrust(); return; }
    if (e.key === 'n' || e.key === 'N') { e.preventDefault(); respondPermission('deny'); return; }
    if (e.key === 'h' || e.key === 'H') { e.preventDefault(); enterPermissionHint(); return; }
    if (e.key === 'Escape') { e.preventDefault(); respondPermission('deny'); return; }
  }

  // Escape handling
  if (e.key === 'Escape') {
    if (dismissTransientPopups()) { e.preventDefault(); return; }
    if (document.getElementById('settingsPanel').classList.contains('visible')) {
      closeSettings(); return;
    }
    if (isStreaming) { e.preventDefault(); return; }
    return;
  }

  // Global shortcuts
  if (matchesKeyBinding(e, 'toggleThinking')) {
    e.preventDefault();
    document.body.classList.toggle('thinking-expanded');
    return;
  }

  if (matchesKeyBinding(e, 'openModelPicker')) {
    e.preventDefault();
    showModelPicker();
    return;
  }

  if (matchesKeyBinding(e, 'newSession')) {
    e.preventDefault();
    newSession();
    return;
  }

  if (matchesKeyBinding(e, 'openSettings')) {
    e.preventDefault();
    toggleSettings();
    return;
  }

  // Input field shortcuts
  if (document.activeElement === input) {
    if (matchesKeyBinding(e, 'insertNewline')) {
      e.preventDefault();
      insertInputText('\n');
      return;
    }

    // Autocomplete navigation
    if (acVisible) {
      if (e.key === 'ArrowDown') { e.preventDefault(); navigateAutocomplete('down'); return; }
      if (e.key === 'ArrowUp') { e.preventDefault(); navigateAutocomplete('up'); return; }
      if (e.key === 'Tab') { e.preventDefault(); confirmAutocomplete(); return; }
      if (e.key === 'Enter' && acSelectedIndex >= 0) { e.preventDefault(); confirmAutocomplete(); return; }
    }

    if (matchesKeyBinding(e, 'sendMessage')) {
      e.preventDefault();
      if (acVisible && acSelectedIndex >= 0) {
        confirmAutocomplete();
      } else {
        hideAutocomplete();
        sendMessage();
      }
      return;
    }
  }

  // Focus input with / key when not already focused
  if (matchesKeyBinding(e, 'focusComposer') && document.activeElement !== input) {
    input.focus();
    // Don't prevent default - let the / character be typed
  }
});

document.addEventListener('pointerdown', function(e) {
  dismissTransientPopupsOutside(e.target);
});

document.addEventListener('paste', function(e) {
  const input = document.getElementById('msgInput');
  if (document.activeElement !== input) return;
  const text = e.clipboardData ? e.clipboardData.getData('text/plain') : '';
  if (!text) return;
  e.preventDefault();
  insertInputText(text);
});

document.addEventListener('mouseover', function(e) {
  const target = e.target.closest('[data-quota-tooltip]');
  if (!target) return;
  showQuotaTooltip(target);
});

document.addEventListener('mouseout', function(e) {
  const target = e.target.closest('[data-quota-tooltip]');
  if (!target || (e.relatedTarget && target.contains(e.relatedTarget))) return;
  hideQuotaTooltip();
});

document.addEventListener('scroll', hideQuotaTooltip, true);

function showQuotaTooltip(target) {
  const tooltip = document.getElementById('quotaTooltip');
  const text = target.dataset.quotaTooltip;
  if (!tooltip || !text) return;
  tooltip.textContent = text;
  tooltip.classList.add('visible');
  const rect = target.getBoundingClientRect();
  const tipRect = tooltip.getBoundingClientRect();
  const margin = 8;
  let left = rect.right - tipRect.width;
  left = Math.max(margin, Math.min(left, window.innerWidth - tipRect.width - margin));
  let top = rect.top - tipRect.height - margin;
  if (top < margin) top = rect.bottom + margin;
  tooltip.style.left = left + 'px';
  tooltip.style.top = top + 'px';
}

function hideQuotaTooltip() {
  const tooltip = document.getElementById('quotaTooltip');
  if (tooltip) tooltip.classList.remove('visible');
}

// =============== UI Helpers ===============

function isToolCallExpanded(toolCallId) {
  if (!activeSessionId || !toolCallId) return false;
  const expanded = expandedToolCallsBySession[activeSessionId];
  return Array.isArray(expanded) && expanded.includes(toolCallId);
}

function toggleToolCall(el) {
  const opening = !el.classList.contains('expanded');
  el.classList.toggle('expanded', opening);
  const sessionId = el.dataset.sessionId || activeSessionId;
  const toolCallId = el.dataset.toolCallId;
  if (!sessionId || !toolCallId) return;
  const expanded = new Set(expandedToolCallsBySession[sessionId] || []);
  if (opening) expanded.add(toolCallId);
  else expanded.delete(toolCallId);
  expandedToolCallsBySession[sessionId] = [...expanded];
}

function toggleThinking(header) {
  const block = header.closest('.thinking-block');
  if (!block) return;
  const expanded = block.classList.toggle('expanded');
  header.setAttribute('aria-expanded', String(expanded));
  const message = block.closest('.msg');
  const container = document.getElementById('chatMessages');
  if (message && container && activeSessionId) {
    expandedThinkingBySession[thinkingStateKey(activeSessionId, [...container.children].indexOf(message))] = expanded;
  }
}

function showError(message) {
  const container = document.getElementById('chatMessages');
  if (!container) return;
  const div = document.createElement('div');
  div.className = 'msg msg-assistant';
  div.innerHTML = '<div class="msg-avatar">!</div><div class="msg-body"><div class="msg-role">Error</div>' +
    '<div class="msg-content" style="color:var(--error)"><p>' + escapeHtml(message) + '</p></div></div>';
  container.appendChild(div);
  container.scrollTop = container.scrollHeight;
}

// ============ 通知中心：主视图全局 toast 层 ============
const NOTIFICATION_TIMEOUT_MS = 6000;
// ============ Dev-flow 只读详情浮层 ============
const DEV_FLOW_DETAIL_BASELINE_LIMIT = 32;
const devFlowDetailBaselines = new Map();

function rememberDevFlowDetailBaseline(projectKey, revision) {
  devFlowDetailBaselines.delete(projectKey);
  devFlowDetailBaselines.set(projectKey, revision);
  while (devFlowDetailBaselines.size > DEV_FLOW_DETAIL_BASELINE_LIMIT) {
    devFlowDetailBaselines.delete(devFlowDetailBaselines.keys().next().value);
  }
}
let devFlowDetailOpen = false;

function showDevFlowDetail(payload) {
  if (!payload || !payload.project || payload.availability !== 'ready') return;
  const projectKey = payload.project.projectKey;
  const baseline = devFlowDetailBaselines.get(projectKey) || 0;
  // The main view rejects stale or out-of-order events for a project it has
  // already rendered at a newer snapshot revision.
  if (payload.revision < baseline) return;
  rememberDevFlowDetailBaseline(projectKey, payload.revision);
  renderDevFlowDetail(payload);
  openDevFlowDetailPanel();
}

function devFlowCountLabel(count, noun) {
  return count === 1 ? '1 ' + noun : count + ' ' + noun + 's';
}

function renderDevFlowDetail(payload) {
  const revision = document.getElementById('devFlowDetailRevision');
  if (revision) revision.textContent = payload.revision ? '#' + payload.revision : '';
  const project = document.getElementById('devFlowDetailProject');
  if (project) project.textContent = (payload.project.root || '') + ' · ' + (payload.project.revision || '');
  const summary = document.getElementById('devFlowDetailSummary');
  if (summary) {
    summary.textContent = devFlowCountLabel(payload.openTasks, 'Task') + ' · ' + devFlowCountLabel(payload.openIssues, 'Issue') +
      (payload.stale ? ' · stale' : '');
  }
  const list = document.getElementById('devFlowDetailList');
  if (!list) return;
  list.replaceChildren();
  const items = Array.isArray(payload.items) ? payload.items : [];
  if (!items.length) {
    const empty = document.createElement('div');
    empty.className = 'dev-flow-detail-empty';
    empty.textContent = 'No open work';
    list.appendChild(empty);
    return;
  }
  items.forEach(item => {
    const row = document.createElement('div');
    row.className = 'dev-flow-detail-item';
    row.setAttribute('role', 'listitem');
    row.tabIndex = 0;
    row.setAttribute('aria-label', (item.kind === 'issue' ? 'Issue ' : 'Task ') + item.shortId + ' ' + item.title);
    const head = document.createElement('div');
    head.className = 'dev-flow-detail-item-head';
    const id = document.createElement('span');
    id.className = 'dev-flow-detail-item-id';
    id.textContent = item.shortId || item.id;
    const title = document.createElement('span');
    title.className = 'dev-flow-detail-item-title';
    title.textContent = item.title || '';
    const status = document.createElement('span');
    status.className = 'dev-flow-detail-item-status' + (item.status === 'in-progress' ? ' in-progress' : '');
    status.textContent = item.status || '';
    head.append(id, title, status);
    row.appendChild(head);
    const metaParts = [];
    if (item.priority) metaParts.push(item.priority);
    if (item.complexity) metaParts.push(item.complexity);
    if (item.taskType) metaParts.push(item.taskType);
    if (item.severity) metaParts.push(item.severity);
    if (item.dependsOn && item.dependsOn.length) metaParts.push('depends: ' + item.dependsOn.join(', '));
    if (item.refs) metaParts.push(item.refs);
    if (metaParts.length) {
      const meta = document.createElement('div');
      meta.className = 'dev-flow-detail-item-meta';
      meta.textContent = metaParts.join(' · ');
      row.appendChild(meta);
    }
    if (item.description) {
      const desc = document.createElement('div');
      desc.className = 'dev-flow-detail-item-desc';
      desc.textContent = item.description;
      row.appendChild(desc);
    }
    const files = [];
    (item.filesCreate || []).forEach(file => files.push('CREATE: ' + file));
    (item.filesModify || []).forEach(file => files.push('MODIFY: ' + file));
    (item.filesTest || []).forEach(file => files.push('TEST: ' + file));
    if (files.length) {
      const fileDetails = document.createElement('details');
      fileDetails.className = 'dev-flow-detail-disclosure';
      const fileSummary = document.createElement('summary');
      fileSummary.textContent = 'Files (' + files.length + ')';
      const fileList = document.createElement('div');
      fileList.className = 'dev-flow-detail-disclosure-body';
      files.forEach(file => {
        const entry = document.createElement('div');
        entry.textContent = file;
        fileList.appendChild(entry);
      });
      fileDetails.append(fileSummary, fileList);
      row.appendChild(fileDetails);
    }
    if (item.doneWhen && item.doneWhen.length) {
      const doneDetails = document.createElement('details');
      doneDetails.className = 'dev-flow-detail-disclosure';
      const doneSummary = document.createElement('summary');
      doneSummary.textContent = 'Done when (' + item.doneWhen.length + ')';
      const doneList = document.createElement('ul');
      doneList.className = 'dev-flow-detail-disclosure-body';
      item.doneWhen.forEach(criterion => {
        const entry = document.createElement('li');
        entry.textContent = criterion;
        doneList.appendChild(entry);
      });
      doneDetails.append(doneSummary, doneList);
      row.appendChild(doneDetails);
    }
    if (payload.focusId && payload.focusId === item.id) {
      row.classList.add('focus');
    }
    list.appendChild(row);
  });
  const focusItem = payload.focusId
    ? items.findIndex(item => item.id === payload.focusId)
    : -1;
  if (focusItem >= 0 && list.children[focusItem]) {
    list.children[focusItem].scrollIntoView({ block: 'nearest' });
  }
}

function openDevFlowDetailPanel() {
  const overlay = document.getElementById('devFlowDetail');
  if (!overlay) return;
  overlay.hidden = false;
  devFlowDetailOpen = true;
  const focusTarget = overlay.querySelector('.dev-flow-detail-item') || document.getElementById('devFlowDetailClose');
  if (focusTarget) focusTarget.focus();
}

function closeDevFlowDetail() {
  const overlay = document.getElementById('devFlowDetail');
  if (!overlay) return;
  overlay.hidden = true;
  devFlowDetailOpen = false;
}

document.getElementById('devFlowDetailClose')
  ?.addEventListener('click', closeDevFlowDetail);

document.addEventListener('pointerdown', event => {
  if (devFlowDetailOpen && !event.target.closest('#devFlowDetail')) {
    closeDevFlowDetail();
  }
});

document.addEventListener('keydown', event => {
  if (!devFlowDetailOpen) return;
  if (event.key === 'Escape') {
    event.preventDefault();
    closeDevFlowDetail();
    return;
  }
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    const items = Array.from(document.querySelectorAll('.dev-flow-detail-item'));
    if (!items.length) return;
    const index = items.indexOf(document.activeElement);
    const next = event.key === 'ArrowDown'
      ? (index + 1) % items.length
      : (index - 1 + items.length) % items.length;
    event.preventDefault();
    items[next].focus();
  }
});

const notificationToasts = new Map();
const unresolvedErrors = new Map();
let legacyNotificationCounter = 0;
let notificationErrorListPinned = false;

function notificationStack() {
  return document.getElementById('notificationStack');
}

function notificationErrorTray() {
  return document.getElementById('notificationErrorTray');
}

function notificationErrorList() {
  return document.getElementById('notificationErrorList');
}

function notificationErrorTrayButton() {
  return document.getElementById('notificationErrorTrayButton');
}

function notificationErrorCount() {
  return document.getElementById('notificationErrorCount');
}

function notificationIcon(severity) {
  if (severity === 'success') return '✓';
  if (severity === 'error') return '!';
  return 'i';
}

function applyNotificationPresentation(toast, severity, title, message) {
  toast.element.className = 'notification-toast notification-' + severity;
  toast.element.setAttribute('role', severity === 'error' ? 'alert' : 'status');
  toast.element.querySelector('.notification-icon').textContent = notificationIcon(severity);
  toast.element.querySelector('.notification-title').textContent = title;
  toast.element.querySelector('.notification-message').textContent = message;
  toast.severity = severity;
  toast.title = title;
  toast.message = message;
}

function showNotification(message) {
  legacyNotificationCounter += 1;
  upsertNotification({
    id: 'legacy-' + legacyNotificationCounter,
    severity: 'info',
    title: 'Rózsa',
    message: String(message),
    timeoutMs: NOTIFICATION_TIMEOUT_MS,
  });
}

function upsertNotification(payload) {
  const id = String(payload.id);
  const severity = String(payload.severity || 'info');
  const title = String(payload.title || 'Rózsa');
  const message = String(payload.message || '');
  const timeoutMs = Number.isFinite(payload.timeoutMs) && payload.timeoutMs > 0
    ? payload.timeoutMs
    : NOTIFICATION_TIMEOUT_MS;
  const existing = notificationToasts.get(id);
  if (existing) {
    applyNotificationPresentation(existing, severity, title, message);
    existing.remainingMs = timeoutMs;
    existing.expiresAt = performance.now() + timeoutMs;
    clearTimeout(existing.timer);
    existing.timer = null;
    if (!existing.paused) {
      existing.timer = setTimeout(() => expireNotification(id), timeoutMs);
    }
    return;
  }
  if (unresolvedErrors.has(id)) {
    if (severity === 'error') {
      unresolvedErrors.set(id, { title, message });
      updateNotificationErrorTray();
      return;
    }
    unresolvedErrors.delete(id);
    updateNotificationErrorTray();
  }
  const element = document.createElement('div');
  element.className = 'notification-toast notification-' + severity;
  element.setAttribute('role', severity === 'error' ? 'alert' : 'status');
  element.innerHTML =
    '<div class="notification-icon" aria-hidden="true">' + notificationIcon(severity) + '</div>' +
    '<div class="notification-body">' +
    '<div class="notification-title"></div>' +
    '<div class="notification-message"></div>' +
    '</div>' +
    '<button class="notification-close" type="button" aria-label="Dismiss notification">✕</button>';
  element.querySelector('.notification-title').textContent = title;
  element.querySelector('.notification-message').textContent = message;
  element.querySelector('.notification-close').addEventListener('click', () => dismissToast(id));
  element.addEventListener('pointerenter', () => pauseToastTimer(id));
  element.addEventListener('pointerleave', () => resumeToastTimer(id));
  notificationStack().appendChild(element);
  const toast = {
    element,
    timer: null,
    remainingMs: timeoutMs,
    severity,
    title,
    message,
    paused: false,
  };
  applyNotificationPresentation(toast, severity, title, message);
  notificationToasts.set(id, toast);
  toast.expiresAt = performance.now() + timeoutMs;
  toast.timer = setTimeout(() => expireNotification(id), timeoutMs);
}

function pauseToastTimer(id) {
  const toast = notificationToasts.get(id);
  if (!toast || toast.paused) return;
  toast.paused = true;
  if (toast.timer != null) {
    clearTimeout(toast.timer);
    toast.timer = null;
    toast.remainingMs = Math.max(0, toast.expiresAt - performance.now());
  }
}

function resumeToastTimer(id) {
  const toast = notificationToasts.get(id);
  if (!toast || !toast.paused) return;
  toast.paused = false;
  if (toast.remainingMs <= 0) {
    expireNotification(id);
    return;
  }
  toast.expiresAt = performance.now() + toast.remainingMs;
  toast.timer = setTimeout(() => expireNotification(id), toast.remainingMs);
}

function expireNotification(id) {
  const toast = notificationToasts.get(id);
  if (!toast) return;
  notificationToasts.delete(id);
  clearTimeout(toast.timer);
  toast.element.remove();
  if (toast.severity === 'error') {
    unresolvedErrors.set(id, { title: toast.title, message: toast.message });
  }
  updateNotificationErrorTray();
}

function dismissToast(id) {
  const toast = notificationToasts.get(id);
  if (!toast) return;
  notificationToasts.delete(id);
  clearTimeout(toast.timer);
  toast.element.remove();
  if (toast.severity === 'error') {
    unresolvedErrors.set(id, { title: toast.title, message: toast.message });
  }
  updateNotificationErrorTray();
}

function resolveNotification(id) {
  const toast = notificationToasts.get(id);
  if (toast) {
    notificationToasts.delete(id);
    clearTimeout(toast.timer);
    toast.element.remove();
  }
  unresolvedErrors.delete(id);
  updateNotificationErrorTray();
}

function isNotificationErrorListOpen() {
  return !notificationErrorList().hidden;
}

function openNotificationErrorList() {
  notificationErrorList().hidden = false;
  notificationErrorTrayButton().setAttribute('aria-expanded', 'true');
  renderNotificationErrorList();
}

function closeNotificationErrorList() {
  notificationErrorList().hidden = true;
  notificationErrorTrayButton().setAttribute('aria-expanded', 'false');
  notificationErrorListPinned = false;
}

function renderNotificationErrorList() {
  const list = notificationErrorList();
  list.textContent = '';
  for (const [id, entry] of unresolvedErrors) {
    const item = document.createElement('div');
    item.className = 'notification-error-item';
    item.setAttribute('role', 'listitem');
    item.innerHTML =
      '<span class="notification-error-item-icon" aria-hidden="true">!</span>' +
      '<div class="notification-error-item-body">' +
      '<div class="notification-error-item-title"></div>' +
      '<div class="notification-error-item-message"></div>' +
      '</div>';
    item.querySelector('.notification-error-item-title').textContent = entry.title;
    item.querySelector('.notification-error-item-message').textContent = entry.message;
    list.appendChild(item);
  }
}

function updateNotificationErrorTray() {
  const count = unresolvedErrors.size;
  notificationErrorCount().textContent = String(count);
  notificationErrorTray().hidden = count === 0;
  if (count === 0) closeNotificationErrorList();
  if (isNotificationErrorListOpen()) renderNotificationErrorList();
}

function setupNotificationErrorTray() {
  const tray = notificationErrorTray();
  const button = notificationErrorTrayButton();
  button.addEventListener('click', () => {
    if (isNotificationErrorListOpen()) {
      closeNotificationErrorList();
    } else {
      openNotificationErrorList();
      notificationErrorListPinned = true;
    }
  });
  tray.addEventListener('pointerenter', () => {
    if (unresolvedErrors.size > 0) openNotificationErrorList();
  });
  tray.addEventListener('pointerleave', () => {
    if (!notificationErrorListPinned) closeNotificationErrorList();
  });
  button.addEventListener('focusin', () => {
    if (unresolvedErrors.size > 0) openNotificationErrorList();
  });
  button.addEventListener('focusout', () => {
    if (!notificationErrorListPinned) closeNotificationErrorList();
  });
}

function showHelp(topic) {
  let helpText = '';
  if (topic) {
    const found = slashCommands.find(c => c.cmd === '/' + topic || c.cmd.slice(1).startsWith(topic));
    if (found) {
      helpText = '## ' + found.cmd + '\n\n' + found.desc + '\n\nCategory: ' + (found.category || 'general');
    } else {
      helpText = 'Unknown command: /' + topic + '\n\nType /help for available commands.';
    }
  } else {
    helpText = '## Available Commands\n\n';
    const categories = {};
    for (const c of slashCommands) {
      const cat = c.category || 'other';
      if (!categories[cat]) categories[cat] = [];
      categories[cat].push(c);
    }
    for (const [cat, cmds] of Object.entries(categories)) {
      helpText += '### ' + cat.charAt(0).toUpperCase() + cat.slice(1) + '\n\n';
      for (const c of cmds) {
        helpText += '- **' + c.cmd + '** — ' + c.desc + '\n';
      }
      helpText += '\n';
    }
    helpText += '### Keyboard Shortcuts\n\n' +
      '- **' + bindingFor('sendMessage') + '** — Send message\n' +
      '- **' + bindingFor('insertNewline') + '** — New line\n' +
      '- **Double Escape** — Abort streaming\n' +
      '- **Escape** — Close panel\n' +
      '- **' + bindingFor('toggleThinking') + '** — Toggle thinking display\n' +
      '- **' + bindingFor('openModelPicker') + '** — Choose model\n' +
      '- **' + bindingFor('newSession') + '** — New session\n' +
      '- **' + bindingFor('openSettings') + '** — Open settings\n';
  }

  const container = document.getElementById('chatMessages');
  if (!container) return;
  const div = document.createElement('div');
  div.className = 'msg msg-assistant';
  div.innerHTML = '<div class="msg-avatar">?</div><div class="msg-body"><div class="msg-role">Help</div>' +
    '<div class="msg-content markdown-body">' + renderMarkdown(helpText) + '</div></div>';
  container.appendChild(div);
  container.scrollTop = container.scrollHeight;
}

function showHotkeys() {
  let hotkeysText = '## Keyboard Shortcuts\n\n| Key | Action |\n|-----|--------|\n';
  for (const definition of keyBindingDefinitions) {
    hotkeysText += `| ${definition.binding} | ${definition.title} |\n`;
  }
  for (const definition of FIXED_KEY_BINDINGS) {
    hotkeysText += `| ${definition.binding} | ${definition.title} |\n`;
  }

  const container = document.getElementById('chatMessages');
  if (!container) return;
  const div = document.createElement('div');
  div.className = 'msg msg-assistant';
  div.innerHTML = '<div class="msg-avatar">?</div><div class="msg-body"><div class="msg-role">Hotkeys</div>' +
    '<div class="msg-content markdown-body">' + renderMarkdown(hotkeysText) + '</div></div>';
  container.appendChild(div);
  container.scrollTop = container.scrollHeight;
}

function clampPercent(value) {
  if (!Number.isFinite(value)) return 0;
  return Math.max(0, Math.min(100, value));
}

function formatCompactTokens(value) {
  const n = Number(value || 0);
  if (n >= 1000000) return (n / 1000000).toFixed(n >= 10000000 ? 0 : 1) + 'm';
  if (n >= 1000) return Math.round(n / 1000) + 'k';
  return String(n);
}

function formatContextTooltip(usage) {
  const pct = Math.round(clampPercent(Number(usage.percent || 0)));
  const used = formatFullTokens(usage.tokens);
  const windowTokens = formatFullTokens(usage.contextWindow);
  const input = formatFullTokens(usage.inputTokens);
  const uncached = formatFullTokens(usage.uncachedInputTokens);
  const cached = formatFullTokens(usage.cachedInputTokens);
  const output = formatFullTokens(usage.outputTokens);
  return [
    'Context window: ' + pct + '%',
    'Used: ' + used + ' / ' + windowTokens,
    'Input: ' + input + ' (' + uncached + ' uncached + ' + cached + ' cached)',
    'Cached input: ' + cached,
    'Output: ' + output,
  ].join('\n');
}

function formatFullTokens(value) {
  const n = Number(value || 0);
  if (!Number.isFinite(n)) return '0';
  return Math.round(n).toLocaleString('en-US');
}

function formatResetTitle(label, window) {
  const resetText = formatResetAt(window.resetAt, window.resetAfterSecs);
  return resetText ? label + ' resets ' + resetText : label + ' reset time unknown';
}

function formatResetAt(resetAt, resetAfterSecs) {
  const resetAtSecs = Number(resetAt || 0);
  const resetAfter = Number(resetAfterSecs || 0);
  let time;
  if (Number.isFinite(resetAtSecs) && resetAtSecs > 0) {
    time = new Date(resetAtSecs * 1000);
  } else if (Number.isFinite(resetAfter) && resetAfter > 0) {
    time = new Date(Date.now() + resetAfter * 1000);
  } else {
    return '';
  }
  const now = new Date();
  const sameDay = time.toDateString() === now.toDateString();
  const clock = time.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  if (sameDay) return clock;
  return time.toLocaleDateString([], { month: 'numeric', day: 'numeric' }) + ' ' + clock;
}

function formatRateLimitSnapshot(snapshot) {
  if (!snapshot) return 'No rate limit data available';
  const parts = [];
  if (snapshot.planType) parts.push('Plan: ' + snapshot.planType);
  if (snapshot.primary) parts.push('5h: ' + Math.round(clampPercent(snapshot.primary.usedPercent)) + '% used');
  if (snapshot.secondary) parts.push('week: ' + Math.round(clampPercent(snapshot.secondary.usedPercent)) + '% used');
  if (snapshot.limitReached) parts.push('Rate limit reached');
  return parts.length ? parts.join(' | ') : 'No rate limit data available';
}

function copyCode(btn) {
  const block = btn.closest('.md-code-block');
  const code = block ? block.querySelector('code') : null;
  if (!code) return;
  navigator.clipboard.writeText(code.textContent).catch(() => {});
  btn.classList.add('copied');
  setTimeout(() => btn.classList.remove('copied'), 1200);
}

// =============== Markdown Rendering Engine ===============

function renderMarkdown(source) {
  if (!source) return '';
  const lines = source.replace(/\r\n/g, '\n').split('\n');
  const html = [];
  let para = [];
  let list = null;
  let quote = [];
  let tableState = null;

  const flushP = () => {
    if (para.length) {
      html.push('<p>' + inlineMd(para.join(' ')) + '</p>');
      para = [];
    }
  };
  const flushL = () => {
    if (list) {
      html.push('<' + list.t + '>' + list.items.map(i => {
        // Task list support
        if (i.startsWith('[x] ') || i.startsWith('[X] ')) {
          return '<li class="task-list-item"><input type="checkbox" checked disabled> ' + inlineMd(i.slice(4)) + '</li>';
        }
        if (i.startsWith('[ ] ')) {
          return '<li class="task-list-item"><input type="checkbox" disabled> ' + inlineMd(i.slice(4)) + '</li>';
        }
        return '<li>' + inlineMd(i) + '</li>';
      }).join('') + '</' + list.t + '>');
      list = null;
    }
  };
  const flushQ = () => {
    if (quote.length) {
      html.push('<blockquote>' + renderMarkdown(quote.join('\n')) + '</blockquote>');
      quote = [];
    }
  };
  const flushTable = () => {
    if (tableState) {
      html.push(renderTable(tableState));
      tableState = null;
    }
  };
  const flush = () => { flushP(); flushL(); flushQ(); flushTable(); };

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const trimmed = line.trim();

    // Code block
    if (/^```/.test(trimmed)) {
      flush();
      const lang = trimmed.slice(3).trim();
      const code = [];
      i++;
      while (i < lines.length && !/^```/.test(lines[i].trim())) {
        code.push(lines[i]);
        i++;
      }
      html.push(codeBlock(code.join('\n'), lang));
      continue;
    }

    // LaTeX block ($$)
    if (/^\$\$/.test(trimmed)) {
      flush();
      const latex = [];
      i++;
      while (i < lines.length && !/^\$\$/.test(lines[i].trim())) {
        latex.push(lines[i]);
        i++;
      }
      html.push('<div class="md-latex-block"><code>' + escapeHtml(latex.join('\n')) + '</code></div>');
      continue;
    }

    // Empty line
    if (!trimmed) { flush(); continue; }

    // Heading (h1-h6)
    const h = trimmed.match(/^(#{1,6})\s+(.+)$/);
    if (h) { flush(); html.push('<h' + h[1].length + '>' + inlineMd(h[2]) + '</h' + h[1].length + '>'); continue; }

    // Horizontal rule
    if (/^(-{3,}|\*{3,}|_{3,})$/.test(trimmed)) { flush(); html.push('<hr>'); continue; }

    // Table detection: line with | characters
    if (trimmed.includes('|') && trimmed.startsWith('|')) {
      const cells = parseTableRow(trimmed);
      if (cells) {
        if (!tableState) {
          flushP(); flushL(); flushQ();
          tableState = { header: cells, alignments: null, rows: [] };
        } else if (!tableState.alignments) {
          // This line should be the separator (|---|---|)
          const aligns = parseTableSeparator(trimmed);
          if (aligns) {
            tableState.alignments = aligns;
          } else {
            // Not a valid separator, treat header as a row
            tableState.rows.push(cells);
          }
        } else {
          tableState.rows.push(cells);
        }
        continue;
      }
    } else if (tableState) {
      flushTable();
    }

    // Blockquote
    const q = trimmed.match(/^>\s?(.*)$/);
    if (q) { flushP(); flushL(); flushTable(); quote.push(q[1]); continue; }

    // List items (unordered and ordered, including task lists)
    const ul = trimmed.match(/^[-*+]\s+(.+)$/);
    const ol = trimmed.match(/^\d+[.)]\s+(.+)$/);
    if (ul || ol) {
      flushP(); flushQ(); flushTable();
      const t = ul ? 'ul' : 'ol';
      if (!list || list.t !== t) flushL();
      if (!list) list = { t, items: [] };
      list.items.push((ul || ol)[1]);
      continue;
    }

    // Image
    const img = trimmed.match(/^!\[([^\]]*)\]\(([^)]+)\)$/);
    if (img) {
      flush();
      html.push('<div class="md-image"><img src="' + escapeHtml(img[2]) + '" alt="' + escapeHtml(img[1]) + '" style="max-width:100%;border-radius:var(--radius)"></div>');
      continue;
    }

    // Default: paragraph text
    flushL(); flushQ(); flushTable();
    para.push(trimmed);
  }
  flush();
  return html.join('');
}

function parseTableRow(line) {
  const trimmed = line.trim();
  if (!trimmed.startsWith('|')) return null;
  // Split by | and trim
  const parts = trimmed.split('|').slice(1); // Remove first empty
  if (parts.length === 0) return null;
  // Remove last if empty (trailing |)
  if (parts[parts.length - 1].trim() === '') parts.pop();
  return parts.map(p => p.trim());
}

function parseTableSeparator(line) {
  const cells = parseTableRow(line);
  if (!cells) return null;
  const aligns = [];
  for (const cell of cells) {
    const trimmed = cell.trim();
    if (!/^:?-+:?$/.test(trimmed)) return null;
    if (trimmed.startsWith(':') && trimmed.endsWith(':')) aligns.push('center');
    else if (trimmed.endsWith(':')) aligns.push('right');
    else aligns.push('left');
  }
  return aligns;
}

function renderTable(state) {
  const aligns = state.alignments || state.header.map(() => 'left');
  let html = '<div class="md-table-wrap"><table class="md-table"><thead><tr>';
  for (let i = 0; i < state.header.length; i++) {
    const align = aligns[i] || 'left';
    html += '<th style="text-align:' + align + '">' + inlineMd(state.header[i]) + '</th>';
  }
  html += '</tr></thead><tbody>';
  for (const row of state.rows) {
    html += '<tr>';
    for (let i = 0; i < row.length; i++) {
      const align = aligns[i] || 'left';
      html += '<td style="text-align:' + align + '">' + inlineMd(row[i]) + '</td>';
    }
    html += '</tr>';
  }
  html += '</tbody></table></div>';
  return html;
}

function inlineMd(raw) {
  if (!raw) return '';
  // Protect code spans first
  const spans = [];
  let t = raw.replace(/`([^`]+)`/g, (_, c) => {
    spans.push('<code>' + escapeHtml(c) + '</code>');
    return '\x00' + (spans.length - 1) + '\x00';
  });

  // Protect inline LaTeX ($...$)
  t = t.replace(/\$([^$\n]+)\$/g, (_, c) => {
    spans.push('<code class="md-latex-inline">' + escapeHtml(c) + '</code>');
    return '\x00' + (spans.length - 1) + '\x00';
  });

  let h = escapeHtml(t);

  // Bold (** and __)
  h = h.replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>');
  h = h.replace(/__(.+?)__/g, '<strong>$1</strong>');

  // Strikethrough
  h = h.replace(/~~(.+?)~~/g, '<del>$1</del>');

  // Highlight (==text==)
  h = h.replace(/==(.+?)==/g, '<mark>$1</mark>');

  // Italic (* and _)
  h = h.replace(/\*([^*\n]+)\*/g, '<em>$1</em>');
  h = h.replace(/(?<![a-zA-Z0-9])_([^_\n]+)_(?![a-zA-Z0-9])/g, '<em>$1</em>');

  // Images inline
  h = h.replace(/!\[([^\]]*)\]\(([^)]+)\)/g, '<img src="$2" alt="$1" style="max-height:200px;border-radius:4px">');

  // Links
  h = h.replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2" target="_blank" rel="noreferrer">$1</a>');

  // Restore protected spans
  h = h.replace(/\x00(\d+)\x00/g, (_, i) => spans[+i] || '');

  return h;
}

function codeBlock(code, lang) {
  const label = escapeHtml((lang || '').toLowerCase() || 'text');
  return '<div class="md-code-block"><div class="md-code-head"><span class="md-code-lang">' + label +
    '</span><button class="md-copy" onclick="copyCode(this)" aria-label="Copy code">' +
    '<svg viewBox="0 0 16 16" fill="none" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">' +
    '<rect x="5" y="4" width="8" height="9" rx="1.5"/><path d="M3 10.5V3.5A1.5 1.5 0 014.5 2h6"/></svg>' +
    '</button></div><pre><code>' + escapeHtml(code) + '</code></pre></div>';
}

// =============== HTML Escaping ===============

function escapeHtml(s) {
  if (!s) return '';
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

function applyFontSize(value) {
  const fontSize = clampFontSize(value);
  const root = document.documentElement;
  root.style.setProperty('--ui-font-size', fontSize + 'px');
  root.style.setProperty('--ui-scale', String(fontSize / 14));
}
