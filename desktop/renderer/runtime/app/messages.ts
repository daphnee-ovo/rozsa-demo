// @ts-nocheck
let conversationEntryCache = new Map();
let messageToolResultCache = new Map();

function countTools(messages) {
  const counts = {};
  for (const raw of messages) {
    const content = raw && raw.message && raw.message.content;
    if (!Array.isArray(content)) continue;
    for (const block of content) {
      if (block.type === 'toolCall' && block.name) {
        const label = toolDisplayLabel(block.name);
        counts[label] = (counts[label] || 0) + 1;
      }
    }
  }
  return counts;
}

function isChatNearBottom(container) {
  const distance = container.scrollHeight - container.scrollTop - container.clientHeight;
  return distance <= 48;
}

function scrollChatToBottom(container) {
  container.scrollTop = container.scrollHeight;
}

function setupChatScrollLock() {
  const container = document.getElementById('chatMessages');
  if (!container) return;
  container.addEventListener('wheel', ev => {
    if (ev.deltaY < 0) {
      chatAutoScrollPaused = true;
      return;
    }
    if (ev.deltaY > 0) {
      requestAnimationFrame(() => {
        if (isChatNearBottom(container)) chatAutoScrollPaused = false;
      });
    }
  }, { passive: true });
  container.addEventListener('scroll', () => {
    if (restoringSessionScroll || !renderedMessageSessionId) return;
    chatAutoScrollPaused = !isChatNearBottom(container);
    if (scrollStateFrame) cancelAnimationFrame(scrollStateFrame);
    scrollStateFrame = requestAnimationFrame(() => {
      scrollStateFrame = 0;
      persistSessionViewState(renderedMessageSessionId, container);
    });
  }, { passive: true });
}

function persistSessionViewState(sessionId, container) {
  if (!sessionId || !container) return;
  sessionViewState[sessionId] = {
    scrollTop: container.scrollTop,
    autoScrollPaused: chatAutoScrollPaused,
  };
  persistConversationState();
}

function restoreSessionViewState(sessionId, container, savedView) {
  const saved = savedView || (sessionId ? sessionViewState[sessionId] : null);
  if (!saved || !Number.isFinite(saved.scrollTop)) {
    chatAutoScrollPaused = false;
    scrollChatToBottom(container);
    return;
  }
  restoringSessionScroll = true;
  requestAnimationFrame(() => {
    const maximum = Math.max(0, container.scrollHeight - container.clientHeight);
    container.scrollTop = Math.min(saved.scrollTop, maximum);
    chatAutoScrollPaused = saved.autoScrollPaused === true || !isChatNearBottom(container);
    restoringSessionScroll = false;
    persistSessionViewState(sessionId, container);
  });
}

function isAssistantMessageRaw(raw) {
  return !!(raw && raw.kind === 'standard' && raw.message && raw.message.role === 'assistant');
}

function assistantMessageGroupPosition(messages, index) {
  if (!isAssistantMessageRaw(messages[index])) return 'standalone';
  return isAssistantMessageRaw(messages[index - 1]) ? 'continuation' : 'start';
}

