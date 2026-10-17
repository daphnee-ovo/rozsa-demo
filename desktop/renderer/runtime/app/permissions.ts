// @ts-nocheck
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
      currentPermissionTrustIndex = -1;
      currentPermissionTrustKeys = [];
      const panel = document.getElementById('permPanel');
      if (!panel) return;
      const tool = document.getElementById('permTool');
      const cmd = document.getElementById('permCmd');
      const cmdToggle = document.getElementById('permCmdToggle');
      const desc = document.getElementById('permDesc');
      if (tool) tool.textContent = ev.tool || '—';
      const command = ev.command || ev.summary || '—';
      const covered = queue.length === 1 && !/[;\n|&]/.test(command);
      panel.dataset.interruptionUi = covered ? 'shared' : 'legacy';
      if (cmd) {
        cmd.innerHTML = covered
          ? window.RozsaPermissions.permissionCommandMarkup(command, ev.tool || '')
          : renderPermissionCommand(command, ev.tool || '');
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
          if (overflowing) panel.dataset.interruptionUi = 'legacy';
        });
      }
      if (currentPermissionTrustIndex >= 0) renderPermissionTrustPage();
      else showPermissionMainPage();
      panel.classList.add('visible');
      emitInterruptionState('open', 'permission');
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
  // The trust flow has no covered design; its runtime entry is suspended.
  return;
}

function renderPermissionTrustPage() {
  if (document.getElementById('permPanelTrust').dataset.uiSuspended) return;
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
  if (document.getElementById('permPanelTrust').dataset.uiSuspended) return;
  if (trustKey) currentPermissionTrustKeys.push(trustKey);
  currentPermissionTrustIndex += 1;
  renderPermissionTrustPage();
}

function activePermissionActions() {
  const id = currentPermissionTrustIndex >= 0 ? 'permTrustActions' : 'permPanelActions';
  const container = document.getElementById(id);
  return container ? Array.from(container.querySelectorAll('.perm-panel-opt:not(:disabled)')) : [];
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
    document.getElementById('permPanel').dataset.interruptionUi = 'legacy';
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
  emitInterruptionState('close', 'permission');
}

// =============== Send Message & Slash Command Dispatch ===============

