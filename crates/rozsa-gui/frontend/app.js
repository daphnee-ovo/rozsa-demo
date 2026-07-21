"use strict";

// ===================================================================
// Rozsa GUI — Tauri IPC Frontend (Full TUI Feature Parity)
//
// Internal Framework:
// app.js
// +-- Initialization (DOMContentLoaded, Tauri API binding, event listeners)
// +-- State Rendering (renderState, updateHeader, updateSidebar, renderMessages)
// +-- Message Rendering (renderMessage, extractText, extractThinking)
// +-- Markdown Engine (renderMarkdown, inlineMd, codeBlock, renderTable)
// +-- Tool Events (handleToolEvent, trackTool, renderToolChips, toolIcon)
// +-- Permissions (showPermission, respondPermission, hidePermPanel)
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
let toolCounts = {};
let currentSettings = null;
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
const guiSceneState = { revision: 0, scene: 'main', selectedPane: null };
const mainThemeState = { revision: 0 };
let pendingGuiSceneSnapshot = null;
let pendingGuiSceneIntent = null;
let mainSceneContinuity = null;
const TRANSIENT_POPUP_IDS = ['autocomplete', 'forkPicker', 'subagentPanel', 'quotaTooltip'];

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
  { cmd: '/thinking', desc: 'Set thinking level (off/low/medium/high)', category: 'model' },
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

  await listen('gui-scene-snapshot', ev => applyGuiSceneSnapshot(ev.payload));
  await listen('theme-state', ev => applyMainThemeState(ev.payload));
  await listen('ui-state', ev => renderState(ev.payload));
  await listen('tool-event', ev => handleToolEvent(ev.payload));
  await listen('permission-request', ev => showPermission(ev.payload));
  await listen('error', ev => showError(typeof ev.payload === 'string' ? ev.payload : JSON.stringify(ev.payload)));
  await listen('notification', ev => showNotification(typeof ev.payload === 'string' ? ev.payload : JSON.stringify(ev.payload)));
  await listen('native-sidebar-toggle', () => {
    if (!nativeSplitMode) toggleMainSidebar();
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
  refreshRateLimits(false);
});

window.addEventListener('resize', syncMainSidebarViewport);
window.addEventListener('resize', syncChromeBackgroundGeometry);
window.addEventListener('resize', scheduleNativeFullscreenSync);
window.addEventListener('pointermove', handleSidebarEdgeReveal);

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
  // 记录当前活跃 session 的 streaming 状态
  if (snap.sessionId) {
    activeSessionId = snap.sessionId;
    const approvals = pendingPermissions[snap.sessionId] || [];
    sessionStreamingState[snap.sessionId] = approvals.length ? 'approval' : (isStreaming ? 'running' : 'idle');
  }
  if (snap.streamUpdate) {
    renderMessages(snap.messages, true, snap.sessionId || null, snap.turnActivity, snap.turnSummaries);
    return;
  }
  updateHeader(snap);
  if (!nativeSplitMode) updateSidebar(snap);
  renderMessages(snap.messages, snap.isStreaming, snap.sessionId || null, snap.turnActivity, snap.turnSummaries);
  renderRunningMessages(snap.queuedMessages, snap.steeringConversation);
  updateAbortButton();
  if (!nativeSplitMode) renderSessionList();
  if (sessionChanged) restoreSessionDraft(snap.sessionId);
  schedulePermPanelDisplay();
}

function updateHeader(snap) {
  const nameEl = document.getElementById('currentSessionName');
  if (nameEl && !snap.streamUpdate) nameEl.textContent = snap.sessionName || 'Rózsa';

  const modelBtn = document.getElementById('modelSelector');
  if (modelBtn && snap.model) modelBtn.textContent = snap.model.id;

  const thinkingLevel = document.getElementById('thinkingLevel');
  if (thinkingLevel && snap.thinkingLevel) thinkingLevel.textContent = snap.thinkingLevel;
}