function buildToolMarkup(toolCall, result, sessionId) {
  trackTool(toolCall.name);
  const view = window.RozsaTools.toolView(toolCall, result);
  if (view.disposition === 'suspended') {
    return '<div data-ui-suspended="' + view.coverage + '" data-tool-call-id="' + escapeHtml(toolCall.id || '') + '"></div>';
  }
  const presentation = parseDevFlowBashPresentation(toolCall, result, devFlowTitleForItem);
  if (presentation && toolCall.id) requestDevFlowTitles(toolCall.id, presentation);
  const title = resolveToolTitle(toolCall, presentation);
  if (view.disposition === 'covered') {
    const evidence = view.kind === 'output'
      ? window.RozsaTools.evidenceMarkup(view.label, view.output)
      : window.RozsaTools.evidenceViewMarkup(view.kind, view.content);
    return window.RozsaTools.toolMarkup(toolCall.id || '', title.name, title.arg, '', view.status, {
      expanded: isToolCallExpanded(toolCall.id), bodyHtml: evidence,
      className: presentation ? 'dev-flow-tool-call' : '',
    }).replace(' tabindex="0"', ' data-session-id="' + escapeHtml(sessionId || '') + '" tabindex="0"');
  }
  const status = result ? (result.isError ? 's-error' : 's-success') : 's-running';
  const evidence = renderToolEvidence(toolCall, result, presentation);
  return '<div data-tool-ui="legacy" data-ui-migration-blocked="' + view.coverage + '" class="tool-call' +
    (isToolCallExpanded(toolCall.id) ? ' expanded' : '') + (presentation ? ' dev-flow-tool-call' : '') +
    '" data-tool-call-id="' + escapeHtml(toolCall.id || '') + '" data-session-id="' + escapeHtml(sessionId || '') + '">' +
    '<div class="tool-track"><div class="tool-icon">' + toolIcon(toolCall.name) + '</div></div>' +
    '<div class="tool-content"><div class="tool-header"><span class="tool-call-status ' + status + '"></span>' +
    '<span class="tool-name">' + escapeHtml(title.name) + '</span><span class="tool-call-args">' + escapeHtml(title.arg) +
    '</span><span class="tool-call-toggle">▸</span></div></div><div class="tool-call-body' + evidence.className + '">' +
    evidence.html + '</div></div>';
}

function createConversationEntry(raw, toolResultMap, isActiveStream, turnActivity, thinkingDurationMs, thinkingExpanded, showIdentity, messageIndex, sessionId) {
  const message = raw && raw.message;
  const role = message && message.role;
  const content = message && Array.isArray(message.content) ? message.content : [];
  const latestType = content.length ? content[content.length - 1].type : '';
  let bodyMarkup = '';
  if (role === 'assistant') {
    for (const call of content.filter(block => block.type === 'toolCall')) {
      const result = call.id ? toolResultMap[call.id] || null : null;
      bodyMarkup += buildToolMarkup(call, result, sessionId);
    }
  }
  const turnActivityMarkup = role === 'assistant' && turnActivity &&
    ((turnActivity.changedFiles && turnActivity.changedFiles.length) || turnActivity.verification)
    ? renderTurnActivityCard(turnActivity) : '';
  const identity = raw && raw.entryId ? raw.entryId : message && message.timestamp ? message.timestamp : messageIndex;
  return {
    id: String(sessionId || '') + ':' + String(identity), kind: raw && raw.kind === 'custom' ? 'custom' : 'standard',
    message: message || raw, messageIndex, continuation: role === 'assistant' && !showIdentity,
    activeStream: isActiveStream, streamTarget: latestType === 'thinking' ? 'thinking' : latestType === 'text' ? 'text' : undefined,
    thinkingExpanded, thinkingDuration: isActiveStream || thinkingDurationMs == null ? '' : formatThinkingDuration(thinkingDurationMs),
    turnActivityMarkup, bodyMarkup,
  };
}

function parseDevFlowBashPresentation(toolCall, result, titleLookup = null) {
  if (!toolCall || !result || typeof toolCall.name !== 'string' ||
      toolCall.name.toLowerCase() !== 'bash') return null;
  if (result.toolName && typeof result.toolName === 'string' &&
      result.toolName.toLowerCase() !== 'bash') return null;
  const details = result.details && typeof result.details === 'object' ? result.details : {};
  if (result.isError || details.success === false || details.exit_code !== 0 || details.truncated === true) {
    return null;
  }
  const command = toolCall.arguments && typeof toolCall.arguments.command === 'string'
    ? toolCall.arguments.command
    : '';
  const parsed = parseDevFlowBashCommand(command);
  if (!parsed) return null;
  const { words, stageCount } = parsed;
  const operation = parseDevFlowAction(words, result.output);
  if (!operation || (stageCount > 1 && operation.action !== 'created')) return null;
  if (!operation.ids.length && !operation.allowEmpty) return null;
  return {
    action: operation.action,
    items: operation.ids.map(({ kind, id }) => ({
      kind: operation.expectedKind || kind,
      id,
      shortId: devFlowShortId(id),
      title: typeof titleLookup === 'function' ? titleLookup(operation.expectedKind || kind, id) : null,
    })),
  };
}

