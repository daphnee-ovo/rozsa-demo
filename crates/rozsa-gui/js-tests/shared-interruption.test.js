import assert from 'node:assert/strict';
import { test } from 'node:test';
import { loadApp, readFrontend } from './helpers/browser.js';
import { permissionCommandMarkup } from '../frontend/ui/scripts/features/permissions.js';
import { JSDOM } from 'jsdom';

test('closing a visible interruption restores the composer in both prototype scenes', () => {
  for (const [scene, panelClass] of [['ask-user-question', 'question-panel'], ['permission-request', 'perm-panel']]) {
    const dom = new JSDOM(`<html data-rozsa-scene="${scene}"><head><style>${readFrontend('ui/styles/composites/permission-question.css')}</style></head><body><div data-od-id="chat-input"><div class="${panelClass} visible"></div><div class="input-wrapper"><textarea></textarea></div></div></body></html>`);
    try {
      const composer = dom.window.document.querySelector('.input-wrapper');
      assert.equal(dom.window.getComputedStyle(composer).display, 'none');
      dom.window.document.querySelector(`.${panelClass}`).classList.remove('visible');
      assert.notEqual(dom.window.getComputedStyle(composer).display, 'none', scene);
    } finally { dom.window.close(); }
  }
});

test('permission command preview preserves the exact requested command text', () => {
  const dom = new JSDOM('<div id="preview"></div>');
  try {
    const preview = dom.window.document.getElementById('preview');
    for (const command of ["printf 'ROZSA_PERMISSION_ALLOW\\n'", 'pwd', 'git  status', 'git\tstatus', 'git\nstatus', '  git status', 'printf "<unsafe>&"', '']) {
      preview.innerHTML = permissionCommandMarkup(command, 'bash');
      assert.equal(preview.textContent, '$ ' + command, JSON.stringify(command));
      assert.equal(preview.querySelector('.perm-syn-command').textContent, command.match(/\S+/)?.[0] ?? '');
      assert.equal(preview.querySelector('unsafe'), null);
    }
    preview.innerHTML = permissionCommandMarkup('<unsafe>&', 'read');
    assert.equal(preview.textContent, '<unsafe>&');
    assert.equal(preview.querySelector('unsafe'), null);
  } finally { dom.window.close(); }
});

test('question validation, binding and dispatch preserve user input', async () => {
  const feature = await import('../frontend/ui/scripts/features/permissions.js');
  const adapterModule = await import('../frontend/runtime/adapters/interruption.js');
  const component = await import('../frontend/ui/scripts/components/interruption-panel.js');
  const { JSDOM } = await import('jsdom');
  const dom = new JSDOM('<main data-od-id="chat-input"><input id="permHintInput"></main>');
  globalThis.document = dom.window.document;
  const panel = document.querySelector('[data-od-id="chat-input"]');
  const choices = [false, true].map(otherChoice => {
    const input = document.createElement('input');
    input.dataset.other = String(otherChoice);
    input.value = otherChoice ? 'other' : 'a';
    panel.append(input);
    return input;
  });
  const options = { querySelectorAll: () => choices, querySelector: selector => selector === 'input:checked' ? choices.find(choice => choice.checked) : choices[1] };
  const other = document.createElement('input');
  other.hidden = true;
  panel.append(other);
  const error = document.createElement('div');
  panel.append(error);
  const messages = { missing: 'Choose', otherMissing: 'Type' };
  feature.bindQuestionOptions(options, other, error, { selectOtherOnInput: true });
  assert.equal(feature.collectSingleQuestionAnswer(options, other, error, messages), null);
  assert.equal(error.textContent, 'Choose');
  choices[1].checked = true;
  choices[1].dispatchEvent(new dom.window.Event('change'));
  assert.equal(other.hidden, false);
  assert.equal(document.activeElement, other);
  assert.equal(feature.collectSingleQuestionAnswer(options, other, error, messages), null);
  assert.equal(error.textContent, 'Type');
  other.value = '  custom answer  ';
  other.dispatchEvent(new dom.window.Event('input'));
  assert.equal(feature.collectSingleQuestionAnswer(options, other, error, messages), 'custom answer');
  choices[1].checked = false;
  choices[0].checked = true;
  choices[0].dispatchEvent(new dom.window.Event('change'));
  assert.equal(other.hidden, true);
  assert.equal(other.value, '');
  assert.equal(error.textContent, '');
  assert.equal(feature.collectSingleQuestionAnswer(options, other, error, messages), 'a');

  const calls = [];
  const handlers = {
    respondPermission: choice => calls.push(choice), enterPermissionTrust: () => calls.push('trust'), enterPermissionHint: () => calls.push('hint'),
    togglePermissionCommand() {}, showPermissionMainPage() {}, submitPermissionHint: () => calls.push('submit-hint'),
    submitUserQuestion: () => calls.push('question'), normalizePermissionHint: value => calls.push(value), handlePermissionHintKeydown: () => calls.push('key'),
  };
  const api = { ...adapterModule.createInterruptionAdapter(handlers), boundModules: new Set(), byId: id => document.getElementById(id) };
  component.init(api);
  component.init(api);
  for (const action of ['allow', 'deny', 'hint', 'submit-hint', 'submit-question']) {
    const button = document.createElement('button');
    button.dataset.permissionAction = action;
    panel.append(button);
    button.dispatchEvent(new dom.window.MouseEvent('click', { bubbles: true }));
  }
  const hintInput = document.getElementById('permHintInput');
  hintInput.value = 'hint';
  hintInput.dispatchEvent(new dom.window.Event('input'));
  hintInput.dispatchEvent(new dom.window.KeyboardEvent('keydown'));
  assert.deepEqual(calls.map(value => value === hintInput ? 'input' : value), ['allow', 'deny', 'hint', 'submit-hint', 'question', 'input', 'key']);
  dom.window.close();
  delete globalThis.document;
});