function updateQuotaBars(snapshot) {
  const hourBar = document.getElementById('quotaHourBar');
  const hourVal = document.getElementById('quotaHour');
  const weekBar = document.getElementById('quotaWeekBar');
  const weekVal = document.getElementById('quotaWeek');
  updateQuotaWindow(hourBar, hourVal, snapshot && snapshot.primary, '5 hours');
  updateQuotaWindow(weekBar, weekVal, snapshot && snapshot.secondary, 'This week');
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
  bar.style.width = used + '%';
  bar.classList.toggle('warn', used >= 80);
  valueEl.textContent = Math.round(used) + '%';
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
    updateQuotaBars(snapshot);
    quotaLoaded = true;
    if (showResult) showNotification(formatRateLimitSnapshot(snapshot));
  } catch (e) {
    updateQuotaBars(null);
    quotaLoaded = true;
    if (showResult) showError('Rate limit query failed: ' + String(e));
  } finally {
    quotaLoading = false;
  }
}

function updateSidebar(snap) {
  updateQuotaVisibility(snap.model);

  if (snap.contextUsage) {
    const pct = clampPercent(Number(snap.contextUsage.percent || 0));
    const tokens = Number(snap.contextUsage.tokens || 0);
    const ctxEl = document.getElementById('contextTokens');
    if (ctxEl) ctxEl.textContent = formatCompactTokens(tokens);
    const ring = document.querySelector('.context-ring circle:last-child');
    if (ring) ring.setAttribute('stroke-dashoffset', 44 - (44 * pct / 100));
    const ringWrap = document.querySelector('.context-ring');
    if (ringWrap) {
      ringWrap.removeAttribute('title');
      ringWrap.dataset.quotaTooltip = formatContextTooltip(snap.contextUsage);
    }
  }

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

function updateQuotaVisibility(model) {
  const nextEligible = modelHasRateLimit(model);
  const nextKey = modelKey(model);
  const group = document.getElementById('quotaGroup');
  quotaEligible = nextEligible;
  if (group) group.style.display = nextEligible ? '' : 'none';
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
  if (!quotaLoaded) refreshRateLimits(false);
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
    JSON.stringify(activityForVisibleIndex(index)) + ':' + (thinkingDurationForIndex(index) ?? ''));
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
      const tcStatus = result ? (result.isError ? 's-error' : 's-success') : 's-running';
      const toolTitle = formatToolTitle(tc);
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
      }

      body += '<div class="tool-call' + (isToolCallExpanded(tc.id) ? ' expanded' : '') +
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

function formatToolArgs(tc) {
  if (!tc.arguments) return '';
  if (typeof tc.arguments === 'string') return tc.arguments;
  // Show key fields for known tools
  const args = tc.arguments;
  if (tc.name === 'Bash' && args.command) return args.command;
  if (tc.name === 'Read' && args.file_path) return args.file_path;
  if (tc.name === 'Write' && args.file_path) return args.file_path;
  if (tc.name === 'Edit' && args.file_path) return args.file_path + ' (edit)';
  return JSON.stringify(args);
}

function formatToolTitle(tc) {
  const name = tc.name || 'Tool';
  const args = tc.arguments || {};
  if (typeof args === 'string') return { name, arg: args };
  if (name === 'Bash' && args.command) return { name, arg: args.command };
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
}

