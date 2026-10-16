import { loadTemplate } from '../core/template-loader.ts';
import { resolveTemplateRoot } from '../core/shell.ts';

let toolTemplate;
export async function loadToolTemplate() {
  const template = await loadTemplate(new URL('composites/tool-call.html', resolveTemplateRoot()));
  toolTemplate = template.firstElementChild.outerHTML;
}

export function escapeToolText(value) {
  return String(value ?? '').replace(/[&<>'"]/g, char => ({'&':'&amp;', '<':'&lt;', '>':'&gt;', "'":'&#39;', '"':'&quot;'}[char]));
}

export function toolMarkup(id, name, arg, output, status, options = {}) {
  if (!toolTemplate) throw new Error('ToolCall template has not been loaded');
  if (!['s-success', 's-running', 's-error'].includes(status)) throw new TypeError('Unsupported tool status');
  const values = {
    id: escapeToolText(id), name: escapeToolText(name), args: escapeToolText(arg), status,
    classes: (options.expanded ? ' expanded' : '') + (options.className ? ' ' + escapeToolText(options.className) : ''),
    bodyClass: options.bodyClass ? ' ' + escapeToolText(options.bodyClass) : '',
    body: options.bodyHtml || '<pre style="white-space:pre-wrap;margin:0">' + escapeToolText(output || '') + '</pre>',
  };
  return toolTemplate.replace(/\{\{(\w+)\}\}/g, (_, key) => {
    if (!Object.hasOwn(values, key)) throw new Error('Unknown tool template slot: ' + key);
    return values[key];
  });
}

export function codeViewMarkup(content) {
  return String(content || '').split('\n').map((line, index) =>
    '<div class="code-line"><span class="code-ln">' + (index + 1) + '</span><span class="code-text">' + escapeToolText(line) + '</span></div>'
  ).join('');
}

export function diffViewMarkup(patch) {
  let oldLine = 1;
  let newLine = 1;
  return String(patch || '').split('\n').map(line => {
    const hunk = line.match(/^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/);
    if (hunk) { oldLine = Number.parseInt(hunk[1], 10); newLine = Number.parseInt(hunk[2], 10); return ''; }
    if (line.startsWith('---') || line.startsWith('+++') || !line) return '';
    if (line.startsWith('-')) return '<div class="diff-line diff-del"><span class="diff-sign">−</span><span class="diff-ln">' + oldLine++ + '</span><span class="diff-text">' + escapeToolText(line.slice(1)) + '</span></div>';
    if (line.startsWith('+')) return '<div class="diff-line diff-add"><span class="diff-sign">+</span><span class="diff-ln">' + newLine++ + '</span><span class="diff-text">' + escapeToolText(line.slice(1)) + '</span></div>';
    const lineNumber = newLine;
    oldLine++; newLine++;
    return '<div class="diff-line"><span class="diff-sign"> </span><span class="diff-ln">' + lineNumber + '</span><span class="diff-text">' + escapeToolText(line.slice(1)) + '</span></div>';
  }).join('');
}

export function evidenceMarkup(label, output) {
  return '<div class="tool-evidence"><div class="tool-evidence-section"><div class="tool-evidence-label">' +
    escapeToolText(label) + '</div><pre class="tool-evidence-output">' + escapeToolText(output || '') + '</pre></div></div>';
}

export function evidenceViewMarkup(kind, content) {
  if (kind !== 'code' && kind !== 'diff') throw new TypeError('Unsupported evidence view');
  return '<div class="tool-evidence"><div class="tool-evidence-section"><div class="tool-evidence-label">' +
    (kind === 'code' ? 'File content' : 'Diff') + '</div><div class="' + kind + '-view tool-evidence-' + kind + '-view">' +
    (kind === 'code' ? codeViewMarkup(content) : diffViewMarkup(content)) + '</div></div></div>';
}

export function changesMarkup(files, verification) {
  const icon = name => '<svg class="lucide" data-lucide="' + name + '" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false">' + {
    'file-plus-2': '<path d="M4 22h14a2 2 0 0 0 2-2V7l-5-5H6a2 2 0 0 0-2 2v4"/><path d="M14 2v6h6"/><path d="M3 15h6"/><path d="M6 12v6"/>',
    'file-pen-line': '<path d="M12 20h9"/><path d="M16.5 3.5a2.12 2.12 0 0 1 3 3L8 18l-4 1 1-4Z"/><path d="m15 5 3 3"/>',
    'chevron-right': '<path d="m9 18 6-6-6-6"/>',
  }[name] + '</svg>';
  const rows = (files || []).map(file => '<div class="change-entry"><div class="change-row">' +
    '<span class="change-icon ' + (file.status === 'added' ? 'new' : 'mod') + '">' + icon(file.status === 'added' ? 'file-plus-2' : 'file-pen-line') + '</span>' +
    '<button class="change-name" type="button" aria-expanded="false" data-turn-diff="' + escapeToolText(JSON.stringify({patch:file.patch || ''})) + '">' +
    escapeToolText(file.path) + '</button><span class="change-add">+' + escapeToolText(file.added) + '</span><span class="change-del">-' + escapeToolText(file.deleted) + '</span><span class="change-toggle">' + icon('chevron-right') + '</span>' +
    '</div><div class="turn-diff-inline" hidden></div></div>').join('');
  const footer = verification ? '<div class="changes-footer"><span class="change-add">Verified</span><span class="changes-runtime">' +
    escapeToolText(verification.command) + ' · exit ' + escapeToolText(verification.exitCode) + ' · ' + escapeToolText(verification.duration) + '</span></div>' : '';
  return '<div class="changes-card"><div class="changes-header"><span>Changes: ' + (files || []).length + ' files</span></div>' +
    '<div class="changes-list">' + rows + '</div>' + footer + '</div>';
}

export function textMarkup(value) {
  return escapeToolText(value || '').split(/\n{2,}/).filter(Boolean).map(paragraph =>
    '<p>' + paragraph.replace(/\n/g, '<br>') + '</p>'
  ).join('');
}

export function messageTextMarkup(value) {
  const lines = String(value || '').replace(/\r\n/g, '\n').split('\n');
  const blocks = [];
  let paragraph = [];
  let code = null;
  const flush = () => {
    if (!paragraph.length) return;
    const text = paragraph.join('\n');
    // Underscores inside identifiers are plain text, including Unicode words.
    const underscoreEmphasis = /(?:^|[^\p{L}\p{N}\p{M}_])_{1,3}[^_\s](?:[^\n]*[^\s])?_{1,3}(?![\p{L}\p{N}\p{M}_])/u.test(text);
    const missing = underscoreEmphasis || /(^|\n)(?: {4}|\t)|(^|\n)\s*(?:#{1,6}\s|>|[-*+]\s|\d+[.)]\s|\||~{3,}|={3,}$|(?:[-*_]\s*){3,}$)|!?\[[^\]]*\](?:\(|\[|:)|<\/?[A-Za-z!]|`|\*[^*\n]+\*|\*\*/m.test(text);
    blocks.push(missing ? '<div data-ui-suspended="UI-MARKDOWN-001"></div>' : textMarkup(text));
    paragraph = [];
  };
  for (const line of lines) {
    if (/^\s*```/.test(line)) {
      flush();
      if (code === null) code = [];
      else { blocks.push('<div class="code-view">' + codeViewMarkup(code.join('\n')) + '</div>'); code = null; }
    } else if (code !== null) code.push(line);
    else if (!line.trim()) flush();
    else paragraph.push(line);
  }
  flush();
  if (code !== null) blocks.push('<div data-ui-suspended="UI-MARKDOWN-001"></div>');
  return blocks.join('');
}
