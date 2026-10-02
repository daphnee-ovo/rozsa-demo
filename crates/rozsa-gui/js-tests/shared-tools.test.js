import test from 'node:test';
import assert from 'node:assert/strict';
import * as tools from '../frontend/ui/scripts/features/tools.js';
import { toolView, activityView } from '../frontend/runtime/adapters/tools.js';
import { createDOM, withDOM, loadApp } from './helpers/browser.js';

test('tool cards load the actual template, escape output and reject unsupported status', async () => {
  const dom = createDOM();
  try {
    await withDOM(dom, async () => {
      await tools.loadToolTemplate();
      dom.window.document.body.innerHTML = tools.toolMarkup('call"1', 'Bash', 'echo real', '', 's-success', {
        expanded: true, bodyHtml: tools.evidenceMarkup('Output', '<script>unsafe</script>\nreal stderr'),
      });
      const card = dom.window.document.querySelector('.tool-call');
      assert(card.classList.contains('expanded'));
      assert.equal(card.dataset.toolCallId, 'call"1');
      assert.equal(card.querySelector('script'), null);
      assert.equal(card.querySelector('.tool-evidence-output').textContent, '<script>unsafe</script>\nreal stderr');
      assert.equal(card.querySelector('[onclick]'), null);
      assert.throws(() => tools.toolMarkup('id', 'tool', '', '', 'cancelled'), TypeError);
    });
  } finally { dom.window.close(); }
});

test('diff line numbers follow both sides of a real hunk', () => {
  const dom = createDOM(tools.diffViewMarkup('@@ -10,2 +20,2 @@\n same\n-old\n+new'));
  try {
    const rows = Array.from(dom.window.document.querySelectorAll('.diff-line'));
    assert.deepEqual(rows.map(row => row.querySelector('.diff-ln').textContent), ['20', '11', '21']);
    assert.deepEqual(rows.map(row => row.querySelector('.diff-text').textContent), ['same', 'old', 'new']);
    assert.equal(rows[1].classList.contains('diff-del'), true);
    assert.equal(rows[2].classList.contains('diff-add'), true);
    assert.match(tools.codeViewMarkup('<tag>\nnext'), /&lt;tag&gt;/);
  } finally { dom.window.close(); }
});

test('plain identifiers retain underscores while unsupported Markdown remains empty', () => {
  for (const plain of ['plain\ntext\n\nnext', 'QA_PERMISSION_ALLOW', 'QA_PERMISSION_DENY', 'QA_PERMISSION_HINT',
    'some_identifier_name', 'some__identifier__name', '中文_标识符_名称', 'prefix_emphasis_suffix',
    'snake_case and another_identifier_name']) {
    assert.equal(tools.messageTextMarkup(plain), tools.textMarkup(plain), plain);
  }
  assert.match(tools.messageTextMarkup('```rust\n<tag>\n```'), /&lt;tag&gt;/);
  for (const missing of ['# Heading', '- List', '> Quote', '[link](https://example.com)', '[link][ref]',
    '| Table |', '**bold**', '*emphasis*', '_emphasis_', '__strong__', '___strong emphasis___',
    '_some_identifier_name_', 'before _emphasis_ after', '中文 _强调_ 内容', '(_emphasis_)', '<b>html</b>',
    'title\n===', '~~~\ncode\n~~~', '    indented', '`inline`', '```\nunclosed']) {
    assert.equal(tools.messageTextMarkup(missing), '<div data-ui-suspended="UI-MARKDOWN-001"></div>', missing);
  }
  assert.match(tools.messageTextMarkup('visible\n\n# Missing\n\nvisible too'), /<p>visible too<\/p>/);
});

test('runtime tools retain failures and pending questions rather than inventing success', () => {
  for (const name of ['Read', 'Bash']) {
    assert.equal(toolView({ name }, { output: 'actual output', isError: false, details: {} }).disposition, 'covered');
  }
  assert.equal(toolView({ name: 'Edit' }, { output: 'done', details: { file_deltas: [{ patch: '@@ -1 +1 @@\n-a\n+b' }] } }).kind, 'diff');
  for (const name of ['Read', 'Edit', 'Bash', 'Subagent', 'Generic', 'Write']) {
    assert.equal(toolView({ name }, { output: 'real failure', isError: true }).disposition, 'blocked');
    assert.equal(toolView({ name }, null).disposition, 'suspended');
  }
  assert.equal(toolView({ name: 'AskUserQuestion' }, null).disposition, 'blocked');
  assert.equal(toolView({ name: 'Bash' }, { output: 'nonzero', details: { exit_code: 1 } }).disposition, 'blocked');
  assert.equal(toolView({ name: 'Read' }, { output: 'truncated', details: { truncated: true } }).disposition, 'suspended');
  assert.equal(toolView({ name: 'Subagent' }, { output: 'done' }).disposition, 'suspended');
  assert.equal(toolView({ name: 'Read' }, { output: '' }).disposition, 'suspended');
});

const activity = { captureComplete: true,
  fileChanges: [{ path: 'src/<safe>.rs', status: 'modified', patch: '@@ -1 +1 @@\n-a\n+b', added: 1, deleted: 1 }],
  verification: { success: true, command: 'cargo test', exitCode: 0, durationMs: 42 } };

test('activity normalization preserves source data and rejects incomplete evidence', () => {
  const original = structuredClone(activity);
  const view = activityView(activity);
  assert.equal(view.disposition, 'covered');
  assert.equal(view.verification.duration, '42ms');
  assert.deepEqual(activity, original);
  const dom = createDOM(tools.changesMarkup(view.files, view.verification));
  try {
    assert.match(dom.window.document.body.textContent, /src\/<safe>\.rs/);
    assert(dom.window.document.querySelector('[data-turn-diff]'));
  } finally { dom.window.close(); }
  for (const missing of [{ ...activity, captureComplete: false },
    { ...activity, verification: { ...activity.verification, success: false } },
    { ...activity, fileChanges: [{ ...activity.fileChanges[0], status: 'deleted' }] }]) {
    assert.equal(activityView(missing).disposition, 'blocked');
  }
});

test('the complete runtime entry delegates covered evidence and exposes real verification failures', () => {
  const app = loadApp({ globals: { RozsaTools: { ...tools, activityView }, testActivity: activity } });
  try {
    const view = activityView(activity);
    assert.equal(app.evaluate('renderTurnActivityCard(testActivity)'), tools.changesMarkup(view.files, view.verification));
    const failure = app.evaluate('renderTurnActivityCard({...testActivity, verification:{...testActivity.verification,success:false}})');
    app.document.body.innerHTML = failure;
    assert.match(app.document.body.textContent, /Verification failed/);
    assert(app.document.querySelector('[data-ui-migration-blocked="UI-EVIDENCE-001"]'));
  } finally { app.close(); }
});

test('tool expansion is recorded on the originating session through real DOM', () => {
  const app = loadApp({ html: '<div id="card" data-session-id="session-a" data-tool-call-id="tool-a"></div>' });
  try {
    app.evaluate("activeSessionId='session-b'; toggleToolCall(document.getElementById('card'));");
    assert.equal(app.document.getElementById('card').classList.contains('expanded'), true);
    assert.equal(app.evaluate("JSON.stringify(expandedToolCallsBySession)"), '{"session-a":["tool-a"]}');
    app.evaluate("toggleToolCall(document.getElementById('card'))");
    assert.equal(app.document.getElementById('card').classList.contains('expanded'), false);
    assert.equal(app.evaluate("JSON.stringify(expandedToolCallsBySession)"), '{"session-a":[]}');
  } finally { app.close(); }
});
