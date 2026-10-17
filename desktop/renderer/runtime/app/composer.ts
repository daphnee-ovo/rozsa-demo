// @ts-nocheck
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
  persistConversationState();
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
  if (isStreaming && conversationOperationSuspended('runningSubmit')) return;
  const input = document.getElementById('msgInput');
  if (!input) return;
  const text = getInputText(input).trim();
  if (!text) return;
  if (suspendedConversationCommand(text)) return;
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
  if (suspendedConversationCommand(text)) return true;
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
  if (conversationOperationSuspended('fork')) return;
  const panel = document.getElementById('forkPicker');
  if (!panel) return;
  try {
    const points = await invoke('get_fork_points');
    if (conversationOperationSuspended('fork')) return;
    panel.hidden = false;
    panel.innerHTML = '<div class="running-messages-title">Fork conversation</div><ol>' +
      (points.length ? points.map(point => '<li><button class="turn-activity-file" onclick="forkAtMessage(' + point.messageIndex + ')">' +
        escapeHtml(point.label) + '</button></li>').join('') : '<li>No user messages available.</li>') + '</ol>';
  } catch (error) { showError(String(error)); }
}

async function forkAtMessage(messageIndex) {
  if (conversationOperationSuspended('fork')) return;
  try {
    await invoke('fork_session', { messageIndex });
    sessions = await invoke('get_sessions');
    renderSessionList();
    document.getElementById('forkPicker').hidden = true;
    showNotification('Created forked session');
  } catch (error) { showError(String(error)); }
}

async function showSubagentPanel() {
  if (conversationOperationSuspended('subagents')) return;
  const panel = document.getElementById('subagentPanel');
  if (!panel) return;
  try {
    const subagents = await invoke('get_subagents');
    if (conversationOperationSuspended('subagents')) return;
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

