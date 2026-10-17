// @ts-nocheck
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
  bar.classList.remove('warn');
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
  if (group) group.hidden = !(nextEligible && quotaDisplayEnabled);
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
    if (conversationOperationSuspended('runningSubmit')) {
      if (mode) mode.hidden = true;
      window.RozsaComposer.renderSendState(sendBtn, 'stop');
      return;
    }
    const hasText = !!getInputText(input).trim();
    if (mode) mode.hidden = !hasText;
    if (hasText) {
      if (mode && !mode.dataset.initialized) {
        mode.value = currentSettings?.running_send_mode || 'queue';
        mode.dataset.initialized = 'true';
      }
      const selected = mode ? mode.value : 'queue';
      window.RozsaComposer.renderSendState(sendBtn, selected === 'steer' ? 'steer' : 'queue');
    } else {
      window.RozsaComposer.renderSendState(sendBtn, 'stop');
    }
  } else {
    if (mode) mode.hidden = true;
    window.RozsaComposer.renderSendState(sendBtn, 'send');
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
  if (sessionChanged) {
    conversationEntryCache.clear();
    messageToolResultCache.clear();
  }
  const savedView = sessionId ? sessionViewState[sessionId] : null;
  if (sessionChanged) chatAutoScrollPaused = savedView?.autoScrollPaused === true;
  const restoringScroll = sessionChanged && savedView && Number.isFinite(savedView.scrollTop);
  const shouldStickToBottom = !restoringScroll && !chatAutoScrollPaused &&
    (sessionChanged ? !savedView : isChatNearBottom(container));

  const allMessages = Array.isArray(messages) ? messages : [];
  const toolResultMap = Object.create(null);
  for (const raw of allMessages) {
    if (raw.kind !== 'standard' || !raw.message || raw.message.role !== 'toolResult') continue;
    const message = raw.message;
    const id = message.toolCallId;
    if (!id) continue;
    const resultKey = String(sessionId || '') + ':' + id;
    const previous = messageToolResultCache.get(resultKey);
    if (previous?.raw === raw) toolResultMap[id] = previous.value;
    else {
      const text = (message.content || []).filter(block => block.type === 'text').map(block => block.text).join('\n');
      const value = { output: text, isError: !!message.isError, toolName: message.toolName || '', details: message.details || {} };
      messageToolResultCache.set(resultKey, { raw, value });
      toolResultMap[id] = value;
    }
  }
  const visibleMessages = allMessages.filter(raw => !(raw.kind === 'standard' && raw.message && raw.message.role === 'toolResult'));
  const lastAssistantIndex = streaming ? -1 : visibleMessages.reduce((last, raw, index) =>
    raw.kind === 'standard' && raw.message && raw.message.role === 'assistant' ? index : last, -1);
  const summariesByRawIndex = new Map((Array.isArray(turnSummaries) ? turnSummaries : [])
    .map(summary => [summary.assistantMessageIndex, summary.activity]));
  const activityForVisibleIndex = index => {
    const rawIndex = allMessages.indexOf(visibleMessages[index]);
    return summariesByRawIndex.get(rawIndex) || (index === lastAssistantIndex ? turnActivity : null);
  };
  const activeStreamIndex = activeStreamMessageIndex(allMessages, visibleMessages, streaming);
  const thinkingDurationForIndex = updateThinkingTimings(allMessages, visibleMessages, activeStreamIndex, sessionId);
  const entries = visibleMessages.map((raw, index) => {
    const activity = activityForVisibleIndex(index);
    const expanded = isThinkingExpanded(sessionId, index);
    const showIdentity = assistantMessageGroupPosition(visibleMessages, index) !== 'continuation';
    const messageIdentity = raw.entryId || raw.message?.timestamp || 'message';
    const cacheKey = String(sessionId || '') + ':' + String(messageIdentity) + ':' + index;
    const cached = conversationEntryCache.get(cacheKey);
    const active = index === activeStreamIndex;
    const duration = thinkingDurationForIndex(index);
    const resultRefs = raw.message?.role === 'assistant'
      ? (raw.message.content || []).filter(block => block.type === 'toolCall').map(block => toolResultMap[block.id] || null)
      : [];
    if (cached && cached.raw === raw && cached.activity === activity && cached.expanded === expanded
      && cached.showIdentity === showIdentity && cached.active === active && cached.duration === duration
      && cached.resultRefs.length === resultRefs.length && cached.resultRefs.every((value, resultIndex) => value === resultRefs[resultIndex])) return cached.entry;
    const entry = createConversationEntry(raw, toolResultMap, active, activity, duration, expanded, showIdentity, index, sessionId);
    conversationEntryCache.set(cacheKey, { raw, activity, expanded, showIdentity, active, duration, resultRefs, entry });
    return entry;
  });

  toolCounts = countTools(visibleMessages);
  renderToolChips();
  if (!window.RozsaConversation?.renderView) throw new Error('React conversation renderer is unavailable');
  window.RozsaConversation.renderView({ entries, markdown: renderMarkdown, empty: entries.length === 0 });
  renderedMessageSessionId = sessionId;
  renderedRawMessageCount = allMessages.length;
  if (restoringScroll || shouldStickToBottom) {
    requestAnimationFrame(() => {
      if (restoringScroll) restoreSessionViewState(sessionId, container, savedView);
      else scrollChatToBottom(container);
    });
  }
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

