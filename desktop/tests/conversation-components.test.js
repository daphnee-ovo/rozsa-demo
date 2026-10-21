import { test } from 'vitest';
import assert from 'node:assert/strict';
import { getByRole, getByText, queryByText } from '@testing-library/dom';
import { init, emptyConversationMarkup, messageFrame, runningPanelMarkup, thinkingMarkup } from '../renderer/scripts/features/conversation.ts';
import { createDOM, withDOM } from './helpers/browser.js';

test('conversation content remains readable while continuation omits repeated identity', () => {
  const dom = createDOM();
  const { document } = dom.window;
  document.body.innerHTML = emptyConversationMarkup();
  getByText(document, 'Start a new conversation');

  document.body.innerHTML = messageFrame('user', '<p>Hello</p>', 3);
  getByText(document, 'Hello');
  getByText(document, 'U');

  document.body.innerHTML = messageFrame('assistant', '<p>World</p>', 4, true);
  getByText(document, 'World');
  assert.equal(queryByText(document, 'R'), null);
  assert.throws(() => messageFrame('system', 'hidden', 5), { name: 'TypeError' });

  document.body.innerHTML = runningPanelMarkup({ title: '<unsafe>', count: 2, items: ['A & B'] });
  assert.match(document.body.textContent, /<unsafe>/);
  assert.equal(getByRole(document, 'listitem').textContent, 'A & B');
  assert.equal(runningPanelMarkup(null), '');

  document.body.innerHTML = thinkingMarkup('<p>Reason</p>', { expanded: false, active: true, duration: '2s' });
  const thinking = getByRole(document, 'button', { name: /THINKING/ });
  assert.equal(thinking.getAttribute('aria-expanded'), 'false');
  getByText(document, 'Reason');
  getByText(document, '2s');
  dom.window.close();
});
test('conversation component binds thinking click and keyboard behavior once', async () => {
  const dom = createDOM('<div id="chatMessages"><div class="thinking-header" role="button" tabindex="0" aria-expanded="false"></div></div>');
  try {
    await withDOM(dom, () => {
      const chat = document.getElementById('chatMessages');
      const header = chat.querySelector('.thinking-header');
      const api = {
        boundModules: new Set(), byId: id => document.getElementById(id), renderMessages() {},
        toggleThinking: element => element.setAttribute('aria-expanded', String(element.getAttribute('aria-expanded') !== 'true')),
      };
      init(api); init(api);
      assert.equal(getByRole(chat, 'button').getAttribute('aria-expanded'), 'false');
      header.dispatchEvent(new dom.window.MouseEvent('click', { bubbles: true }));
      assert.equal(getByRole(chat, 'button').getAttribute('aria-expanded'), 'true');
      const enter = new dom.window.KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true });
      header.dispatchEvent(enter);
      assert.equal(getByRole(chat, 'button').getAttribute('aria-expanded'), 'false');
      const space = new dom.window.KeyboardEvent('keydown', { key: ' ', bubbles: true, cancelable: true });
      header.dispatchEvent(space);
      header.dispatchEvent(new dom.window.KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
      assert.equal(getByRole(chat, 'button').getAttribute('aria-expanded'), 'true');
      assert.equal(enter.defaultPrevented, true);
      assert.equal(space.defaultPrevented, true);
    });
  } finally { dom.window.close(); }
});