async function onModelChange(modelId) {
  try {
    await invoke('switch_model', { modelId: modelId });
    const state = await invoke('get_state');
    renderState(state);
    currentSettings = await invoke('get_settings');
    renderSettingsPane(currentSettings);
    const btn = document.getElementById('modelSelector');
    if (btn) btn.textContent = modelId;
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

// =============== Settings Panel ===============

function isSidebarCollapsed() {
  return sidebarCollapsed || sidebarAutoCollapsed;
}

function preparePlatformSceneDom() {
  document.body.classList.toggle('native-split-main', nativeSplitMode);
  if (nativeSplitMode) return;
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
    renderSettingsSelection(snapshot.selectedPane || 'appearance');
    loadSettings().catch(() => {});
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

function handleSidebarEdgeReveal(event) {
  if (nativeSplitMode) return;
  const collapsed = isSidebarCollapsed();
  const settingsPanel = document.getElementById('settingsPanel');
  const settingsVisible = settingsPanel?.classList.contains('visible');
  const edgeVisible = event.clientX <= 18;
  if (settingsVisible) {
    settingsPanel.classList.toggle('settings-edge-visible', collapsed && edgeVisible);
    return;
  }
  const appBody = document.querySelector('[data-od-id="app-body"]');
  appBody?.classList.toggle('sidebar-edge-visible', collapsed && edgeVisible);
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
    loadSettings().catch(() => {});
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
    currentSettings = await invoke('get_settings');
    availableThemes = await invoke('list_themes');
    renderSettingsPane(currentSettings);
    await applySelectedTheme();
  } catch (e) {
    console.warn('appearance settings:', e);
    currentSettings = {};
    availableThemes = [];
    showError('Failed to load appearance settings: ' + String(e));
    throw e;
  }
}

function renderSettingsPane(settings) {
  if (!settings) return;

  // Thinking level
  const thinkingSel = document.getElementById('settingsThinking');
  if (thinkingSel && settings.thinking_level) {
    thinkingSel.value = settings.thinking_level.toLowerCase();
  }

  // Permission mode
  const permSel = document.getElementById('settingsPermMode');
  if (permSel && settings.permissions && settings.permissions.mode) {
    permSel.value = settings.permissions.mode;
  }

  // Auto-approve patterns
  const autoApproveEl = document.getElementById('settingsAutoApprove');
  if (autoApproveEl && settings.permissions && settings.permissions.autoApprovePatterns) {
    const patterns = settings.permissions.autoApprovePatterns;
    let html = '<div class="settings-group-label">Auto-Approve Patterns</div>';
    if (patterns.length === 0) {
      html += '<div style="font-size:11px;color:var(--muted);padding:4px 0">No patterns configured</div>';
    } else {
      for (const p of patterns) {
        html += '<div class="setting-item"><span class="setting-label" style="font-family:var(--font-mono);font-size:11px">' +
          escapeHtml(p) + '</span></div>';
      }
    }
    autoApproveEl.innerHTML = html;
  }

  // Model selector in settings
  const modelSel = document.getElementById('settingsModelSelect');
  if (modelSel && settings.model_id) {
    modelSel.value = settings.model_id;
  }

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

  renderThemeSelect('light', appearance.lightTheme);
  renderThemeSelect('dark', appearance.darkTheme);
  renderThemeControls('light', themeDefinitions.light, appearance.isMacos);
  renderThemeControls('dark', themeDefinitions.dark, appearance.isMacos);
  installSystemThemeListener();
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
  const sidebar = document.getElementById(prefix + 'ThemeTranslucentSidebar');
  const sidebarOption = document.getElementById(prefix + 'ThemeSidebarOption');
  setSettingSwitch(sidebar, theme.translucentSidebar);
  if (sidebarOption) sidebarOption.hidden = !isMacos;

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
  if (sidebar) {
    sidebar.onclick = () => {
      setSettingSwitch(sidebar, !isSettingSwitchOn(sidebar));
      previewTheme(mode);
      scheduleThemeSave(mode);
    };
  }
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
    translucentSidebar: isSettingSwitchOn(document.getElementById(prefix + 'ThemeTranslucentSidebar')),
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
  root.setAttribute('data-theme-translucent-sidebar', theme.translucentSidebar && isMacos ? 'true' : 'false');
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
  if (!Number.isFinite(parsed)) return 13;
  return Math.min(50, Math.max(5, parsed));
}

function renderGeneralSettings(settings) {
  // Thinking level
  const thinkingSel = document.getElementById('settingsThinking');
  if (thinkingSel) {
    if (settings.thinking_level) thinkingSel.value = settings.thinking_level.toLowerCase();
    thinkingSel.onchange = () => saveSetting('thinking', thinkingSel.value);
  }

  // Auto compact
  const compactSwitch = document.getElementById('settingsAutoCompact');
  if (compactSwitch) {
    setSettingSwitch(compactSwitch, settings.auto_compact);
    wireSettingSwitch('settingsAutoCompact', enabled => saveSetting('auto_compact', String(enabled)));
  }

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

  // Transport
  const transportSel = document.getElementById('settingsTransport');
  if (transportSel) {
    if (settings.transport) transportSel.value = settings.transport;
    transportSel.onchange = () => saveSetting('transport', transportSel.value);
  }

  // Permission mode
  const permSel = document.getElementById('settingsPermMode');
  if (permSel) {
    if (settings.permission_mode) permSel.value = settings.permission_mode;
    permSel.onchange = () => saveSetting('permission_mode', permSel.value);
  }
}

async function saveSetting(key, value) {
  try {
    await invoke('update_setting', { key: key, value: value });
    await loadSettings();
  } catch (e) {
    console.warn('update_setting failed:', e);
    showError('Failed to save setting: ' + key);
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
  const text = getInputText(input);
  const selection = getInputSelection(input);
  if (inputHighlightRanges.length === 0) {
    setInputText(input, text);
    setInputSelection(input, selection.start, selection.end);
    autoResize(input);
    return;
  }
  const chars = Array.from(text);
  const fragment = document.createDocumentFragment();
  let cursor = 0;
  const normalized = inputHighlightRanges
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
  autoResize(input);
}

function syncInputHighlightScroll() {
  // contenteditable renders highlights directly; no overlay scroll sync is needed.
}

// =============== Keyboard Shortcuts ===============

document.addEventListener('keydown', function(e) {
  const input = document.getElementById('msgInput');

  // IME owns composition keystrokes. Let the browser keep the preedit text
  // intact; Enter/send, autocomplete, and DOM replacement run after commit.
  if (isInputComposing || e.isComposing || e.keyCode === 229) return;

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
    if (isStreaming) { e.preventDefault(); abortAgent(); return; }
    return;
  }

  // Global shortcuts
  if (e.ctrlKey && e.key === 't') {
    e.preventDefault();
    document.body.classList.toggle('thinking-expanded');
    return;
  }

  if (e.ctrlKey && e.key === 'p') {
    e.preventDefault();
    showModelPicker();
    return;
  }

  if (e.ctrlKey && e.key === 'n') {
    e.preventDefault();
    newSession();
    return;
  }

  if (e.ctrlKey && e.key === ',') {
    e.preventDefault();
    toggleSettings();
    return;
  }

  // Input field shortcuts
  if (document.activeElement === input) {
    if (e.key === 'Enter' && e.shiftKey) {
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

    if (e.key === 'Enter' && !e.shiftKey) {
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
  if (e.key === '/' && document.activeElement !== input && !e.ctrlKey && !e.metaKey) {
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

function showNotification(message) {
  const container = document.getElementById('chatMessages');
  if (!container) return;
  const div = document.createElement('div');
  div.className = 'msg msg-assistant';
  div.innerHTML = '<div class="msg-avatar" style="background:var(--success-bg);color:var(--success)">i</div>' +
    '<div class="msg-body"><div class="msg-role">System</div>' +
    '<div class="msg-content"><p>' + escapeHtml(message) + '</p></div></div>';
  container.appendChild(div);
  container.scrollTop = container.scrollHeight;
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
      '- **Enter** — Send message\n' +
      '- **Shift+Enter** — New line\n' +
      '- **Escape** — Abort streaming / close panel\n' +
      '- **Ctrl+T** — Toggle thinking display\n' +
      '- **Ctrl+N** — New session\n' +
      '- **Ctrl+,** — Open settings\n';
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
  const hotkeysText =
    '## Keyboard Shortcuts\n\n' +
    '| Key | Action |\n' +
    '|-----|--------|\n' +
    '| Enter | Send message |\n' +
    '| Shift+Enter | New line |\n' +
    '| Escape | Abort / Close panel |\n' +
    '| Ctrl+T | Toggle thinking |\n' +
    '| Ctrl+N | New session |\n' +
    '| Ctrl+, | Settings |\n' +
    '| Y/T/N/H | Permission response |\n' +
    '| Tab | Confirm autocomplete |\n' +
    '| Up/Down | Navigate autocomplete |\n';

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
  root.style.setProperty('--ui-scale', String(fontSize / 13));
}
