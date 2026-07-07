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
// +-- Slash Command Autocomplete (updateAutocomplete, selectSlashCmd, navigateAutocomplete)
// +-- Keyboard Shortcuts (global keydown handler)
// +-- UI Helpers (toggleToolCall, toggleThinking, copyCode, autoResize, escapeHtml)
// ===================================================================

let invoke, listen;
let sessions = [];
let models = [];
let currentPermissionId = null;
let currentPermissionTrustKey = null;
// 权限请求关联到 session（path → permissionEvent）
let pendingPermissions = {};
let toolCounts = {};
let currentSettings = null;
let isStreaming = false;
let acSelectedIndex = -1;
let acRequestSeq = 0;
let acPrefix = '';
let acItems = [];
let inputHighlightRanges = [];
let activeSessionIdx = 0;
// 跟踪每个 session 的 streaming 状态（path → bool）
let sessionStreamingState = {};

// =============== Slash Commands Registry ===============

const slashCommands = [
  // Session Management
  { cmd: '/new', desc: '新建会话', category: 'session' },
  { cmd: '/clear', desc: '清空当前会话', category: 'session' },
  { cmd: '/name', desc: '设置会话名称', category: 'session' },
  { cmd: '/session', desc: '显示会话信息', category: 'session' },
  { cmd: '/resume', desc: '恢复会话（会话列表）', category: 'session' },
  { cmd: '/clone', desc: '克隆当前会话', category: 'session' },
  { cmd: '/fork', desc: '从某消息分叉会话', category: 'session' },
  { cmd: '/tree', desc: '查看会话条目树', category: 'session' },
  { cmd: '/graph', desc: '可视化会话时间线', category: 'session' },
  { cmd: '/gc', desc: '清理过期会话文件', category: 'session' },

  // Model & Settings
  { cmd: '/model', desc: '切换模型（打开选择器）', category: 'model' },
  { cmd: '/scoped-models', desc: '列出所有可用模型', category: 'model' },
  { cmd: '/thinking', desc: '设置思考等级 (off/low/medium/high)', category: 'model' },
  { cmd: '/settings', desc: '打开设置面板', category: 'settings' },
  { cmd: '/lsp', desc: '配置 LSP 诊断模式', category: 'settings' },

  // Context Management
  { cmd: '/compact', desc: '压缩会话上下文', category: 'context' },
  { cmd: '/permissions', desc: '显示权限模式和决策统计', category: 'context' },
  { cmd: '/subagents', desc: '列出子代理', category: 'context' },
  { cmd: '/main', desc: '切换回主代理视图', category: 'context' },

  // Data Operations
  { cmd: '/export', desc: '导出会话 (html/md/jsonl)', category: 'data' },
  { cmd: '/import', desc: '导入 JSONL 会话文件', category: 'data' },
  { cmd: '/share', desc: '分享会话 (gh gist)', category: 'data' },
  { cmd: '/copy', desc: '复制最后一条助手消息', category: 'data' },
  { cmd: '/search', desc: '搜索会话内容', category: 'data' },

  // Authentication
  { cmd: '/login', desc: 'OAuth 登录', category: 'auth' },
  { cmd: '/logout', desc: '退出登录', category: 'auth' },
  { cmd: '/usage', desc: '查询速率限制', category: 'auth' },

  // Help & Utilities
  { cmd: '/help', desc: '显示帮助信息', category: 'help' },
  { cmd: '/hotkeys', desc: '显示快捷键', category: 'help' },
  { cmd: '/changelog', desc: '显示变更日志', category: 'help' },
  { cmd: '/reload', desc: '重新加载配置', category: 'help' },
  { cmd: '/quit', desc: '退出应用', category: 'help' },
];

// Commands intercepted locally (not sent to backend as chat)
const LOCAL_COMMANDS = new Set([
  'model', 'settings', 'thinking', 'clear', 'new', 'help', 'hotkeys', 'quit',
]);

// =============== Initialization ===============

