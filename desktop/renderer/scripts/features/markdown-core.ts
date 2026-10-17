export function renderMarkdown(source: string): string {
  if (!source) return "";
  const lines = source.replace(/\r\n/g, "\n").split("\n");
  const html: string[] = [];
  let paragraph: string[] = [];
  let list: { type: string; items: string[] } | null = null;
  let quote: string[] = [];
  let table: { header: string[]; alignments: string[] | null; rows: string[][] } | null = null;
  const flushParagraph = () => {
    if (!paragraph.length) return;
    html.push(`<p>${inlineMarkdown(paragraph.join(" "))}</p>`);
    paragraph = [];
  };
  const flushList = () => {
    if (!list) return;
    html.push(`<${list.type}>${list.items.map(item => {
      if (item.startsWith("[x] ") || item.startsWith("[X] ")) return `<li class="task-list-item"><input type="checkbox" checked disabled> ${inlineMarkdown(item.slice(4))}</li>`;
      if (item.startsWith("[ ] ")) return `<li class="task-list-item"><input type="checkbox" disabled> ${inlineMarkdown(item.slice(4))}</li>`;
      return `<li>${inlineMarkdown(item)}</li>`;
    }).join("")}</${list.type}>`);
    list = null;
  };
  const flushQuote = () => {
    if (!quote.length) return;
    html.push(`<blockquote>${renderMarkdown(quote.join("\n"))}</blockquote>`);
    quote = [];
  };
  const flushTable = () => {
    if (!table) return;
    html.push(renderTable(table));
    table = null;
  };
  const flush = () => { flushParagraph(); flushList(); flushQuote(); flushTable(); };

  for (let index = 0; index < lines.length; index++) {
    const line = lines[index], trimmed = line.trim();
    if (/^```/.test(trimmed)) {
      flush();
      const language = trimmed.slice(3).trim(), code: string[] = [];
      while (++index < lines.length && !/^```/.test(lines[index].trim())) code.push(lines[index]);
      html.push(codeBlock(code.join("\n"), language));
      continue;
    }
    if (/^\$\$/.test(trimmed)) {
      flush();
      const latex: string[] = [];
      while (++index < lines.length && !/^\$\$/.test(lines[index].trim())) latex.push(lines[index]);
      html.push(`<div class="md-latex-block"><code>${escapeHtml(latex.join("\n"))}</code></div>`);
      continue;
    }
    if (!trimmed) { flush(); continue; }
    const heading = trimmed.match(/^(#{1,6})\s+(.+)$/);
    if (heading) { flush(); html.push(`<h${heading[1].length}>${inlineMarkdown(heading[2])}</h${heading[1].length}>`); continue; }
    if (/^(-{3,}|\*{3,}|_{3,})$/.test(trimmed)) { flush(); html.push("<hr>"); continue; }
    if (trimmed.includes("|") && trimmed.startsWith("|")) {
      const cells = parseTableRow(trimmed);
      if (cells) {
        if (!table) { flushParagraph(); flushList(); flushQuote(); table = { header: cells, alignments: null, rows: [] }; }
        else if (!table.alignments) {
          const alignments = parseTableSeparator(trimmed);
          if (alignments) table.alignments = alignments;
          else table.rows.push(cells);
        } else table.rows.push(cells);
        continue;
      }
    } else if (table) flushTable();
    const quoteLine = trimmed.match(/^>\s?(.*)$/);
    if (quoteLine) { flushParagraph(); flushList(); flushTable(); quote.push(quoteLine[1]); continue; }
    const unordered = trimmed.match(/^[-*+]\s+(.+)$/), ordered = trimmed.match(/^\d+[.)]\s+(.+)$/);
    if (unordered || ordered) {
      flushParagraph(); flushQuote(); flushTable();
      const type = unordered ? "ul" : "ol";
      if (list && list.type !== type) flushList();
      if (!list) list = { type, items: [] };
      list.items.push((unordered || ordered)![1]);
      continue;
    }
    const image = trimmed.match(/^!\[([^\]]*)\]\(([^)]+)\)$/);
    if (image) {
      flush();
      html.push(`<div class="md-image"><img src="${escapeHtml(image[2])}" alt="${escapeHtml(image[1])}" style="max-width:100%;border-radius:var(--radius)"></div>`);
      continue;
    }
    flushList(); flushQuote(); flushTable(); paragraph.push(trimmed);
  }
  flush();
  return html.join("");
}

