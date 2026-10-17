// @ts-nocheck
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
  window.RozsaDevFlow.renderDetail(window.RozsaDevFlow.devFlowDetailView(payload));
}
function openDevFlowDetailPanel() {
  const overlay = document.getElementById('devFlowDetail');
  if (!overlay) return;
  overlay.hidden = false;
  document.body.classList.add('dev-flow-detail-open');
  devFlowDetailOpen = true;
  const focusTarget = document.getElementById('devFlowDetailClose');
  if (focusTarget) focusTarget.focus();
}

function closeDevFlowDetail() {
  const overlay = document.getElementById('devFlowDetail');
  if (!overlay) return;
  overlay.hidden = true;
  document.body.classList.remove('dev-flow-detail-open');
  devFlowDetailOpen = false;
}


const notificationToasts = new Map();
const unresolvedErrors = new Map();
let legacyNotificationCounter = 0;
let notificationErrorTrayController;

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
  return window.RozsaNotifications.notificationIcon(severity);
}

function applyNotificationPresentation(toast, severity, title, message) {
  window.RozsaNotifications.applyNotificationPresentation(toast, severity, title, message);
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
  const element = window.RozsaNotifications.createNotificationElement({ id, severity, title, message });
  window.RozsaNotifications.bindToast(element, id, { dismissToast, pauseToastTimer, resumeToastTimer });
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
  notificationErrorTrayController?.resetPin();
}

function renderNotificationErrorList() {
  const list = notificationErrorList();
  list.innerHTML = [...unresolvedErrors.values()].map(window.RozsaNotifications.errorItemMarkup).join('');
}

function updateNotificationErrorTray() {
  const count = unresolvedErrors.size;
  notificationErrorCount().textContent = String(count);
  notificationErrorTray().hidden = count === 0;
  if (count === 0) closeNotificationErrorList();
  if (isNotificationErrorListOpen()) renderNotificationErrorList();
}

function setupNotificationErrorTray() {
  notificationErrorTrayController = window.RozsaNotifications.bindErrorTray({
    byId: id => document.getElementById(id), hasErrors: () => unresolvedErrors.size > 0,
    isNotificationErrorListOpen, openNotificationErrorList, closeNotificationErrorList,
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

