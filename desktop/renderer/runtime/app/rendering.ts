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
  const savedView = sessionId ? sessionViewState[sessionId] : null;
  if (sessionChanged) chatAutoScrollPaused = savedView?.autoScrollPaused === true;
  const restoringScroll = sessionChanged && savedView && Number.isFinite(savedView.scrollTop);
  const shouldStickToBottom = !restoringScroll && !chatAutoScrollPaused &&
    (sessionChanged ? !savedView : isChatNearBottom(container));

  if (!messages || messages.length === 0) {
    container.innerHTML = window.RozsaConversation.emptyConversationMarkup();
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
      const assistantGroupPosition = assistantMessageGroupPosition(visibleMessages, i);
      container.appendChild(renderMessage(
        visibleMessages[i],
        toolResultMap,
        i === activeStreamIndex,
        activityForVisibleIndex(i),
        thinkingDurationForIndex(i),
        isThinkingExpanded(sessionId, i),
        assistantGroupPosition !== 'continuation',
        i,
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

