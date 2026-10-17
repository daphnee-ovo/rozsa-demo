// @ts-nocheck
function renderSessionList() {
  const el = document.getElementById('sessionList');
  if (!el) return;
  const renderKey = JSON.stringify(sessions) + ':' + activeSessionIdx + ':' + JSON.stringify(sessionStreamingState);
  if (renderKey === renderedSessionListKey) return;
  renderedSessionListKey = renderKey;
  el.innerHTML = window.RozsaSidebar.sessionsMarkup(sessions.map((session, index) => ({
    name: session.name || session.first_message || 'Untitled',
    date: formatSessionDate(session.modified || session.last_modified),
    activity: sessionStreamingState[session.id] || 'idle', active: index === activeSessionIdx,
  })));
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
  const switchTrace = new URLSearchParams(location.search).get('rozsa_perf') === '1'
    ? { sessionId: s.id, startedAt: performance.now(), preparationCompletedAt: 0, rpcCompletedAt: 0, renderCommittedAt: 0 }
    : null;
  if (switchTrace) window.__ROZSA_SESSION_SWITCH_TRACE__ = switchTrace;
  const chat = document.getElementById('chatMessages');
  if (chat && renderedMessageSessionId) {
    persistSessionViewState(renderedMessageSessionId, chat);
  }
  if (activeSessionId) captureSessionDraft(activeSessionId);
  if (activeSessionId) capturePermissionUiState(activeSessionId);
  activeSessionIdx = idx;
  const items = document.querySelectorAll('.session-item');
  items.forEach((item, index) => item.classList.toggle('active', index === idx));
  if (switchTrace) switchTrace.preparationCompletedAt = performance.now();
  try {
    await invoke('switch_session', { path: s.path });
    if (switchTrace) switchTrace.rpcCompletedAt = performance.now();
    // switch_session publishes the selected snapshot through ui-state. Fetching
    // get_state here repeated the full history mapping and React render.
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
  if (conversationOperationSuspended('modelPicker')) return;
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
  if (conversationOperationSuspended('modelPicker')) return;
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
  if (conversationOperationSuspended('modelPicker')) return;
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
  const title = document.getElementById('thinkingEffortPickerTitle');
  if (!popover || !title) return;
  const options = supportedThinkingEfforts();
  const requested = String(effort || currentSettings?.thinkingEffort
    || document.getElementById('thinkingEffort')?.textContent || 'off').toLowerCase();
  const selected = normalizeThinkingEffort(options, requested);
  const selectedIndex = Math.max(0, options.indexOf(selected));
  title.textContent = options.length > 1 ? 'Thinking effort' : 'Thinking unavailable for this model';
  window.RozsaPopover.renderChoice(options, selectedIndex);
  if (!popover.hidden) positionThinkingEffortPopover();
}

function positionThinkingEffortPopover() {
  window.RozsaPopover?.position();
}

function toggleThinkingEffortPicker(event) {
  window.RozsaPopover.toggle(event);
}

function hideThinkingEffortPicker() {
  window.RozsaPopover?.close();
}

function previewThinkingEffort(index) {
  const options = supportedThinkingEfforts();
  const selectedIndex = Math.max(0, Math.min(options.length - 1, Number(index)));
  window.RozsaPopover.renderChoice(options, selectedIndex);
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

