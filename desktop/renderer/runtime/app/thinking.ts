// @ts-nocheck
function renderToolEvidence(toolCall, result, devFlowPresentation) {
  if (devFlowPresentation && result) {
    return { html: renderBashToolEvidence(toolCall, result), className: ' dev-flow-tool-evidence' };
  }
  const toolName = normalizeToolName(toolCall && toolCall.name);
  if (toolName === 'bash') {
    return result
      ? { html: renderBashToolEvidence(toolCall, result), className: ' dev-flow-tool-evidence' }
      : renderGenericToolEvidence(toolCall, result);
  }
  if (toolName === 'read') return renderReadToolEvidence(toolCall, result);
  if (toolName === 'write') return renderWriteEditToolEvidence(toolCall, result, 'write');
  if (toolName === 'edit') return renderWriteEditToolEvidence(toolCall, result, 'edit');
  if (toolName === 'subagent') return renderSubagentToolEvidence(toolCall, result);
  if (toolName === 'askuserquestion' || toolName === 'ask_user_question') {
    return renderQuestionToolEvidence(toolCall, result);
  }
  return renderGenericToolEvidence(toolCall, result);
}

const DEV_FLOW_TITLE_CACHE_LIMIT = 64;
const DEV_FLOW_TITLE_RESPONSE_LIMIT = 2 * 1024 * 1024;
const DEV_FLOW_TITLE_TIMEOUT_MS = 1500;
const devFlowTitleCache = new Map();
const devFlowTitleRequests = new Map();
let devFlowTitleSettingsRequest = null;

function devFlowTitleCacheKey(kind, id) {
  const dashboardUrl = typeof devFlowSettings !== 'undefined'
    ? devFlowSettings?.project?.dashboardUrl || ''
    : '';
  return dashboardUrl + '|' + kind + ':' + id;
}

function devFlowTitleForItem(kind, id) {
  const key = devFlowTitleCacheKey(kind, id);
  return devFlowTitleCache.has(key) ? devFlowTitleCache.get(key) : null;
}

function rememberDevFlowTitle(key, title) {
  devFlowTitleCache.delete(key);
  devFlowTitleCache.set(key, title || null);
  while (devFlowTitleCache.size > DEV_FLOW_TITLE_CACHE_LIMIT) {
    devFlowTitleCache.delete(devFlowTitleCache.keys().next().value);
  }
}

function devFlowTitleEndpoint(kind, id) {
  const rawUrl = typeof devFlowSettings !== 'undefined'
    ? devFlowSettings?.project?.dashboardUrl
    : null;
  if (!rawUrl || !id) return null;
  try {
    const base = new URL(rawUrl);
    const loopback = ['localhost', '127.0.0.1', '::1', '[::1]'].includes(base.hostname);
    if (base.protocol !== 'http:' || !loopback) return null;
    const collection = kind === 'issue' ? 'issues' : 'tasks';
    const path = 'api/v1/' + collection + '/' + encodeURIComponent(id);
    return new URL(path, rawUrl.endsWith('/') ? rawUrl : rawUrl + '/');
  } catch (_) {
    return null;
  }
}

function ensureDevFlowTitleSettings() {
  if (typeof devFlowSettings !== 'undefined' && devFlowSettings !== null) return null;
  if (typeof invoke !== 'function') return null;
  if (!devFlowTitleSettingsRequest) {
    devFlowTitleSettingsRequest = invoke('get_dev_flow_settings')
      .then(snapshot => {
        devFlowSettings = snapshot;
        return snapshot;
      })
      .catch(() => null)
      .finally(() => {
        devFlowTitleSettingsRequest = null;
      });
  }
  return devFlowTitleSettingsRequest;
}

function fetchDevFlowTitle(endpoint) {
  if (typeof fetch !== 'function') return Promise.resolve(null);
  const controller = typeof AbortController === 'function' ? new AbortController() : null;
  const timer = controller ? setTimeout(() => controller.abort(), DEV_FLOW_TITLE_TIMEOUT_MS) : null;
  return fetch(endpoint.toString(), {
    method: 'GET',
    signal: controller ? controller.signal : undefined,
  }).then(response => {
    if (!response.ok) throw new Error('Dev Flow title lookup failed: ' + response.status);
    return response.text();
  }).then(text => {
    if (text.length > DEV_FLOW_TITLE_RESPONSE_LIMIT) throw new Error('Dev Flow title response is too large');
    const payload = JSON.parse(text);
    return typeof payload?.title === 'string' ? payload.title : null;
  }).catch(() => null).finally(() => {
    if (timer) clearTimeout(timer);
  });
}

function requestDevFlowTitles(toolCallId, presentation) {
  if (presentation.action === 'removed') return;
  const missing = presentation.items.filter(item => !item.title);
  for (const item of missing) {
    const cacheKey = devFlowTitleCacheKey(item.kind, item.id);
    if (devFlowTitleCache.has(cacheKey)) {
      item.title = devFlowTitleCache.get(cacheKey);
      continue;
    }
    const endpoint = devFlowTitleEndpoint(item.kind, item.id);
    if (!endpoint) {
      const settingsRequest = ensureDevFlowTitleSettings();
      if (settingsRequest) {
        settingsRequest.then(() => {
          if (devFlowTitleEndpoint(item.kind, item.id)) requestDevFlowTitles(toolCallId, presentation);
        });
      }
      continue;
    }
    const requestKey = endpoint.toString();
    let request = devFlowTitleRequests.get(requestKey);
    if (!request) {
      request = fetchDevFlowTitle(endpoint);
      devFlowTitleRequests.set(requestKey, request);
      request.finally(() => devFlowTitleRequests.delete(requestKey));
    }
    request.then(title => {
      rememberDevFlowTitle(cacheKey, title);
      item.title = title;
      updateDevFlowToolCard(toolCallId, presentation);
    });
  }
}

