// @ts-nocheck
const DEV_FLOW_RESOURCE_VALUE_OPTIONS = {
  task: {
    update: ['--title', '--task-type', '--priority', '--refs', '--file', '--depends-on', '--parallel', '--complexity', '--done-when'],
    remove: ['--confirm'],
    reopen: ['--confirm'],
  },
  issue: {
    update: ['--title', '--severity', '--location', '--desc', '--reproduce', '--fix', '--file'],
    remove: ['--confirm'],
    reopen: ['--confirm'],
  },
};

function parseDevFlowAction(words, output) {
  const invocation = parseDevFlowInvocation(words);
  if (!invocation) return null;
  if (invocation.command === 'claim') {
    const claim = parseDevFlowClaimArgs(invocation.args);
    if (!claim) return null;
    return {
      action: claim.revoke ? 'released' : 'claimed',
      expectedKind: null,
      ids: claim.ids,
      allowEmpty: claim.revoke && claim.ids.length === 0,
    };
  }

  const { kind, operation, args } = invocation;
  if (operation === 'create') {
    const ids = parseCreatedDevFlowIds(output, kind);
    return ids ? { action: 'created', expectedKind: kind, ids, allowEmpty: false } : null;
  }

  const actionByOperation = {
    update: 'updated',
    remove: 'removed',
    done: 'completed',
    close: 'closed',
    reopen: 'reopened',
  };
  const action = actionByOperation[operation];
  if (!action) return null;
  if ((kind === 'task' && operation === 'close') ||
      (kind === 'issue' && operation === 'done')) return null;

  const ids = parseDevFlowResourceIds(
    args,
    kind,
    DEV_FLOW_RESOURCE_VALUE_OPTIONS[kind]?.[operation] || [],
    operation === 'done' || operation === 'close',
  );
  return ids ? { action, expectedKind: kind, ids, allowEmpty: false } : null;
}

function parseDevFlowInvocation(words) {
  if (!Array.isArray(words) || !words.length || !isDevFlowExecutable(words[0])) return null;
  const args = words.slice(1);
  let index = 0;
  while (isDevFlowFormatOption(args[index])) index++;
  if (args[index] === 'claim') {
    return { command: 'claim', args: args.slice(index + 1) };
  }
  if (args[index] !== 'task' && args[index] !== 'issue') return null;
  const kind = args[index++];
  while (isDevFlowFormatOption(args[index])) index++;
  const operation = args[index++];
  if (!operation) return null;
  return { kind, operation, args: args.slice(index) };
}

function isDevFlowFormatOption(value) {
  return value === '-H' || value === '--human';
}

function splitDevFlowOption(value) {
  const text = String(value || '');
  const separator = text.indexOf('=');
  return separator < 0
    ? { name: text, inlineValue: null }
    : { name: text.slice(0, separator), inlineValue: text.slice(separator + 1) };
}

function parseDevFlowClaimArgs(args) {
  const ids = [];
  let revoke = false;
  for (let index = 0; index < args.length; index++) {
    const option = splitDevFlowOption(args[index]);
    if (isDevFlowFormatOption(args[index])) continue;
    if (option.name === '--revoke') {
      if (option.inlineValue !== null) return null;
      revoke = true;
      continue;
    }
    if (option.name === '--timeout') {
      const value = option.inlineValue !== null ? option.inlineValue : args[++index];
      const timeout = Number(value);
      if (!Number.isInteger(timeout) || timeout <= 0) return null;
      continue;
    }
    if (option.name.startsWith('-')) return null;
    const parsed = normalizeDevFlowId(args[index]);
    if (!parsed) return null;
    ids.push(parsed);
  }
  return { revoke, ids };
}

