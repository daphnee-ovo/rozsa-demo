// @ts-nocheck
function usesCombinedAttachmentPicker() {
  return /Macintosh|Mac OS X/.test(navigator.userAgent);
}

function configureAttachmentPicker() {
  const fileButton = document.getElementById('attachFileButton');
  const directoryButton = document.getElementById('attachDirectoryButton');
  const combined = usesCombinedAttachmentPicker();

  if (fileButton) {
    fileButton.title = combined ? 'Attach file or folder' : 'Attach file';
    fileButton.setAttribute('aria-label', fileButton.title);
  }
  if (directoryButton) directoryButton.hidden = combined;
}

const nativeFileDragEvents = {
  'native-file-drag-enter': 'enter',
  'native-file-drag-over': 'over',
  'native-file-drag-drop': 'drop',
  'native-file-drag-leave': 'leave',
};

function handleNativeFileDrag(type, payload = {}) {
  if (conversationOperationSuspended('attachments')) return;
  const inputWrapper = document.querySelector('.input-wrapper');
  const active = type === 'enter' || type === 'over';
  inputWrapper?.classList.toggle('file-drop-active', active);

  if (type !== 'drop') return;

  const paths = Array.isArray(payload?.paths)
    ? payload.paths.filter(path => typeof path === 'string' && path.length > 0)
    : [];
  if (paths.length > 0) {
    insertFileReferences(paths);
  }
}

async function configureNativeFileDrag() {
  const input = document.getElementById('msgInput');
  if (!input || typeof listen !== 'function') return;

  // Native host file-drop events deliver paths. Prevent the
  // contenteditable default from navigating or inserting an opaque File node.
  input.addEventListener('dragover', event => event.preventDefault());
  input.addEventListener('drop', event => event.preventDefault());

  try {
    await Promise.all(Object.entries(nativeFileDragEvents).map(([eventName, type]) => (
      listen(eventName, event => handleNativeFileDrag(type, event.payload))
    )));
  } catch (error) {
    showError('Native file drag listener failed: ' + String(error));
  }
}

async function attachFileReference() {
  if (conversationOperationSuspended('attachments')) return;
  const mode = usesCombinedAttachmentPicker() ? 'any' : 'file';
  await attachReference(mode);
}

async function attachDirectoryReference() {
  if (conversationOperationSuspended('attachments')) return;
  await attachReference('directory');
}

