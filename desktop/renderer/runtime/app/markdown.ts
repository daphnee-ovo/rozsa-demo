// @ts-nocheck
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

function applyFontSize(value) {
  const fontSize = clampFontSize(value);
  const root = document.documentElement;
  root.style.setProperty('--ui-font-size', fontSize + 'px');
  root.style.setProperty('--ui-scale', String(fontSize / 14));
}