function parseTableRow(line: string): string[] | null {
  const trimmed = line.trim();
  if (!trimmed.startsWith("|")) return null;
  const parts = trimmed.split("|").slice(1);
  if (!parts.length) return null;
  if (parts[parts.length - 1].trim() === "") parts.pop();
  return parts.map(part => part.trim());
}

function parseTableSeparator(line: string): string[] | null {
  const cells = parseTableRow(line);
  if (!cells) return null;
  const alignments: string[] = [];
  for (const cell of cells) {
    if (!/^:?-+:?$/.test(cell)) return null;
    if (cell.startsWith(":") && cell.endsWith(":")) alignments.push("center");
    else if (cell.endsWith(":")) alignments.push("right");
    else alignments.push("left");
  }
  return alignments;
}

function renderTable(state: { header: string[]; alignments: string[] | null; rows: string[][] }): string {
  const alignments = state.alignments || state.header.map(() => "left");
  let html = '<div class="md-table-wrap"><table class="md-table"><thead><tr>';
  for (let index = 0; index < state.header.length; index++) html += `<th style="text-align:${alignments[index] || "left"}">${inlineMarkdown(state.header[index])}</th>`;
  html += "</tr></thead><tbody>";
  for (const row of state.rows) {
    html += "<tr>";
    for (let index = 0; index < row.length; index++) html += `<td style="text-align:${alignments[index] || "left"}">${inlineMarkdown(row[index])}</td>`;
    html += "</tr>";
  }
  return `${html}</tbody></table></div>`;
}

function inlineMarkdown(raw: string): string {
  if (!raw) return "";
  const spans: string[] = [];
  let text = raw.replace(/`([^`]+)`/g, (_match, code: string) => {
    spans.push(`<code>${escapeHtml(code)}</code>`);
    return `\x00${spans.length - 1}\x00`;
  });
  text = text.replace(/\$([^$\n]+)\$/g, (_match, latex: string) => {
    spans.push(`<code class="md-latex-inline">${escapeHtml(latex)}</code>`);
    return `\x00${spans.length - 1}\x00`;
  });
  let html = escapeHtml(text);
  html = html.replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>").replace(/__(.+?)__/g, "<strong>$1</strong>");
  html = html.replace(/~~(.+?)~~/g, "<del>$1</del>").replace(/==(.+?)==/g, "<mark>$1</mark>");
  html = html.replace(/\*([^*\n]+)\*/g, "<em>$1</em>").replace(/(?<![a-zA-Z0-9])_([^_\n]+)_(?![a-zA-Z0-9])/g, "<em>$1</em>");
  html = html.replace(/!\[([^\]]*)\]\(([^)]+)\)/g, '<img src="$2" alt="$1" style="max-height:200px;border-radius:4px">');
  html = html.replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2" target="_blank" rel="noreferrer">$1</a>');
  return html.replace(/\x00(\d+)\x00/g, (_match, index: string) => spans[Number(index)] || "");
}

function codeBlock(code: string, language: string): string {
  const label = escapeHtml(language.toLowerCase() || "text");
  return '<div class="md-code-block"><div class="md-code-head"><span class="md-code-lang">' + label +
    '</span><button class="md-copy" onclick="copyCode(this)" aria-label="Copy code">' +
    '<svg viewBox="0 0 16 16" fill="none" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">' +
    '<rect x="5" y="4" width="8" height="9" rx="1.5"/><path d="M3 10.5V3.5A1.5 1.5 0 014.5 2h6"/></svg>' +
    '</button></div><pre><code>' + escapeHtml(code) + '</code></pre></div>';
}

function escapeHtml(value: string): string {
  if (!value) return "";
  return value.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}
