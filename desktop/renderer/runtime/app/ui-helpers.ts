// @ts-nocheck
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
  window.RozsaSidebar.suspendQuotaTooltip();
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
    const messageIndex = Number(message.dataset.odId?.replace(/^message-/, ''));
    if (Number.isInteger(messageIndex) && messageIndex >= 0) {
      expandedThinkingBySession[thinkingStateKey(activeSessionId, messageIndex)] = expanded;
    }
  }
}

function showError(message) {
  if (window.RozsaNotifications) {
    window.RozsaNotifications.showRuntimeError(message, { upsertNotification, nextErrorId: () => 'runtime-error-' + (++legacyNotificationCounter) });
    return;
  }
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
