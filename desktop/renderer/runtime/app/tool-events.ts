// @ts-nocheck
function handleToolEvent(ev) {
  if (ev.type === 'Start') trackTool(ev.name);
}

function renderTurnActivityCard(activity) {
  const view = window.RozsaTools.activityView(activity);
  if (view.disposition === 'covered') return window.RozsaTools.changesMarkup(view.files, view.verification);
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
      '<button class="change-name" aria-expanded="false" ' + (change ? 'data-turn-diff="' + payload + '"' : '') + '>' +
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
  return '<div data-tool-ui="legacy" data-ui-migration-blocked="UI-EVIDENCE-001"><div class="changes-card"><div class="changes-header"><span>Changes: ' + files.length + ' file' + (files.length !== 1 ? 's' : '') + '</span></div>' +
    (rows ? '<div class="changes-list">' + rows + '</div>' : '') + verificationSummary + limitation + '</div></div>';
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
    queue.innerHTML = window.RozsaConversation.runningPanelMarkup(queued.length
      ? { title: 'Queue', count: queued.length, items: queued }
      : null);
    renderedQueueKey = queueKey;
  }

  if (steering && steeringKey !== renderedSteeringKey) {
    steering.hidden = steeringMessages.length === 0;
    steering.innerHTML = window.RozsaConversation.runningPanelMarkup(steeringMessages.length
      ? { title: 'Steering conversation', items: steeringMessages.map(message => (message.text || '') + ' · Awaiting tool result') }
      : null);
    renderedSteeringKey = steeringKey;
  }
}

function trackTool(name) {
  if (!name) return;
  const label = toolDisplayLabel(name);
  toolCounts[label] = (toolCounts[label] || 0) + 1;
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
  const toolName = normalizeToolName(name);
  if (toolName === 'bash') return '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="4 5.5 1.5 8 4 10.5"/><line x1="8" y1="10" x2="13" y2="10"/></svg>';
  if (toolName === 'edit') return '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M12.5 2.5a1.4 1.4 0 012 2L5 14l-3 1 1-3 9.5-9.5z"/><path d="M11 4l2 2"/></svg>';
  if (toolName === 'write') return '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M12.5 2.5a1.4 1.4 0 012 2L5 14l-3 1 1-3 9.5-9.5z"/><path d="M11 4l2 2"/></svg>';
  if (toolName === 'read') return '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3 2.5h6l4 4v7a1 1 0 01-1 1H3a1 1 0 01-1-1V3.5a1 1 0 011-1z"/><path d="M9 2.5v4h4"/><path d="M5 8.5h6M5 11h4"/></svg>';
  // Generic file icon for other tools
  return '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3 2.5h6l4 4v7a1 1 0 01-1 1H3a1 1 0 01-1-1V3.5a1 1 0 011-1z"/><path d="M9 2.5v4h4"/></svg>';
}

// =============== Agent Questions ===============