test('real permission responses retain queues after failure and advance only the active session', async () => {
  const calls = [];
  const errors = [];
  const events = [];
  const responseState = { fail: false };
  const app = await loadApp({
    html: '<section id="permPanel"><div id="permPanelActions"><button data-permission-action="trust"></button></div><div id="permPanelContext"></div><div id="permPanelMain"></div><div id="permPanelTrust"></div><div id="permPanelHint"></div></section><section id="questionPanel"><button id="questionPanelSubmit"></button></section>',
    globals: {
      console: { error() {} }, structuredClone, testErrors: errors, responseState,
      testInvoke: async (command, payload) => { calls.push([command, structuredClone(payload)]); if (responseState.fail) throw Error('rejected'); },
    },
  });
  const { document, evaluate } = app;
  document.addEventListener('rozsa:interruption-close', event => events.push(event));
  const runtime = code => evaluate(code);
  runtime(`invoke=testInvoke;showError=message=>testErrors.push(message);
    pendingPermissions={a:[{session_id:'a',request_id:'p1'},{session_id:'a',request_id:'p2'}],b:[{session_id:'b',request_id:'p1'}]};
    pendingUserQuestions={a:[{sessionId:'a',requestId:'q1',questions:[{header:'Choice'}]}],b:[{sessionId:'b',requestId:'q1'}]};
    currentPermissionId='p1';currentPermissionSessionId='a';currentPermissionTrustGroups=[];currentPermissionTrustKeys=[];currentPermissionTrustIndex=-1;
    currentQuestionId='q1';currentQuestionSessionId='a';currentQuestionIndex=0;currentQuestionAnswers={};
    sessionStreamingState={};permissionUiStateBySession={a:{requestId:'p1'}};
    schedulePermPanelDisplay=()=>{};scheduleQuestionPanelDisplay=()=>{};configureInterruptionSuspensions();`);
  const trust = document.querySelector('#permPanelActions [data-permission-action="trust"]');
  assert.equal(trust.dataset.uiSuspended, 'permission-trust');
  assert.equal(trust.disabled, true);
  runtime("enterPermissionTrust();choosePermissionTrust('key');renderPermissionTrustPage();");
  assert.equal(calls.length, 0, 'trust entry must never auto-allow, including empty trust groups');
  runtime('responseState.fail=true;');
  await runtime("respondPermission('deny-hint','reason');");
  assert.equal(runtime('pendingPermissions.a.length'), 2);
  assert(runtime('permissionUiStateBySession.a'));
  assert.equal(errors.length, 1);
  assert.equal(document.getElementById('permPanel').dataset.interruptionUi, 'legacy');
  assert.deepEqual(calls[0], ['respond_permission', { sessionId: 'a', id: 'p1', choice: 'deny-hint', trustKey: null, trustKeys: null, hint: 'reason' }]);
  runtime('responseState.fail=false;');
  await runtime("respondPermission('allow');");
  assert.equal(runtime('pendingPermissions.a.length'), 1);
  assert.equal(runtime('pendingPermissions.a[0].request_id'), 'p2');
  assert.equal(runtime('pendingPermissions.a[0].needsRecheck'), true);
  assert.equal(runtime('pendingPermissions.b.length'), 1);
  assert.equal(runtime('currentPermissionId'), null);
  assert(events.some(event => event.type === 'rozsa:interruption-close'));
  runtime("collectQuestionAnswer=()=> 'custom';responseState.fail=true;");
  await runtime('submitUserQuestion();');
  assert.equal(runtime('pendingUserQuestions.a.length'), 1);
  assert.equal(document.getElementById('questionPanelSubmit').disabled, false);
  runtime('responseState.fail=false;');
  await runtime('submitUserQuestion();');
  assert.equal(runtime('pendingUserQuestions.a'), undefined);
  assert.equal(runtime('pendingUserQuestions.b.length'), 1);
  assert.deepEqual(calls.at(-1), ['respond_user_question', { sessionId: 'a', id: 'q1', answers: { Choice: 'custom' } }]);
  assert.equal(runtime('currentQuestionId'), null);
  app.close();
});
