// @ts-nocheck
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

function renderMessage(raw, toolResultMap, isActiveStream = false, turnActivity = null, thinkingDurationMs = null, thinkingExpanded = false, showAssistantIdentity = true, messageIndex = 0) {
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
    const text = extractText(content);
    return window.RozsaConversation.createMessageFrame('user', window.RozsaTools.messageTextMarkup(text), messageIndex);

  } else if (role === 'assistant') {
    let body = '';
    const errorMessage = msg.errorMessage || '';
    if (errorMessage) {
      body += '<div class="msg-content msg-error"><pre>' + escapeHtml(errorMessage) + '</pre></div>';
    }

    const thinking = extractThinking(content);
    const latestType = content.length ? content[content.length - 1].type : '';
    if (thinking) {
      const thinkingActive = isActiveStream && latestType === 'thinking';
      const thinkingDuration = thinkingActive || thinkingDurationMs === null
        ? ''
        : formatThinkingDuration(thinkingDurationMs);
      body += window.RozsaConversation.thinkingMarkup(renderMarkdown(thinking), {
        expanded: thinkingExpanded, active: thinkingActive, duration: thinkingDuration,
      });
    }

    const toolCalls = content.filter(b => b.type === 'toolCall');
    for (const tc of toolCalls) {
      trackTool(tc.name);
      // 通过 tc.id 查找对应的 toolResult
      const result = toolResultMap && tc.id ? toolResultMap[tc.id] : null;
      const view = window.RozsaTools.toolView(tc, result);
      if (view.disposition === 'suspended') {
        body += '<div data-ui-suspended="' + view.coverage + '" data-tool-call-id="' + escapeHtml(tc.id || '') + '"></div>';
        continue;
      }
      if (view.disposition === 'covered') {
        const devFlowPresentation = parseDevFlowBashPresentation(tc, result, devFlowTitleForItem);
        if (devFlowPresentation && tc.id) requestDevFlowTitles(tc.id, devFlowPresentation);
        const title = resolveToolTitle(tc, devFlowPresentation);
        const evidence = view.kind === 'output'
          ? window.RozsaTools.evidenceMarkup(view.label, view.output)
          : window.RozsaTools.evidenceViewMarkup(view.kind, view.content);
        body += window.RozsaTools.toolMarkup(tc.id || '', title.name, title.arg, '', view.status, {
          expanded: isToolCallExpanded(tc.id), bodyHtml: evidence,
          className: devFlowPresentation ? 'dev-flow-tool-call' : '',
        }).replace(' tabindex="0"', ' data-session-id="' + escapeHtml(activeSessionId || '') + '" tabindex="0"');
        continue;
      }
      const devFlowPresentation = parseDevFlowBashPresentation(tc, result, devFlowTitleForItem);
      if (devFlowPresentation && tc.id) requestDevFlowTitles(tc.id, devFlowPresentation);
      const tcStatus = result ? (result.isError ? 's-error' : 's-success') : 's-running';
      const toolTitle = resolveToolTitle(tc, devFlowPresentation);
      const toolEvidence = renderToolEvidence(tc, result, devFlowPresentation);

      body += '<div data-tool-ui="legacy" data-ui-migration-blocked="' + view.coverage + '" class="tool-call' + (isToolCallExpanded(tc.id) ? ' expanded' : '') +
        (devFlowPresentation ? ' dev-flow-tool-call' : '') +
        '" data-tool-call-id="' + escapeHtml(tc.id || '') + '" data-session-id="' +
        escapeHtml(activeSessionId || '') + '">' +
        '<div class="tool-track"><div class="tool-icon">' + toolIcon(tc.name) + '</div>' +
        '</div>' +
        '<div class="tool-content"><div class="tool-header">' +
        '<span class="tool-call-status ' + tcStatus + '"></span>' +
        '<span class="tool-name">' + escapeHtml(toolTitle.name) + '</span>' +
        '<span class="tool-call-args">' + escapeHtml(toolTitle.arg) + '</span>' +
        '<span class="tool-call-toggle">▸</span></div></div>' +
        '<div class="tool-call-body' + toolEvidence.className + '">' +
        toolEvidence.html +
        '</div></div>';
    }

    const text = extractText(content);
    if (text) {
      const textActive = isActiveStream && latestType === 'text';
      body += '<div class="msg-content markdown-body"' + (textActive ? ' data-stream-cursor-target="text"' : '') +
        '>' + (isActiveStream || !showAssistantIdentity || errorMessage
          ? renderMarkdown(text) : window.RozsaTools.messageTextMarkup(text)) + '</div>';
    }

    if (turnActivity && ((turnActivity.changedFiles && turnActivity.changedFiles.length) || turnActivity.verification)) {
      body += renderTurnActivityCard(turnActivity);
    }

    return window.RozsaConversation.createMessageFrame('assistant', body, messageIndex, !showAssistantIdentity);

  } else if (role === 'toolResult') {
    div.className = 'msg msg-assistant';
    const text = extractText(content);
    const toolName = msg.toolName || 'tool';
    const toolLabel = toolDisplayLabel(toolName);
    const status = msg.isError ? 's-error' : 's-success';
    trackTool(toolName);
    const preview = (text || '').slice(0, 150);
    div.innerHTML = '<div class="msg-avatar" style="visibility:hidden">R</div><div class="msg-body">' +
      '<div class="tool-call" onclick="toggleToolCall(this)"><div class="tool-track">' +
      '<div class="tool-icon">' + toolIcon(toolName) + '</div>' +
      '<span class="tool-name">' + escapeHtml(toolLabel) + '</span></div>' +
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

