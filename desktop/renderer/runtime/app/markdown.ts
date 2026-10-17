// @ts-nocheck
function renderMarkdown(source) {
  const renderer = window.RozsaMarkdownRenderer;
  if (typeof renderer !== 'function') throw new Error('Markdown renderer is not initialized');
  return renderer(source);
}

function escapeHtml(value) {
  if (!value) return '';
  return value.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

function applyFontSize(value) {
  const fontSize = clampFontSize(value);
  const root = document.documentElement;
  root.style.setProperty('--ui-font-size', fontSize + 'px');
  root.style.setProperty('--ui-scale', String(fontSize / 14));
}

window.renderMarkdown = renderMarkdown;