function parseDevFlowResourceIds(args, expectedKind, valueOptions, multiple) {
  const ids = [];
  for (let index = 0; index < args.length; index++) {
    const raw = args[index];
    if (isDevFlowFormatOption(raw)) continue;
    const option = splitDevFlowOption(raw);
    if (valueOptions.includes(option.name)) {
      const value = option.inlineValue !== null ? option.inlineValue : args[++index];
      if (value === undefined || value === '') return null;
      continue;
    }
    if (option.name.startsWith('-')) return null;
    const parsed = normalizeDevFlowId(raw);
    if (!parsed || parsed.kind !== expectedKind) return null;
    ids.push(parsed);
  }
  if (!ids.length || (!multiple && ids.length !== 1)) return null;
  return ids;
}

function parseDevFlowBashCommand(command) {
  if (typeof command !== 'string' || !command.trim()) return null;
  const withoutStderrRedirect = command.replace(/\s+2>&1\s*$/, '').trim();
  if (!withoutStderrRedirect || /[;&`]/.test(withoutStderrRedirect) || withoutStderrRedirect.includes('$(')) {
    return null;
  }
  const stages = splitDevFlowPipeline(withoutStderrRedirect);
  if (!stages) return null;
  const tokens = tokenizeDevFlowStage(stages[stages.length - 1]);
  if (!tokens) return null;
  const words = [];
  for (let index = 0; index < tokens.length; index++) {
    if (tokens[index] === '<') {
      if (typeof tokens[index + 1] !== 'string' || !tokens[index + 1]) return null;
      index++;
    } else {
      words.push(tokens[index]);
    }
  }
  return { words, stageCount: stages.length };
}

function splitDevFlowPipeline(command) {
  const stages = [''];
  let quote = null;
  let escaped = false;
  for (const char of command) {
    if (escaped) {
      stages[stages.length - 1] += char;
      escaped = false;
      continue;
    }
    if (quote === '\'') {
      stages[stages.length - 1] += char;
      if (char === '\'') quote = null;
      continue;
    }
    if (quote === '"') {
      stages[stages.length - 1] += char;
      if (char === '"') quote = null;
      else if (char === '\\') escaped = true;
      continue;
    }
    if (char === '\\') {
      stages[stages.length - 1] += char;
      escaped = true;
    } else if (char === '\'' || char === '"') {
      stages[stages.length - 1] += char;
      quote = char;
    } else if (char === '|') {
      if (!stages[stages.length - 1].trim()) return null;
      stages.push('');
    } else {
      stages[stages.length - 1] += char;
    }
  }
  if (quote || escaped || !stages[stages.length - 1].trim()) return null;
  return stages;
}

function tokenizeDevFlowStage(stage) {
  const tokens = [];
  let word = '';
  let started = false;
  let quote = null;
  let escaped = false;
  const flush = () => {
    if (started) {
      tokens.push(word);
      word = '';
      started = false;
    }
  };
  for (const char of stage) {
    if (escaped) {
      word += char;
      started = true;
      escaped = false;
      continue;
    }
    if (quote === '\'') {
      if (char === '\'') quote = null;
      else word += char;
      started = true;
      continue;
    }
    if (quote === '"') {
      if (char === '"') quote = null;
      else if (char === '\\') escaped = true;
      else word += char;
      started = true;
      continue;
    }
    if (char === '\\') {
      escaped = true;
    } else if (char === '\'' || char === '"') {
      quote = char;
      started = true;
    } else if (char === '<') {
      flush();
      tokens.push('<');
    } else if (char === '>' || char === '&') {
      return null;
    } else if (/\s/.test(char)) {
      flush();
    } else {
      word += char;
      started = true;
    }
  }
  if (quote || escaped) return null;
  flush();
  return tokens;
}

function isDevFlowExecutable(value) {
  return value === 'dow' || /(?:^|[\\/])dow$/.test(value);
}

function parseCreatedDevFlowIds(output, expectedKind) {
  const matches = String(output || '').match(/\b(?:TASK-T|ISSUE-I)\d+\b/gi) || [];
  const ids = matches.map(normalizeDevFlowId).filter(Boolean);
  if (!ids.length || ids.some(item => item.kind !== expectedKind)) return null;
  return ids;
}

function normalizeDevFlowId(value) {
  const match = String(value || '').match(/^(TASK-T|T|ISSUE-I|I)(\d+)$/i);
  if (!match || Number(match[2]) <= 0) return null;
  const kind = match[1].toUpperCase().startsWith('I') ? 'issue' : 'task';
  const prefix = kind === 'issue' ? 'ISSUE-I' : 'TASK-T';
  const id = prefix + String(Number(match[2])).padStart(3, '0');
  return { kind, id };
}

function devFlowShortId(id) {
  return String(id).replace(/^(?:TASK|ISSUE)-/, '');
}

function formatBashDevFlowTitle(presentation) {
  const action = {
    created: 'Created',
    updated: 'Updated',
    removed: 'Removed',
    claimed: 'Claimed',
    released: 'Released',
    completed: 'Completed',
    closed: 'Closed',
    reopened: 'Reopened',
  }[presentation.action] || 'Dev-flow';
  const items = Array.isArray(presentation.items) ? presentation.items : [];
  if (!items.length && presentation.action === 'released') {
    return { name: action, arg: 'all claims' };
  }
  const arg = items.map(item => {
    const kind = item.kind === 'issue' ? 'Issue' : 'Task';
    return kind + ' ' + (item.shortId || item.id || '') + ' ' + (item.title || 'Details unavailable');
  }).join(' · ');
  return { name: action, arg };
}

function renderBashToolEvidence(toolCall, result) {
  const details = result.details || {};
  const command = toolCall.arguments && toolCall.arguments.command
    ? toolCall.arguments.command
    : '';
  const facts = [
    details.exit_code !== undefined && details.exit_code !== null ? 'exit ' + details.exit_code : 'exit unavailable',
    Number.isFinite(details.duration_ms) ? details.duration_ms + 'ms' : 'duration unavailable',
    Number.isFinite(details.timeout_ms) ? 'timeout ' + details.timeout_ms + 'ms' : 'timeout unavailable',
    details.truncated ? 'truncated' : 'not truncated',
  ];
  return '<div class="dev-flow-tool-command">$ ' + escapeHtml(command) + '</div>' +
    '<div class="dev-flow-tool-meta">' + facts.map(escapeHtml).join(' · ') + '</div>' +
    '<pre>' + escapeHtml(result.output || '') + '</pre>';
}

function toolResultDetails(result) {
  const details = result && result.details;
  return details && typeof details === 'object' && !Array.isArray(details) ? details : {};
}

function toolResultOutput(result) {
  return result && result.output !== undefined && result.output !== null
    ? String(result.output)
    : '';
}

function toolEvidenceMeta(facts) {
  const values = facts
    .filter(value => value !== null && value !== undefined && String(value).trim())
    .map(value => String(value));
  return values.length
    ? '<div class="tool-evidence-meta">' + values.map(escapeHtml).join(' · ') + '</div>'
    : '';
}

function toolEvidenceOutput(output, label = 'Output') {
  const text = String(output ?? '');
  if (!text) return '';
  return '<div class="tool-evidence-section"><div class="tool-evidence-label">' +
    escapeHtml(label) + '</div><pre class="tool-evidence-output">' + escapeHtml(text) + '</pre></div>';
}

function toolEvidencePending(label) {
  return '<div class="tool-evidence-pending">' + escapeHtml(label) + '</div>';
}

function toolCallArgs(toolCall) {
  return toolCall && toolCall.arguments && typeof toolCall.arguments === 'object'
    ? toolCall.arguments
    : {};
}

function toolResultPath(toolCall, details, delta = null) {
  const args = toolCallArgs(toolCall);
  return details.file_path || details.path || (delta && delta.path) || args.file_path || args.path || '';
}

function renderReadToolEvidence(toolCall, result) {
  const args = toolCallArgs(toolCall);
  const details = toolResultDetails(result);
  const path = toolResultPath(toolCall, details);
  const facts = [
    path ? 'File ' + path : 'File unavailable',
    formatReadRange(args) ? 'Requested ' + formatReadRange(args) : '',
    Number.isFinite(details.total_lines) ? details.total_lines + ' total lines' : '',
    Number.isFinite(details.output_lines) ? details.output_lines + ' lines shown' : '',
    details.truncated
      ? 'truncated' + (details.truncated_by ? ' by ' + details.truncated_by : '')
      : (result ? 'complete' : ''),
    details.first_line_exceeds_limit ? 'first line exceeds output limit' : '',
    result ? 'line numbers preserved from tool output' : '',
  ];
  const body = toolEvidenceMeta(facts) + (result
    ? toolEvidenceOutput(toolResultOutput(result))
    : toolEvidencePending('Awaiting file contents'));
  return { html: body, className: ' tool-evidence' };
}

function renderWriteEditToolEvidence(toolCall, result, mode) {
  const args = toolCallArgs(toolCall);
  const details = toolResultDetails(result);
  const deltas = Array.isArray(details.file_deltas) ? details.file_deltas : [];
  const delta = deltas[0] || null;
  const path = toolResultPath(toolCall, details, delta);
  const status = delta && typeof delta.status === 'string' ? delta.status.toLowerCase() : '';
  const action = mode === 'write'
    ? (status === 'added' ? 'created' : (status === 'deleted' ? 'deleted' : 'updated'))
    : 'edited';
  const facts = [
    path ? 'File ' + path : 'File unavailable',
    action,
    mode === 'write' && Number.isFinite(details.bytes_written) ? details.bytes_written + ' bytes' : '',
    mode === 'write' && Number.isFinite(details.line_count) ? details.line_count + ' lines' : '',
    mode === 'edit' && Number.isFinite(details.replacements) ? details.replacements + ' replacement' + (details.replacements === 1 ? '' : 's') : '',
    delta && Number.isFinite(delta.added) ? 'delta +' + delta.added + '/-' + (Number.isFinite(delta.deleted) ? delta.deleted : 0) : '',
    result && result.isError ? 'error' : '',
  ];
  let body = toolEvidenceMeta(facts);
  const output = toolResultOutput(result);
  body += output ? toolEvidenceOutput(output, 'Result') : '';

  const content = delta && typeof delta.after === 'string'
    ? delta.after
    : (typeof args.content === 'string' ? args.content : null);
  if (mode === 'write' && content !== null) {
    body += '<div class="tool-evidence-section"><div class="tool-evidence-label">File content</div>' +
      '<div class="code-view tool-evidence-code-view">' + renderCodeView(content) + '</div></div>';
  }
  if (mode === 'edit' && delta && typeof delta.patch === 'string' && delta.patch) {
    body += '<div class="tool-evidence-section"><div class="tool-evidence-label">Diff</div>' +
      '<div class="diff-view tool-evidence-diff-view">' + renderDiffView(delta.patch) + '</div></div>';
  }
  if (!body) body = toolEvidencePending('Awaiting file result');
  return { html: body, className: ' tool-evidence' };
}

function subagentActionLabel(action) {
  const labels = { spawn: 'Spawn', send: 'Send', wait: 'Wait', interrupt: 'Interrupt', list: 'List' };
  return labels[normalizeToolName(action)] || 'Subagent';
}

function subagentModelLabel(info) {
  const provider = info && info.model_provider ? String(info.model_provider) : '';
  const model = info && info.model_id ? String(info.model_id) : '';
  return provider && model ? provider + '/' + model : (model || provider);
}

function renderSubagentList(items) {
  if (!items.length) return '<div class="tool-evidence-pending">No subagents</div>';
  return '<div class="tool-evidence-list">' + items.map(info => {
    const target = info.name || info.id || 'subagent';
    const status = info.status ? String(info.status) : 'unknown';
    const model = subagentModelLabel(info);
    return '<div class="tool-evidence-row"><span class="tool-evidence-key">' +
      escapeHtml(target) + '</span><span class="tool-evidence-value">' +
      escapeHtml([info.id && info.id !== target ? info.id : '', status, model].filter(Boolean).join(' · ')) +
      '</span></div>';
  }).join('') + '</div>';
}

function renderSubagentToolEvidence(toolCall, result) {
  const args = toolCallArgs(toolCall);
  const details = toolResultDetails(result);
  const action = details.action || args.action || '';
  const target = details.name || details.id || args.name || args.id || '';
  const facts = [
    action ? subagentActionLabel(action) : 'Subagent',
    target ? target : (action === 'list' ? 'all subagents' : ''),
    details.status ? 'status ' + details.status : '',
    subagentModelLabel(details) ? 'model ' + subagentModelLabel(details) : '',
    details.thinking_effort ? 'thinking ' + details.thinking_effort : '',
  ];
  let body = toolEvidenceMeta(facts);
  if (Array.isArray(details.subagents)) body += renderSubagentList(details.subagents);
  if (result) {
    body += toolEvidenceOutput(toolResultOutput(result), 'Summary');
  } else {
    body += toolEvidencePending('Awaiting subagent result');
  }
  return { html: body, className: ' tool-evidence' };
}

function formatQuestionAnswer(value) {
  if (Array.isArray(value)) return value.join(', ');
  if (value && typeof value === 'object') return JSON.stringify(value);
  return String(value ?? '');
}

function renderQuestionToolEvidence(toolCall, result) {
  const args = toolCallArgs(toolCall);
  const details = toolResultDetails(result);
  const questions = Array.isArray(args.questions) ? args.questions : [];
  const answers = details.answers && typeof details.answers === 'object' && !Array.isArray(details.answers)
    ? details.answers
    : null;
  const facts = [
    questions.length + (questions.length === 1 ? ' question' : ' questions'),
    result ? (result.isError ? 'error' : (answers ? 'answered' : 'completed')) : 'waiting for answer',
  ];
  let body = toolEvidenceMeta(facts);
  if (questions.length) {
    body += '<div class="tool-evidence-list">' + questions.map(question =>
      '<div class="tool-evidence-question"><span class="tool-evidence-key">' +
      escapeHtml(question.header || 'Question') + '</span><span class="tool-evidence-value">' +
      escapeHtml(question.question || '') + '</span></div>'
    ).join('') + '</div>';
  }
  if (answers) {
    body += '<div class="tool-evidence-list tool-evidence-answers">' + Object.entries(answers).map(([header, answer]) =>
      '<div class="tool-evidence-row"><span class="tool-evidence-key">' + escapeHtml(header) +
      '</span><span class="tool-evidence-value">' + escapeHtml(formatQuestionAnswer(answer)) + '</span></div>'
    ).join('') + '</div>';
  } else if (result && result.isError) {
    body += toolEvidenceOutput(toolResultOutput(result), 'Error');
  } else if (result && !answers && toolResultOutput(result)) {
    body += toolEvidenceOutput(toolResultOutput(result), 'Result (raw fallback)');
  } else if (!result) {
    body += toolEvidencePending('The separate question panel is awaiting your answer');
  }
  return { html: body, className: ' tool-evidence' };
}

function renderGenericToolEvidence(toolCall, result) {
  const label = toolDisplayLabel(toolCall && toolCall.name);
  if (!result) {
    return {
      html: toolEvidenceMeta(['Arguments ' + formatToolArgs(toolCall)]) +
        toolEvidencePending('Awaiting ' + label + ' result'),
      className: ' tool-evidence',
    };
  }
  const details = toolResultDetails(result);
  const facts = [
    result.isError ? 'error' : 'completed',
    details.truncated ? 'truncated' : '',
  ];
  return {
    html: toolEvidenceMeta(facts) + toolEvidenceOutput(toolResultOutput(result)),
    className: ' tool-evidence',
  };
}