async function attachReference(mode) {
  if (conversationOperationSuspended('attachments')) return;
  try {
    const path = await invoke('pick_attachment', { mode });
    if (conversationOperationSuspended('attachments')) return;
    if (!path) return;
    insertFileReferences([path]);
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
  const current = getInputText(input);
  const selection = getInputSelection(input);
  const start = selection.start;
  const end = selection.end;
  setInputText(input, current.slice(0, start) + text + current.slice(end));
  const cursor = start + text.length;
  setInputSelection(input, cursor);
  input.focus();
  autoResize(input);
  updateAutocomplete();
}

function insertFileReferences(paths) {
  if (conversationOperationSuspended('attachments')) return;
  const input = document.getElementById('msgInput');
  if (!input || !Array.isArray(paths) || paths.length === 0) return;
  const current = getInputText(input);
  const selection = getInputSelection(input);
  const beforeSelection = current.slice(0, selection.start);
  const separator = beforeSelection.length > 0 && !/\s$/.test(beforeSelection) ? ' ' : '';
  insertInputText(separator + paths.map(formatFileReference).join(''));
}

function formatFileReference(path) {
  if (path.includes('"')) return '@' + path + ' ';
  if (/\s/.test(path)) return '@"' + path + '" ';
  return '@' + path + ' ';
}

function autoResize(el) {
  el.style.height = 'auto';
  el.style.height = Math.min(el.scrollHeight, 220) + 'px';
}

// =============== Slash Command Autocomplete ===============

let acVisible = false;

async function updateAutocomplete() {
  if (conversationOperationSuspended('autocomplete')) return;
  const input = document.getElementById('msgInput');
  const popup = document.getElementById('autocomplete');
  if (!input || !popup) return;
  const val = getInputText(input);
  const cursor = getInputCursor(input);
  const seq = ++acRequestSeq;
  let result = null;
  try {
    result = await invoke('autocomplete_input', { text: val, cursor });
  } catch (e) {
    hideAutocomplete();
    return;
  }
  if (isInputComposing || seq !== acRequestSeq) return;
  if (conversationOperationSuspended('autocomplete')) return;
  const highlightRanges = result.highlightRanges || [];
  setInputMatchState(!!result.validMatch || highlightRanges.length > 0);
  updateInputHighlight(highlightRanges);
  if (!result.items || result.items.length === 0 || !result.prefix) {
    hideAutocomplete(false);
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
  if (conversationOperationSuspended('autocomplete')) return;
  acSelectedIndex = index;
  const popup = document.getElementById('autocomplete');
  if (!popup) return;
  popup.querySelectorAll('.ac-item').forEach((el, i) => {
    el.classList.toggle('active', i === index);
  });
}

function navigateAutocomplete(direction) {
  if (conversationOperationSuspended('autocomplete')) return false;
  const popup = document.getElementById('autocomplete');
  if (!popup || !acVisible) return false;
  const items = popup.querySelectorAll('.ac-item');
  if (items.length === 0) return false;

  if (direction === 'down') {
    acSelectedIndex = (acSelectedIndex + 1) % items.length;
  } else {
    acSelectedIndex = acSelectedIndex <= 0 ? items.length - 1 : acSelectedIndex - 1;
  }

  items.forEach((el, i) => el.classList.toggle('active', i === acSelectedIndex));
  items[acSelectedIndex].scrollIntoView({ block: 'nearest' });
  return true;
}

function confirmAutocomplete() {
  if (conversationOperationSuspended('autocomplete')) return false;
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
  if (conversationOperationSuspended('autocomplete')) return;
  const input = document.getElementById('msgInput');
  const item = acItems[index];
  if (!input || !item || !acPrefix) return;
  const text = getInputText(input);
  const cursor = getInputCursor(input);
  const start = Math.max(0, cursor - acPrefix.length);
  setInputText(input, text.slice(0, start) + item.value + text.slice(cursor));
  const nextCursor = start + item.value.length;
  setInputSelection(input, nextCursor);
  input.focus();
  autoResize(input);
  hideAutocomplete(false);
  updateAutocomplete();
}

function hideAutocomplete(clearMatch = true) {
  const popup = document.getElementById('autocomplete');
  if (popup) {
    popup.classList.remove('visible');
    popup.replaceChildren();
  }
  acVisible = false;
  acSelectedIndex = -1;
  acPrefix = '';
  acItems = [];
  if (clearMatch) {
    setInputMatchState(inputHighlightRanges.length > 0);
  }
}

function isTransientPopupVisible(popup) {
  if (!popup) return false;
  if (popup.id === 'autocomplete' || popup.id === 'quotaTooltip') {
    return popup.classList.contains('visible');
  }
  return !popup.hidden;
}

function hideTransientPopup(popup) {
  if (popup.id === 'autocomplete') {
    acRequestSeq++;
    hideAutocomplete();
  } else if (popup.id === 'quotaTooltip') {
    hideQuotaTooltip();
  } else if (popup.id === 'thinkingEffortPopover') {
    hideThinkingEffortPicker();
  } else {
    popup.hidden = true;
  }
}

function dismissTransientPopupsOutside(target) {
  let dismissed = false;
  for (const id of TRANSIENT_POPUP_IDS) {
    const popup = document.getElementById(id);
    if (!isTransientPopupVisible(popup) || popup.contains(target)) continue;
    hideTransientPopup(popup);
    dismissed = true;
  }
  return dismissed;
}

function dismissTransientPopups() {
  let dismissed = false;
  for (const id of TRANSIENT_POPUP_IDS) {
    const popup = document.getElementById(id);
    if (!isTransientPopupVisible(popup)) continue;
    hideTransientPopup(popup);
    dismissed = true;
  }
  return dismissed;
}

function setInputMatchState(valid) {
  const wrapper = document.querySelector('.input-wrapper');
  if (wrapper) wrapper.classList.toggle('valid-token', valid);
}

function updateInputHighlight(ranges) {
  inputHighlightRanges = Array.isArray(ranges) ? ranges : [];
  const input = document.getElementById('msgInput');
  if (!input || isInputComposing) return;
  renderRichInputHighlights(input, inputHighlightRanges, true);
}

function renderRichInputHighlights(input, ranges, resize = false) {
  const text = getInputText(input);
  const selection = getInputSelection(input);
  if (!Array.isArray(ranges) || ranges.length === 0) {
    setInputText(input, text);
    setInputSelection(input, selection.start, selection.end);
    if (resize) autoResize(input);
    return;
  }
  const chars = Array.from(text);
  const fragment = document.createDocumentFragment();
  let cursor = 0;
  const normalized = ranges
    .map(range => ({
      start: Math.max(0, Math.min(chars.length, Number(range.start || 0))),
      end: Math.max(0, Math.min(chars.length, Number(range.end || 0))),
    }))
    .filter(range => range.end > range.start)
    .sort((a, b) => a.start - b.start || a.end - b.end);
  for (const range of normalized) {
    if (range.start < cursor) continue;
    fragment.appendChild(document.createTextNode(chars.slice(cursor, range.start).join('')));
    const span = document.createElement('span');
    span.className = 'valid-token-text';
    span.textContent = chars.slice(range.start, range.end).join('');
    fragment.appendChild(span);
    cursor = range.end;
  }
  fragment.appendChild(document.createTextNode(chars.slice(cursor).join('')));
  input.replaceChildren(fragment);
  setInputSelection(input, selection.start, selection.end);
  if (resize) autoResize(input);
}

function syncInputHighlightScroll() {
  // contenteditable renders highlights directly; no overlay scroll sync is needed.
}

