// @ts-nocheck
const QUESTION_OTHER_VALUE = '__rozsa_other__';

function configureInterruptionSuspensions() {
  const trust = document.querySelector('#permPanelActions [data-permission-action="trust"]');
  trust.disabled = true;
  trust.title = 'Unavailable until its UI design is complete';
  trust.dataset.uiSuspended = 'permission-trust';
  document.getElementById('permPanelTrust').dataset.uiSuspended = 'permission-trust';
}

function emitInterruptionState(type, kind) {
  document.dispatchEvent(new CustomEvent('rozsa:interruption-' + type, { detail: { kind } }));
}

function showUserQuestion(ev) {
  if (!ev || !ev.sessionId || !ev.requestId || !Array.isArray(ev.questions) || !ev.questions.length) return;
  const queue = pendingUserQuestions[ev.sessionId] || [];
  queue.push(ev);
  pendingUserQuestions[ev.sessionId] = queue;
  sessionStreamingState[ev.sessionId] = 'question';
  scheduleQuestionPanelDisplay();
}

function scheduleQuestionPanelDisplay() {
  void displayQuestionPanelIfNeeded();
}

async function displayQuestionPanelIfNeeded() {
  if (questionDisplayInFlight) return;
  questionDisplayInFlight = true;
  try {
    const sessionId = activeSessionId || (sessions[activeSessionIdx] && sessions[activeSessionIdx].id);
    const queue = sessionId ? pendingUserQuestions[sessionId] : null;
    const ev = queue && queue[0];
    if (!ev) {
      hideQuestionPanel();
      return;
    }
    if (currentQuestionId !== ev.requestId || currentQuestionSessionId !== ev.sessionId) {
      currentQuestionId = ev.requestId;
      currentQuestionSessionId = ev.sessionId;
      currentQuestionIndex = 0;
      currentQuestionAnswers = {};
    }
    renderQuestionPage(ev);
    const panel = document.getElementById('questionPanel');
    if (!panel) return;
    panel.classList.add('visible');
    emitInterruptionState('open', 'question');
    const input = document.getElementById('msgInput');
    if (input) input.style.display = 'none';
    panel.querySelector('#questionPanelOptions input')?.focus();
  } finally {
    questionDisplayInFlight = false;
  }
}

function renderQuestionPage(ev) {
  const question = ev.questions[currentQuestionIndex];
  if (!question) return;
  const title = document.getElementById('questionPanelTitle');
  const options = document.getElementById('questionPanelOptions');
  const otherInput = document.getElementById('questionPanelOtherInput');
  const error = document.getElementById('questionPanelError');
  const submit = document.getElementById('questionPanelSubmit');
  if (!title || !options || !otherInput || !error || !submit) return;

  const panel = document.getElementById('questionPanel');
  const covered = ev.questions.length === 1 && !question.multiSelect;
  panel.dataset.interruptionUi = covered ? 'shared' : 'legacy';
  if (!submit.dataset.sharedMarkup) submit.dataset.sharedMarkup = submit.innerHTML;
  if (covered) {
    title.textContent = '[1/1] ' + (question.question || '');
    title.title = question.question || '';
    options.innerHTML = window.RozsaPermissions.questionOptionsMarkup([
      ...(question.options || []).map(option => ({ ...option, value: option.label || '' })),
      { label: 'Other', description: 'Type a custom answer.', value: QUESTION_OTHER_VALUE, other: true },
    ], 'question-' + ev.requestId);
    otherInput.value = '';
    otherInput.hidden = true;
    error.textContent = '';
    window.RozsaPermissions.bindQuestionOptions(options, otherInput, error, { selectOtherOnInput: true });
    submit.innerHTML = submit.dataset.sharedMarkup;
    submit.setAttribute('aria-label', 'Done (D)');
    submit.disabled = false;
    options.querySelector('input')?.focus();
    return;
  }

  const questionText = question.question || '';
  title.textContent = '[' + (currentQuestionIndex + 1) + '/' + ev.questions.length + '] ' + questionText;
  title.title = questionText;
  error.textContent = '';
  options.replaceChildren();
  const inputType = question.multiSelect ? 'checkbox' : 'radio';
  const name = 'question-' + ev.requestId + '-' + currentQuestionIndex;
  const addOption = (labelText, description, value, isOther = false, optionNumber) => {
    const row = document.createElement('label');
    row.className = 'question-panel-option';
    const input = document.createElement('input');
    input.type = inputType;
    input.name = name;
    input.value = value;
    input.dataset.other = isOther ? 'true' : 'false';
    input.dataset.optionNumber = String(optionNumber);
    const key = document.createElement('span');
    key.className = 'question-panel-option-key';
    key.textContent = String(optionNumber);
    const copy = document.createElement('span');
    copy.className = 'question-panel-option-copy';
    const label = document.createElement('span');
    label.className = 'question-panel-option-label';
    label.textContent = labelText;
    copy.appendChild(label);
    if (description) {
      const detail = document.createElement('span');
      detail.className = 'question-panel-option-description';
      detail.textContent = description;
      copy.appendChild(detail);
    }
    row.append(key, input, copy);
    input.addEventListener('change', () => {
      if (isOther) {
        otherInput.hidden = !input.checked;
        if (input.checked) otherInput.focus();
      } else if (!question.multiSelect) {
        otherInput.hidden = true;
        otherInput.value = '';
      }
      error.textContent = '';
    });
    options.appendChild(row);
    return input;
  };

  (Array.isArray(question.options) ? question.options : []).forEach((option, index) => {
    addOption(option.label || 'Option', option.description || '', option.label || '', false, index + 1);
  });
  const other = addOption(
    'Other',
    'Type a custom answer.',
    QUESTION_OTHER_VALUE,
    true,
    (Array.isArray(question.options) ? question.options.length : 0) + 1,
  );
  otherInput.value = '';
  otherInput.hidden = true;
  otherInput.oninput = () => {
    if (otherInput.value.trim()) {
      other.checked = true;
      otherInput.hidden = false;
    }
    error.textContent = '';
  };
  const isLastQuestion = currentQuestionIndex + 1 >= ev.questions.length;
  submit.replaceChildren();
  const submitKey = document.createElement('span');
  submitKey.className = 'question-panel-submit-key';
  submitKey.textContent = isLastQuestion ? 'D' : 'N';
  const submitLabel = document.createElement('span');
  submitLabel.textContent = isLastQuestion ? 'Done' : 'Next';
  submit.append(submitKey, submitLabel);
  submit.setAttribute('aria-label', isLastQuestion ? 'Done (D)' : 'Next (N)');
  submit.disabled = false;
  const first = options.querySelector('input');
  if (first) first.focus();
}

