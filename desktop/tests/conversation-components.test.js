import test from 'node:test';
import assert from 'node:assert/strict';
import { init, emptyConversationMarkup, messageFrame, createMessageFrame, runningPanelMarkup, thinkingMarkup } from '../renderer/scripts/features/conversation.ts';
import { createConversationFlow } from '../renderer/scripts/flows/conversation.ts';
import { createDOM, withDOM } from './helpers/browser.js';

test('conversation renderers preserve message, continuation, empty, running and thinking markup', () => {
  assert.equal(emptyConversationMarkup().includes('Start a new conversation'), true);
  const user = messageFrame('user', '<p>Hello</p>', 3);
  assert.match(user, /class="msg msg-user" data-od-id="message-3"/);
  assert.match(user, /<div class="msg-avatar">U<\/div>/);
  assert.match(user, /<div class="msg-role">You<\/div>/);

  const continuation = messageFrame('assistant', '<p>World</p>', 4, true);
  assert.match(continuation, /msg-assistant-continuation/);
  assert.doesNotMatch(continuation, /msg-avatar|msg-role/);
  assert.throws(() => messageFrame('system', 'hidden', 5), { name: 'TypeError' });

  const running = runningPanelMarkup({ title: '<unsafe>', count: 2, items: ['A & B'] });
  assert.match(running, /&lt;unsafe&gt;/);
  assert.match(running, /A &amp; B/);
  assert.equal(runningPanelMarkup(null), '');
  const thinking = thinkingMarkup('<p>Reason</p>', { expanded: false, active: true, duration: '2s' });
  assert.match(thinking, /aria-expanded="false"/);
  assert.match(thinking, /data-stream-cursor-target="thinking"/);
  assert.match(thinking, /THINKING/);
  assert.match(thinking, /thinking-duration/);
});

test('conversation component binds thinking click and keyboard behavior once', async () => {
  const dom = createDOM('<div id="chatMessages"><div class="thinking-header" role="button" tabindex="0"></div></div>');
  try {
    await withDOM(dom, () => {
      const chat = document.getElementById('chatMessages');
      const header = chat.querySelector('.thinking-header');
      const calls = [];
      const api = { boundModules: new Set(), byId: id => document.getElementById(id), renderMessages: () => calls.push('render'), toggleThinking: element => calls.push(element) };
      init(api); init(api);
      header.dispatchEvent(new dom.window.MouseEvent('click', { bubbles: true }));
      const enter = new dom.window.KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true });
      header.dispatchEvent(enter);
      const space = new dom.window.KeyboardEvent('keydown', { key: ' ', bubbles: true, cancelable: true });
      header.dispatchEvent(space);
      header.dispatchEvent(new dom.window.KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
      assert.equal(calls.length, 4);
      assert.equal(calls[0], 'render');
      assert.equal(calls[1], header);
      assert.equal(calls[2], header);
      assert.equal(calls[3], header);
      assert.equal(enter.defaultPrevented, true);
      assert.equal(space.defaultPrevented, true);
    });
  } finally { dom.window.close(); }
});

test('conversation flow forwards the existing UI actions to its adapter', () => {
  const calls = [];
  const adapter = Object.fromEntries(['attachFile', 'attachDirectory', 'slashCommand', 'chooseModel', 'submit', 'refreshSendState']
    .map(name => [name, () => calls.push(name)]));
  const flow = createConversationFlow(adapter);
  flow.attachFileReference(); flow.attachDirectoryReference(); flow.insertSlashCommandPrefix();
  flow.showModelPicker(); flow.addMessage(); flow.updateAbortButton();
  assert.deepEqual(calls, ['attachFile', 'attachDirectory', 'slashCommand', 'chooseModel', 'submit', 'refreshSendState']);
});

test('the native conversation entry loads the TypeScript component and flow sources', async () => {
  const source = await import('../renderer/scripts/features/conversation.ts');
  const flow = await import('../renderer/scripts/flows/conversation.ts');
  assert.equal(typeof source.init, 'function');
  assert.equal(typeof source.messageFrame, 'function');
  assert.equal(typeof flow.mountConversation, 'function');
  assert.equal(typeof flow.createConversationFlow, 'function');
});
