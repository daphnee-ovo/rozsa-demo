const escapeText = value => String(value ?? '').replace(/[&<>'"]/g, char => ({'&':'&amp;', '<':'&lt;', '>':'&gt;', "'":'&#39;', '"':'&quot;'}[char]));

export function questionOptionsMarkup(options, name) {
  return options.map(({label, description, other, value}, index) => '<label class="question-panel-option"><span class="question-panel-option-key">' + (index + 1) +
    '</span><input type="radio" name="' + escapeText(name) + '" value="' + escapeText(value ?? label) + '" data-option-number="' + (index + 1) + '" data-other="' + String(Boolean(other)) + '"><span class="question-panel-option-copy"><span class="question-panel-option-label">' +
    escapeText(label) + '</span><span class="question-panel-option-description">' + escapeText(description) + '</span></span></label>').join('');
}

export function bindQuestionOptions(options, otherInput, error, {selectOtherOnInput = false} = {}) {
  options.querySelectorAll('input').forEach(input => input.addEventListener('change', () => {
    const isOther = input.dataset.other === 'true';
    otherInput.hidden = !(isOther && input.checked);
    if (otherInput.hidden) otherInput.value = '';
    error.textContent = '';
    if (!otherInput.hidden) otherInput.focus();
  }));
  otherInput.oninput = () => {
    if (selectOtherOnInput && otherInput.value.trim()) {
      options.querySelector('input[data-other="true"]').checked = true;
      otherInput.hidden = false;
    }
    error.textContent = '';
  };
}

export function collectSingleQuestionAnswer(options, otherInput, error, messages) {
  const selected = options.querySelector('input:checked');
  if (!selected) { error.textContent = messages.missing; return null; }
  if (selected.dataset.other !== 'true') return selected.value;
  const value = otherInput.value.trim();
  if (!value) { error.textContent = messages.otherMissing; otherInput.focus(); return null; }
  return value;
}

export function permissionCommandMarkup(command, tool) {
  if (String(tool).toLowerCase() !== 'bash') return escapeText(command);
  return '<span class="perm-syn-prompt">$ </span><span class="perm-syn-command">' +
    escapeText(command.split(/\s+/)[0]) + '</span>' + escapeText(command.slice(command.indexOf(' ') + 1));
}