window.addEventListener('DOMContentLoaded', async () => {
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

  await listen('ui-state', ev => renderState(ev.payload));
  await listen('tool-event', ev => handleToolEvent(ev.payload));
  await listen('permission-request', ev => showPermission(ev.payload));
  await listen('error', ev => showError(typeof ev.payload === 'string' ? ev.payload : JSON.stringify(ev.payload)));
  await listen('notification', ev => showNotification(typeof ev.payload === 'string' ? ev.payload : JSON.stringify(ev.payload)));

  try { const s = await invoke('get_state'); renderState(s); } catch (e) { showError('get_state failed: ' + String(e)); }
  try { sessions = await invoke('get_sessions'); renderSessionList(); } catch (e) { showSidebarError('sessionList', 'get_sessions failed: ' + String(e)); }
  try { models = await invoke('list_models'); renderModelSelector(); } catch (e) { showError('list_models failed: ' + String(e)); }
  refreshRateLimits(false);
});

// =============== State Rendering ===============

function renderState(snap) {
  if (!snap) return;
  isStreaming = !!snap.isStreaming;
  // 记录当前活跃 session 的 streaming 状态
  if (sessions[activeSessionIdx]) {
    sessionStreamingState[sessions[activeSessionIdx].path] = isStreaming;
  }
  updateHeader(snap);
  updateSidebar(snap);
  renderMessages(snap.messages, snap.isStreaming);
  updateAbortButton();
  renderSessionList();
}

function updateHeader(snap) {
  const nameEl = document.getElementById('currentSessionName');
  if (nameEl && snap.sessionName) nameEl.textContent = snap.sessionName;

  const modelBtn = document.getElementById('modelSelector');
  if (modelBtn && snap.model) modelBtn.textContent = snap.model.id;

  const badge = document.querySelector('[data-od-id="perm-badge"]');
  if (badge) {
    if (snap.isStreaming) {
      badge.textContent = 'streaming...';
      badge.className = 'permission-badge perm-on';
    } else {
      badge.textContent = snap.thinkingLevel || 'ready';
      badge.className = 'permission-badge perm-auto';
    }
  }
}

function updateQuotaBars(snapshot) {
  const hourBar = document.getElementById('quotaHourBar');
  const hourVal = document.getElementById('quotaHour');
  const weekBar = document.getElementById('quotaWeekBar');
  const weekVal = document.getElementById('quotaWeek');
  updateQuotaWindow(hourBar, hourVal, snapshot && snapshot.primary, '5 小时');
  updateQuotaWindow(weekBar, weekVal, snapshot && snapshot.secondary, '本周');
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
  try {
    const snapshot = await invoke('get_rate_limits');
    updateQuotaBars(snapshot);
    if (showResult) showNotification(formatRateLimitSnapshot(snapshot));
  } catch (e) {
    updateQuotaBars(null);
    if (showResult) showError('Rate limit query failed: ' + String(e));
  }
}

