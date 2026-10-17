// @ts-nocheck
"use strict";

// ===================================================================
// Rozsa GUI — Electron renderer
//
// Internal Framework:
// app.js
// +-- Initialization (DOMContentLoaded, Electron bridge binding, event listeners)
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
// Runtime-only design gaps. Passive message/error delivery remains active until
// its state matrix has a safe shared replacement (UI-CONV-002).
const conversationSuspensions = Object.freeze({
  autocomplete: 'UI-COMPOSER-002',
  attachments: 'UI-COMPOSER-003',
  modelPicker: 'UI-COMPOSER-004',
  runningSubmit: 'UI-FLOW-001',
  fork: 'UI-FLOW-001',
  subagents: 'UI-FLOW-001',
});

function conversationOperationSuspended(operation) {
  return Object.hasOwn(conversationSuspensions, operation);
}

function suspendedConversationCommand(text) {
  const operations = { model: 'modelPicker', fork: 'fork', subagent: 'subagents', subagents: 'subagents' };
  return text.split(/\s+/).some(token => token.startsWith('/') &&
    conversationOperationSuspended(operations[token.slice(1).toLowerCase()]));
}

function configureConversationSuspensions() {
  for (const id of ['autocomplete', 'forkPicker', 'subagentPanel']) {
    const slot = document.getElementById(id);
    if (!slot) continue;
    slot.replaceChildren();
    slot.hidden = true;
    slot.dataset.uiSuspended = id === 'autocomplete' ? 'UI-COMPOSER-002' : 'UI-FLOW-001';
  }
  for (const action of ['attach-file', 'attach-directory', 'model-picker', 'running-send-mode']) {
    const control = document.querySelector('[data-action="' + action + '"]');
    if (!control) continue;
    control.disabled = true;
    control.setAttribute('aria-disabled', 'true');
    control.title = 'Unavailable until its UI design is complete';
  }
  const composer = document.getElementById('composer');
  for (const id of ['attachmentReferences', 'attachmentDragFeedback', 'modelPickerSlot']) {
    const slot = document.createElement('div');
    slot.id = id;
    slot.hidden = true;
    slot.dataset.uiSuspended = id === 'modelPickerSlot' ? 'UI-COMPOSER-004' : 'UI-COMPOSER-003';
    composer?.appendChild(slot);
  }
  // Streaming, continuation and real errors cannot safely be hidden. Their
  // existing state handling is retained and must survive T036 cleanup.
  document.getElementById('chatMessages')?.setAttribute('data-ui-migration-blocked', 'UI-CONV-002');
}
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
let conversationPersistenceErrorShown = false;

function restoreConversationState() {
  try {
    const views = sessionStorage.getItem('rozsa:conversation-views:v1');
    const drafts = sessionStorage.getItem('rozsa:conversation-drafts:v1');
    if (views !== null) {
      const value = JSON.parse(views);
      if (!value || typeof value !== 'object' || Array.isArray(value) || Object.values(value).some(view =>
        !view || typeof view !== 'object' || !Number.isFinite(view.scrollTop) || typeof view.autoScrollPaused !== 'boolean')) {
        throw new Error('Stored conversation view state is malformed');
      }
      sessionViewState = value;
    }
    if (drafts !== null) {
      const value = JSON.parse(drafts);
      if (!value || typeof value !== 'object' || Array.isArray(value) || Object.values(value).some(draft =>
        !draft || typeof draft !== 'object' || typeof draft.text !== 'string' || !Number.isInteger(draft.selectionStart)
        || !Number.isInteger(draft.selectionEnd) || typeof draft.runningSendMode !== 'string')) {
        throw new Error('Stored conversation draft state is malformed');
      }
      sessionDraftState = value;
    }
  } catch (error) {
    showError('Conversation state could not be restored: ' + String(error));
  }
}

function persistConversationState() {
  try {
    sessionStorage.setItem('rozsa:conversation-views:v1', JSON.stringify(sessionViewState));
    sessionStorage.setItem('rozsa:conversation-drafts:v1', JSON.stringify(sessionDraftState));
  } catch (error) {
    if (!conversationPersistenceErrorShown) {
      conversationPersistenceErrorShown = true;
      showError('Conversation state could not be saved: ' + String(error));
    }
  }
}
let permissionUiStateBySession = {};
let restoringSessionScroll = false;
let scrollStateFrame = 0;
let sidebarCollapsed = false;
let sidebarAutoCollapsed = false;
let nativeSplitMode = false;
// 由 prototype 的 220px sidebar + 560px 主内容舒适下限测得；820px 为
// 820px 分屏基线，作为展开滞回边界，避免临界区间抖动。
const SIDEBAR_AUTO_COLLAPSE_MAX_WIDTH = 780;
const SIDEBAR_AUTO_EXPAND_MIN_WIDTH = 820;
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
  try {
    if (!window.RozsaUIReady) throw new Error('Shared UI entry not loaded');
    await window.RozsaUIReady;
  } catch (error) {
    showError('Shared UI failed to load: ' + String(error));
    return;
  }
  restoreConversationState();
  nativeSplitMode = false;
  preparePlatformSceneDom();
  setupChatScrollLock();
  if (!nativeSplitMode) {
    syncMainSidebarViewport();
    syncChromeBackgroundGeometry();
  }

  if (!window.rozsa) {
    document.getElementById('chatMessages').innerHTML =
      '<div class="chat-empty"><div class="chat-empty-icon">!</div>' +
      '<div class="chat-empty-title">Electron bridge not loaded</div></div>';
    return;
  }
  invoke = window.RozsaUIHost.port.request;
  listen = async (event, receive) => {
    try {
      return await window.RozsaUIHost.port.subscribe(event, payload => receive({ payload }));
    } catch (error) {
      console.warn(`Electron backend event ${event} is unavailable:`, error);
      return () => {};
    }
  };
  configureAttachmentPicker();
  await configureNativeFileDrag();
  setupComposerHints();
  const composerInput = document.getElementById('msgInput');
  let draftPersistFrame = 0;
  const persistActiveDraft = () => {
    if (!activeSessionId || draftPersistFrame) return;
    draftPersistFrame = requestAnimationFrame(() => {
      draftPersistFrame = 0;
      captureSessionDraft(activeSessionId);
    });
  };
  composerInput?.addEventListener('input', persistActiveDraft);
  document.addEventListener('selectionchange', () => {
    if (composerInput?.contains(document.getSelection()?.anchorNode || null)) persistActiveDraft();
  });
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
  try {
    await window.RozsaNotifications.subscribeNotifications(window.RozsaUIHost.port, { upsertNotification, resolveNotification, showNotification, showError });
  } catch (error) {
    console.warn('Electron backend notifications are unavailable:', error);
  }
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
  try { models = await invoke('list_models'); renderModelSelector(); } catch (e) { console.warn('Electron model registry is unavailable:', e); }
  loadSettings().catch(error => console.warn('Electron settings are unavailable:', error));
  loadKeyBindings().catch(error => console.warn('Electron key bindings are unavailable:', error));
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