function updateDevFlowToolCard(toolCallId, presentation) {
  const card = [...document.querySelectorAll('.dev-flow-tool-call')]
    .find(element => element.dataset.toolCallId === toolCallId);
  if (!card) return;
  const title = formatBashDevFlowTitle(presentation);
  const name = card.querySelector('.tool-name');
  const arg = card.querySelector('.tool-call-args');
  if (name) name.textContent = title.name;
  if (arg) arg.textContent = title.arg;
}

function normalizeToolName(name) {
  return typeof name === 'string' ? name.trim().toLowerCase() : '';
}

function toolDisplayLabel(name) {
  switch (normalizeToolName(name)) {
    case 'bash': return 'Bash';
    case 'read': return 'Read';
    case 'write': return 'Write';
    case 'edit': return 'Edit';
    case 'subagent': return 'Subagent';
    case 'askuserquestion':
    case 'ask_user_question': return 'Question';
    default: return typeof name === 'string' && name.trim() ? name : 'Tool';
  }
}

function compactToolText(value, maxLength = 120) {
  const text = String(value ?? '').replace(/\s+/g, ' ').trim();
  if (text.length <= maxLength) return text;
  return text.slice(0, Math.max(1, maxLength - 1)) + '…';
}

function formatReadRange(args) {
  const offset = Number.isInteger(args.offset) && args.offset > 0 ? args.offset : 1;
  const limit = Number.isInteger(args.limit) && args.limit > 0 ? args.limit : null;
  if (limit) return 'lines ' + offset + '–' + (offset + limit - 1);
  if (args.offset) return 'from line ' + offset;
  return '';
}

function formatPathSummary(args, fallback = 'current directory') {
  const path = args.file_path || args.path;
  return path || fallback;
}

function formatSubagentSummary(args) {
  const action = normalizeToolName(args.action);
  const actionLabels = {
    spawn: 'Spawn',
    send: 'Send',
    wait: 'Wait',
    interrupt: 'Interrupt',
    list: 'List',
  };
  const target = args.name || args.id || (action === 'list' ? 'subagents' : '');
  return {
    name: actionLabels[action] || 'Subagent',
    arg: target ? compactToolText(target) : '',
  };
}

function formatQuestionSummary(args) {
  const questions = Array.isArray(args.questions) ? args.questions : [];
  if (!questions.length) return { name: 'Question', arg: '' };
  if (questions.length > 1) return { name: 'Question', arg: questions.length + ' questions' };
  const question = questions[0] || {};
  return { name: 'Question', arg: compactToolText(question.question || question.header || '') };
}

function formatToolArgs(tc) {
  if (!tc.arguments) return '';
  if (typeof tc.arguments === 'string') return tc.arguments;
  const args = tc.arguments;
  const toolName = normalizeToolName(tc.name);
  if (toolName === 'bash') return args.command || '';
  if (toolName === 'read') {
    return [formatPathSummary(args, ''), formatReadRange(args)].filter(Boolean).join(' · ');
  }
  if (toolName === 'write') return formatPathSummary(args, '');
  if (toolName === 'edit') {
    return [formatPathSummary(args, ''), args.replace_all ? 'replace all' : ''].filter(Boolean).join(' · ');
  }
  if (toolName === 'subagent') {
    const summary = formatSubagentSummary(args);
    return [summary.name, summary.arg].filter(Boolean).join(' · ');
  }
  if (toolName === 'askuserquestion' || toolName === 'ask_user_question') {
    return formatQuestionSummary(args).arg;
  }
  return JSON.stringify(args);
}

function formatToolTitle(tc) {
  const rawName = tc.name || 'Tool';
  const toolName = normalizeToolName(rawName);
  const args = tc.arguments && typeof tc.arguments === 'object' ? tc.arguments : {};
  if (typeof tc.arguments === 'string') {
    return { name: toolDisplayLabel(rawName), arg: compactToolText(tc.arguments) };
  }
  if (toolName === 'bash') {
    const description = compactToolText(args.description);
    return { name: 'Bash', arg: description || compactToolText(args.command || '') };
  }
  if (toolName === 'subagent') return formatSubagentSummary(args);
  if (toolName === 'askuserquestion' || toolName === 'ask_user_question') {
    return formatQuestionSummary(args);
  }
  return { name: toolDisplayLabel(rawName), arg: formatToolArgs(tc) };
}

function resolveToolTitle(tc, devFlowPresentation) {
  return devFlowPresentation
    ? formatBashDevFlowTitle(devFlowPresentation)
    : formatToolTitle(tc);
}

function renderCodeView(content) {
  return window.RozsaTools.codeViewMarkup(content);
}

function renderDiffView(patch) {
  return window.RozsaTools.diffViewMarkup(patch);
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
  const messageEl = container.querySelector(`[data-od-id="message-${index}"]`);
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

function updateThinkingTimings(messages, visibleMessages, activeStreamIndex, sessionId, rawIndexByMessage = new Map(messages.map((message, index) => [message, index]))) {
  const durationsByIndex = new Map();
  visibleMessages.forEach((raw, index) => {
    const msg = raw && raw.message;
    if (!msg || msg.role !== 'assistant' || !extractThinking(msg.content || [])) return;
    const rawIndex = rawIndexByMessage.get(raw);
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

