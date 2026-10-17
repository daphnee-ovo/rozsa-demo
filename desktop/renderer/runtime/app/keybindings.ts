// @ts-nocheck
const FIXED_KEY_BINDINGS = Object.freeze([
  { binding: 'Escape', title: 'Dismiss the current panel' },
  { binding: 'Double Escape', title: 'Abort the active response' },
  { binding: 'Y / T / N / H', title: 'Choose a permission response' },
  { binding: 'J / K / ↑ / ↓ / Enter', title: 'Move through permission choices' },
  { binding: '1–9 / N / D / Enter', title: 'Choose or advance a question or trust option' },
  { binding: 'Tab / ↑ / ↓ / Enter', title: 'Confirm or navigate autocomplete' },
]);

async function loadKeyBindings() {
  try {
    keyBindingDefinitions = await invoke('get_key_bindings');
    setKeyBindingError('');
    renderKeyBindings(document.getElementById('shortcutSearch')?.value || '');
  } catch (error) {
    setKeyBindingError(String(error));
    throw error;
  }
}

function setKeyBindingError(message) {
  const error = document.getElementById('shortcutError');
  if (!error) return;
  error.textContent = message;
  error.hidden = !message;
}

function bindingFor(action) {
  return keyBindingDefinitions.find(definition => definition.action === action)?.binding || '';
}

function canonicalKeyEvent(event) {
  if (['Control', 'Meta', 'Alt', 'Shift'].includes(event.key)) return '';
  const parts = [];
  if (event.ctrlKey) parts.push('Ctrl');
  if (event.metaKey) parts.push('Meta');
  if (event.altKey) parts.push('Alt');
  if (event.shiftKey) parts.push('Shift');
  let key = event.key;
  if (key === ' ') key = 'Space';
  if (key.length === 1 && /[a-z]/i.test(key)) key = key.toUpperCase();
  parts.push(key);
  return parts.join('+');
}

function matchesKeyBinding(event, action) {
  return canonicalKeyEvent(event).toLowerCase() === bindingFor(action).toLowerCase();
}

function renderKeyBindings(query = '') {
  // This settings UI has no approved prototype yet; late callbacks must stay inert.
  return;
  const list = document.getElementById('keyBindingList');
  const fixedList = document.getElementById('fixedKeyBindingList');
  if (!list || !fixedList) return;
  const normalizedQuery = query.trim().toLowerCase();
  const definitions = keyBindingDefinitions.filter(definition =>
    !normalizedQuery ||
    definition.title.toLowerCase().includes(normalizedQuery) ||
    definition.description.toLowerCase().includes(normalizedQuery) ||
    definition.binding.toLowerCase().includes(normalizedQuery)
  );
  list.replaceChildren();
  for (const definition of definitions) {
    const row = document.createElement('div');
    row.className = 'shortcut-row';
    const copy = document.createElement('div');
    copy.className = 'shortcut-copy';
    copy.innerHTML = `<div class="shortcut-title">${escapeHtml(definition.title)}</div>` +
      `<div class="shortcut-description">${escapeHtml(definition.description)}</div>`;
    const actions = document.createElement('div');
    actions.className = 'shortcut-actions';
    const binding = document.createElement('button');
    binding.type = 'button';
    binding.className = 'shortcut-binding';
    binding.textContent = capturingKeyBindingAction === definition.action
      ? 'Press keys…'
      : definition.binding;
    binding.classList.toggle('capturing', capturingKeyBindingAction === definition.action);
    binding.setAttribute('aria-label', `Change ${definition.title} shortcut`);
    binding.onclick = () => beginKeyBindingCapture(definition.action);
    const reset = document.createElement('button');
    reset.type = 'button';
    reset.className = 'shortcut-reset';
    reset.textContent = '↺';
    reset.title = 'Restore default';
    reset.disabled = definition.binding === definition.defaultBinding;
    reset.onclick = () => resetKeyBinding(definition.action);
    actions.append(binding, reset);
    row.append(copy, actions);
    list.appendChild(row);
  }
  if (!definitions.length) {
    const empty = document.createElement('div');
    empty.className = 'shortcut-empty';
    empty.textContent = 'No matching shortcuts';
    list.appendChild(empty);
  }

  fixedList.replaceChildren();
  for (const definition of FIXED_KEY_BINDINGS.filter(definition =>
    !normalizedQuery ||
    definition.title.toLowerCase().includes(normalizedQuery) ||
    definition.binding.toLowerCase().includes(normalizedQuery)
  )) {
    const row = document.createElement('div');
    row.className = 'shortcut-row';
    row.innerHTML = `<div class="shortcut-copy"><div class="shortcut-title">${escapeHtml(definition.title)}</div></div>` +
      `<span class="setting-value">${escapeHtml(definition.binding)}</span>`;
    fixedList.appendChild(row);
  }
}

function beginKeyBindingCapture(action) {
  // This settings UI has no approved prototype yet; late callbacks must stay inert.
  return;
  setKeyBindingError('');
  capturingKeyBindingAction = action;
  renderKeyBindings(document.getElementById('shortcutSearch')?.value || '');
}

async function saveCapturedKeyBinding(event) {
  if (!window.RozsaSettings.isSettingsPaneAvailable('keyboard-shortcuts')) return false;
  if (!capturingKeyBindingAction) return false;
  event.preventDefault();
  event.stopImmediatePropagation();
  if (event.key === 'Escape') {
    capturingKeyBindingAction = null;
    renderKeyBindings(document.getElementById('shortcutSearch')?.value || '');
    return true;
  }
  const binding = canonicalKeyEvent(event);
  if (!binding) return true;
  const action = capturingKeyBindingAction;
  capturingKeyBindingAction = null;
  try {
    keyBindingDefinitions = await invoke('update_key_binding', { action, binding });
    setKeyBindingError('');
  } catch (error) {
    setKeyBindingError(String(error));
    showError('Failed to update keyboard shortcut: ' + String(error));
  }
  renderKeyBindings(document.getElementById('shortcutSearch')?.value || '');
  return true;
}

async function resetKeyBinding(action) {
  // This settings UI has no approved prototype yet; late callbacks must stay inert.
  return;
  try {
    keyBindingDefinitions = await invoke('reset_key_binding', { action });
    setKeyBindingError('');
    renderKeyBindings(document.getElementById('shortcutSearch')?.value || '');
  } catch (error) {
    setKeyBindingError(String(error));
    showError('Failed to reset keyboard shortcut: ' + String(error));
  }
}