function updateSidebar(snap) {
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

function updateAbortButton() {
  const sendBtn = document.querySelector('[data-od-id="send-btn"]');
  if (!sendBtn) return;
  if (isStreaming) {
    sendBtn.textContent = '停止';
    sendBtn.onclick = abortAgent;
  } else {
    sendBtn.textContent = '发送';
    sendBtn.onclick = sendMessage;
  }
}

// =============== Message Rendering ===============

function renderMessages(messages, streaming) {
  const container = document.getElementById('chatMessages');
  if (!container) return;

  if (!messages || messages.length === 0) {
    container.innerHTML = '<div class="chat-empty"><div class="chat-empty-icon">R</div>' +
      '<div class="chat-empty-title">开始新对话</div>' +
      '<div class="chat-empty-hint">向 Rozsa 描述你的编码任务' +
      '<div class="chat-empty-kbd"><kbd>Enter</kbd> 发送 <kbd>Shift+Enter</kbd> 换行</div></div></div>';
    return;
  }

  container.innerHTML = '';
  toolCounts = {};

  // 预建 toolResult 索引: toolCallId → { output, isError, toolName }
  // 每个 toolResult 通过 toolCallId 严格对应一个 toolCall
  const toolResultMap = {};
  for (const raw of messages) {
    if (raw.kind === 'standard' && raw.message && raw.message.role === 'toolResult') {
      const m = raw.message;
      const id = m.toolCallId;
      if (id) {
        const text = (m.content || []).filter(b => b.type === 'text').map(b => b.text).join('\n');
        toolResultMap[id] = { output: text, isError: !!m.isError, toolName: m.toolName || '' };
      }
    }
  }

  const visibleMessages = messages.filter(raw =>
    !(raw.kind === 'standard' && raw.message && raw.message.role === 'toolResult')
  );

  const activeStreamIndex = activeStreamMessageIndex(visibleMessages, streaming);

  for (let i = 0; i < visibleMessages.length; i++) {
    const raw = visibleMessages[i];
    container.appendChild(renderMessage(raw, toolResultMap, i === activeStreamIndex));
  }

  if (activeStreamIndex >= 0) {
    const active = container.children[activeStreamIndex];
    if (active) attachStreamCursor(active);
  }

  renderToolChips();
  container.scrollTop = container.scrollHeight;
}

function renderMessage(raw, toolResultMap, isActiveStream = false) {
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
      '<div class="msg-role">你</div>' +
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
      const thinkingDuration = thinkingActive ? '' : formatThinkingDuration(Date.now() - messageTimestampMs(msg));
      body += '<div class="thinking-block' + (thinkingActive ? ' active' : '') + '"><div class="thinking-header" onclick="toggleThinking(this)">' +
        '<svg class="thinking-icon" width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M8 1.5C5 1.5 3 3.5 3 6c0 1.5.8 2.7 2 3.5V12a1 1 0 001 1h4a1 1 0 001-1V9.5c1.2-.8 2-2 2-3.5 0-2.5-2-4.5-5-4.5z"/><path d="M6 14.5h4"/></svg>' +
        '<span class="thinking-label">' + thinkingLabel + '</span>' +
        (thinkingDuration ? '<span class="thinking-duration">' + thinkingDuration + '</span>' : '') +
        '<span class="thinking-chevron">▸</span></div>' +
        '<div class="thinking-content"' + (thinkingActive ? ' data-stream-cursor-target="thinking"' : '') + '>' +
        renderMarkdown(thinking) + '</div></div>';
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

      body += '<div class="tool-call" onclick="toggleToolCall(this)">' +
        '<div class="tool-track"><div class="tool-icon">' + toolIcon(tc.name) + '</div>' +
        '</div>' +
        '<div class="tool-content"><div class="tool-header">' +
        '<span class="tool-call-status ' + tcStatus + '"></span>' +
        '<span class="tool-name">' + escapeHtml(toolTitle.name) + '</span>' +
        '<span class="tool-call-args">' + escapeHtml(toolTitle.arg) + '</span>' +
        '<span class="tool-call-toggle">▸</span></div></div>' +
        '<div class="tool-call-body"><pre style="white-space:pre-wrap;margin:0;font-size:11.5px">' +
        escapeHtml(bodyOutput) + '</pre></div></div>';
    }

    const text = extractText(content);
    if (text) {
      const textActive = isActiveStream && latestType === 'text';
      body += '<div class="msg-content markdown-body"' + (textActive ? ' data-stream-cursor-target="text"' : '') +
        '>' + renderMarkdown(text) + '</div>';
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

function activeStreamMessageIndex(messages, streaming) {
  if (!streaming) return -1;
  for (let i = messages.length - 1; i >= 0; i--) {
    const raw = messages[i];
    if (raw.kind === 'standard' && raw.message && raw.message.role === 'assistant') return i;
  }
  return -1;
}

function attachStreamCursor(messageEl) {
  if (messageEl.querySelector('.stream-cursor')) return;
  const markedTarget = messageEl.querySelector('[data-stream-cursor-target]');
  const targets = messageEl.querySelectorAll('.msg-content.markdown-body, .thinking-content, .msg-content');
  const target = markedTarget || targets[targets.length - 1];
  if (!target) return;
  const cursor = document.createElement('span');
  cursor.className = 'stream-cursor';
  cursor.textContent = '▌';
  appendCursorAfterLastText(target, cursor);
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

function messageTimestampMs(msg) {
  const ts = Number(msg.timestamp || 0);
  if (!Number.isFinite(ts) || ts <= 0) return Date.now();
  return ts < 100000000000 ? ts * 1000 : ts;
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
  // 权限请求关联到正在跑 agent 的 session（用 activeSessionIdx 当时的 path）
  // 因为权限请求只会在 agent 运行中产生，此时 activeSessionIdx 可能已经切走了
  // 存储请求，只有切回对应 session 时才显示面板
  const permSessionPath = sessions[activeSessionIdx] ? sessions[activeSessionIdx].path : '__current__';
  pendingPermissions[permSessionPath] = ev;
  // 如果当前就在看这个 session，立即显示
  displayPermPanelIfNeeded();
}

function displayPermPanelIfNeeded() {
  const currentPath = sessions[activeSessionIdx] ? sessions[activeSessionIdx].path : '__current__';
  const ev = pendingPermissions[currentPath];
  if (!ev) {
    hidePermPanel();
    return;
  }
  currentPermissionId = ev.id;
  currentPermissionTrustKey = ev.trust_key || ev.trustKey || null;
  const panel = document.getElementById('permPanel');
  if (!panel) return;
  const risk = document.getElementById('permRisk');
  const tool = document.getElementById('permTool');
  const cmd = document.getElementById('permCmd');
  const desc = document.getElementById('permDesc');
  if (risk) risk.textContent = ev.risk || 'Shell';
  if (tool) tool.textContent = ev.tool || '—';
  if (cmd) cmd.textContent = ev.command || ev.summary || '—';
  if (desc) desc.textContent = ev.description || ev.summary || '—';
  panel.classList.add('visible');
  document.getElementById('msgInput').style.display = 'none';
}

async function respondPermission(choice) {
  if (!currentPermissionId) return;
  try {
    await invoke('respond_permission', {
      id: currentPermissionId,
      choice: choice,
      trustKey: choice === 'allow-session' ? currentPermissionTrustKey : null,
    });
  } catch (e) { console.error('respond_permission:', e); }
  // 清除该 session 的 pending permission
  const currentPath = sessions[activeSessionIdx] ? sessions[activeSessionIdx].path : '__current__';
  delete pendingPermissions[currentPath];
  hidePermPanel();
}

function hidePermPanel() {
  const panel = document.getElementById('permPanel');
  if (panel) panel.classList.remove('visible');
  const input = document.getElementById('msgInput');
  if (input) { input.style.display = ''; input.focus(); }
  currentPermissionId = null;
  currentPermissionTrustKey = null;
}

// =============== Send Message & Slash Command Dispatch ===============

async function sendMessage() {
  const input = document.getElementById('msgInput');
  if (!input) return;
  const text = input.value.trim();
  if (!text) return;
  input.value = '';
  input.style.height = 'auto';
  hideAutocomplete();

  // Check if this is a slash command
  if (text.startsWith('/')) {
    const handled = await dispatchSlashCommand(text);
    if (handled) return;
  }

  try {
    await invoke('send_message', { message: text });
    // 发消息后刷新 session 列表（新会话首条消息会创建 .jsonl）
    sessions = await invoke('get_sessions');
    renderSessionList();
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
      sessions = await invoke('get_sessions');
      renderSessionList();
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
    default:
      return;
  }
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
  if (!sessions.length) {
    el.innerHTML = '<div style="padding:12px;font-size:11px;color:var(--muted)">No sessions</div>';
    return;
  }
  el.innerHTML = sessions.map((s, i) =>
    '<div class="session-item' + (i === activeSessionIdx ? ' active' : '') + '" data-path="' + escapeHtml(s.path) +
    '" onclick="doSwitchSession(' + i + ')">' +
    '<span class="session-status ' + (sessionStreamingState[s.path] ? 'running' : 'idle') + '"></span>' +
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

function toggleSettings() {
  const panel = document.getElementById('settingsPanel');
  if (!panel) return;
  if (panel.classList.contains('visible')) {
    closeSettings();
  } else {
    panel.classList.add('visible');
    loadSettings();
  }
}

function closeSettings() {
  const panel = document.getElementById('settingsPanel');
  if (panel) panel.classList.remove('visible');
}

function switchSettingsTab(tabId, btn) {
  document.querySelectorAll('.settings-tab').forEach(t => t.classList.remove('active'));
  document.querySelectorAll('.settings-pane').forEach(p => p.classList.remove('active'));
  if (btn) btn.classList.add('active');
  const pane = document.getElementById('pane-' + tabId);
  if (pane) pane.classList.add('active');
}

async function loadSettings() {
  try {
    currentSettings = await invoke('get_settings');
  } catch (e) {
    console.warn('get_settings:', e);
    currentSettings = {};
  }
  renderSettingsPane(currentSettings);
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
  renderGeneralSettings(settings);
}

function renderGeneralSettings(settings) {
  // Thinking level
  const thinkingSel = document.getElementById('settingsThinking');
  if (thinkingSel) {
    if (settings.thinking_level) thinkingSel.value = settings.thinking_level.toLowerCase();
    thinkingSel.onchange = () => saveSetting('thinking', thinkingSel.value);
  }

  // Auto compact
  const compactSel = document.getElementById('settingsAutoCompact');
  if (compactSel) {
    if (settings.auto_compact !== undefined) compactSel.value = String(settings.auto_compact);
    compactSel.onchange = () => saveSetting('auto_compact', compactSel.value);
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

  // Block images
  const blockSel = document.getElementById('settingsBlockImages');
  if (blockSel) {
    if (settings.block_images !== undefined) blockSel.value = String(settings.block_images);
    blockSel.onchange = () => saveSetting('block_images', blockSel.value);
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
    // Refresh settings state
    currentSettings = await invoke('get_settings');
    renderSettingsPane(currentSettings);
  } catch (e) {
    console.warn('update_setting failed:', e);
    showError('Failed to save setting: ' + key);
  }
}

// =============== Input Handling ===============

function toggleAttachmentMenu() {
  const menu = document.getElementById('attachmentMenu');
  if (!menu) return;
  menu.classList.toggle('visible');
}

async function attachFileReference(mode = 'file') {
  const menu = document.getElementById('attachmentMenu');
  if (menu) menu.classList.remove('visible');
  try {
    const path = await invoke('pick_attachment', { mode });
    if (!path) return;
    insertInputText(formatFileReference(path));
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
  const start = input.selectionStart || input.value.length;
  const end = input.selectionEnd || start;
  input.value = input.value.slice(0, start) + text + input.value.slice(end);
  const cursor = start + text.length;
  input.setSelectionRange(cursor, cursor);
  input.focus();
  autoResize(input);
  updateAutocomplete();
}

function formatFileReference(path) {
  if (path.includes('"')) return '@' + path + ' ';
  if (/\s/.test(path)) return '@"' + path + '" ';
  return '@' + path + ' ';
}

function autoResize(el) {
  el.style.height = 'auto';
  el.style.height = Math.min(el.scrollHeight, 120) + 'px';
  updateInputHighlight(inputHighlightRanges);
}

// =============== Slash Command Autocomplete ===============

let acVisible = false;

async function updateAutocomplete() {
  const input = document.getElementById('msgInput');
  const popup = document.getElementById('autocomplete');
  if (!input || !popup) return;
  const val = input.value;
  const cursor = input.selectionStart || val.length;
  const seq = ++acRequestSeq;
  let result = null;
  try {
    result = await invoke('autocomplete_input', { text: val, cursor });
  } catch (e) {
    hideAutocomplete();
    return;
  }
  if (seq !== acRequestSeq) return;
  setInputMatchState(!!result.validMatch);
  updateInputHighlight(result.highlightRanges || []);
  if (!result.items || result.items.length === 0 || !result.prefix) {
    hideAutocomplete(!result.validMatch);
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
  const cursor = input.selectionStart || input.value.length;
  const start = Math.max(0, cursor - acPrefix.length);
  input.value = input.value.slice(0, start) + item.value + input.value.slice(cursor);
  const nextCursor = start + item.value.length;
  input.setSelectionRange(nextCursor, nextCursor);
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
    setInputMatchState(false);
    updateInputHighlight([]);
  }
}

function setInputMatchState(valid) {
  const wrapper = document.querySelector('.input-wrapper');
  if (wrapper) wrapper.classList.toggle('valid-token', valid);
}

function updateInputHighlight(ranges) {
  inputHighlightRanges = Array.isArray(ranges) ? ranges : [];
  const input = document.getElementById('msgInput');
  const layer = document.getElementById('inputHighlight');
  if (!input || !layer) return;
  const text = input.value;
  if (inputHighlightRanges.length === 0) {
    layer.textContent = text;
    syncInputHighlightScroll();
    return;
  }
  const chars = Array.from(text);
  let html = '';
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
    html += escapeHtml(chars.slice(cursor, range.start).join(''));
    html += '<span class="valid-token-text">' +
      escapeHtml(chars.slice(range.start, range.end).join('')) +
      '</span>';
    cursor = range.end;
  }
  html += escapeHtml(chars.slice(cursor).join(''));
  layer.innerHTML = html;
  syncInputHighlightScroll();
}

function syncInputHighlightScroll() {
  const input = document.getElementById('msgInput');
  const layer = document.getElementById('inputHighlight');
  if (!input || !layer) return;
  layer.scrollTop = input.scrollTop;
}

// =============== Keyboard Shortcuts ===============

document.addEventListener('keydown', function(e) {
  const input = document.getElementById('msgInput');

  // Permission panel shortcuts
  if (currentPermissionId) {
    if (e.key === 'y' || e.key === 'Y') { e.preventDefault(); respondPermission('allow'); return; }
    if (e.key === 't' || e.key === 'T') { e.preventDefault(); respondPermission('allow-session'); return; }
    if (e.key === 'n' || e.key === 'N') { e.preventDefault(); respondPermission('deny'); return; }
    if (e.key === 'a' || e.key === 'A') { e.preventDefault(); respondPermission('deny'); return; }
    if (e.key === 'Escape') { e.preventDefault(); respondPermission('deny'); return; }
  }

  // Escape handling
  if (e.key === 'Escape') {
    if (acVisible) { hideAutocomplete(); return; }
    if (document.getElementById('settingsPanel').classList.contains('visible')) {
      closeSettings(); return;
    }
    if (isStreaming) { abortAgent(); return; }
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

document.addEventListener('click', function(e) {
  const menu = document.getElementById('attachmentMenu');
  if (!menu || !menu.classList.contains('visible')) return;
  if (e.target.closest('[data-attachment-control]')) return;
  menu.classList.remove('visible');
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

function toggleToolCall(el) { el.classList.toggle('expanded'); }

function toggleThinking(header) {
  const block = header.closest('.thinking-block');
  if (block) block.classList.toggle('expanded');
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
    '| Y/T/N/A | Permission response |\n' +
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
  return resetText ? label + ' ' + resetText + ' 重置' : label + ' 重置时间未知';
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

// =============== GUI-only Settings (localStorage) ===============

function applyTheme(value) {
  localStorage.setItem('rozsa-theme', value);
  document.documentElement.setAttribute('data-theme', value);
}

function applyFontSize(value) {
  localStorage.setItem('rozsa-font-size', value);
  document.documentElement.style.fontSize = value + 'px';
}

// 启动时恢复 GUI-only 设置
(function restoreLocalSettings() {
  const theme = localStorage.getItem('rozsa-theme');
  if (theme) {
    document.documentElement.setAttribute('data-theme', theme);
    const sel = document.getElementById('settingsTheme');
    if (sel) sel.value = theme;
  }
  const fontSize = localStorage.getItem('rozsa-font-size');
  if (fontSize) {
    document.documentElement.style.fontSize = fontSize + 'px';
    const sel = document.getElementById('settingsFontSize');
    if (sel) sel.value = fontSize;
  }
})();