function activeQuestionEvent() {
  const queue = currentQuestionSessionId
    ? pendingUserQuestions[currentQuestionSessionId] || []
    : [];
  return queue[0] || null;
}

function selectQuestionOption(number) {
  const option = document.querySelector(
    '#questionPanelOptions input[data-option-number="' + number + '"]',
  );
  if (!option) return false;
  option.checked = option.type === 'checkbox' ? !option.checked : true;
  option.dispatchEvent(new Event('change', { bubbles: true }));
  return true;
}

function clearQuestionOtherInput() {
  const otherInput = document.getElementById('questionPanelOtherInput');
  const other = document.querySelector('#questionPanelOptions input[data-other="true"]');
  if (other) {
    other.checked = false;
    other.focus();
  }
  if (otherInput) {
    otherInput.value = '';
    otherInput.hidden = true;
  }
  const error = document.getElementById('questionPanelError');
  if (error) error.textContent = '';
}

function collectQuestionAnswer(question) {
  const options = document.getElementById('questionPanelOptions');
  const otherInput = document.getElementById('questionPanelOtherInput');
  const error = document.getElementById('questionPanelError');
  if (!options || !otherInput || !error) return null;
  if (document.getElementById('questionPanel').dataset.interruptionUi === 'shared') {
    return window.RozsaPermissions.collectSingleQuestionAnswer(options, otherInput, error, {
      missing: 'Select an option or choose Other.',
      otherMissing: 'Type your answer in the Other field.',
    });
  }
  const checked = Array.from(options.querySelectorAll('input:checked'));
  const values = [];
  let otherSelected = false;
  for (const input of checked) {
    if (input.dataset.other === 'true') {
      otherSelected = true;
    } else {
      values.push(input.value);
    }
  }
  if (otherSelected) {
    const custom = otherInput.value.trim();
    if (!custom) {
      error.textContent = 'Type your answer in the Other field.';
      otherInput.focus();
      return null;
    }
    values.push(custom);
  }
  if (!values.length) {
    error.textContent = 'Select an option or choose Other.';
    return null;
  }
  return question.multiSelect ? values : values[0];
}

async function submitUserQuestion() {
  if (!currentQuestionId || !currentQuestionSessionId) return;
  const queue = pendingUserQuestions[currentQuestionSessionId] || [];
  const ev = queue[0];
  const question = ev && ev.questions[currentQuestionIndex];
  if (!ev || !question) return;
  const answer = collectQuestionAnswer(question);
  if (answer === null) return;
  currentQuestionAnswers[question.header] = answer;
  if (currentQuestionIndex + 1 < ev.questions.length) {
    currentQuestionIndex += 1;
    renderQuestionPage(ev);
    return;
  }

  const submit = document.getElementById('questionPanelSubmit');
  if (submit) submit.disabled = true;
  try {
    await invoke('respond_user_question', {
      sessionId: currentQuestionSessionId,
      id: currentQuestionId,
      answers: currentQuestionAnswers,
    });
  } catch (e) {
    if (submit) submit.disabled = false;
    showError('User question response failed: ' + String(e));
    return;
  }
  pendingUserQuestions[currentQuestionSessionId] = queue.filter(item => item.requestId !== currentQuestionId);
  if (!pendingUserQuestions[currentQuestionSessionId].length) {
    delete pendingUserQuestions[currentQuestionSessionId];
    sessionStreamingState[currentQuestionSessionId] = 'running';
  }
  hideQuestionPanel();
  scheduleQuestionPanelDisplay();
}

function hideQuestionPanel() {
  const panel = document.getElementById('questionPanel');
  if (panel) panel.classList.remove('visible');
  const input = document.getElementById('msgInput');
  if (input) { input.style.display = ''; input.focus(); }
  currentQuestionId = null;
  currentQuestionSessionId = null;
  currentQuestionIndex = 0;
  currentQuestionAnswers = {};
  emitInterruptionState('close', 'question');
}

function discardCurrentQuestionUi() {
  if (!currentQuestionSessionId) return;
  delete pendingUserQuestions[currentQuestionSessionId];
  hideQuestionPanel();
}

// =============== Permissions ===============

