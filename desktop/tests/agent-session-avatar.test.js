import test from 'node:test';
import assert from 'node:assert/strict';
import { act } from 'react';
import { createConversationRenderer } from '../renderer/scripts/features/conversation-view.tsx';
import { loadApp, readFrontend, withDOM } from './helpers/browser.js';

globalThis.IS_REACT_ACT_ENVIRONMENT = true;

test('consecutive assistant messages omit repeated identity', async () => {
  const app = loadApp({ html: '<div id="chatMessages"></div>' });
  try {
    await withDOM(app.dom, async () => {
      app.window.__messages = [
        { kind: 'standard', message: { role: 'assistant' } },
        { kind: 'standard', message: { role: 'assistant' } },
        { kind: 'standard', message: { role: 'user' } },
        { kind: 'standard', message: { role: 'assistant' } },
      ];
      assert.equal(app.evaluate('assistantMessageGroupPosition(window.__messages, 0)'), 'start');
      assert.equal(app.evaluate('assistantMessageGroupPosition(window.__messages, 1)'), 'continuation');
      assert.equal(app.evaluate('assistantMessageGroupPosition(window.__messages, 2)'), 'standalone');
      assert.equal(app.evaluate('assistantMessageGroupPosition(window.__messages, 3)'), 'start');
      const renderer = createConversationRenderer(app.document.getElementById('chatMessages'));
      try {
        await act(async () => renderer.render({ empty: false, markdown: text => text, entries: [
          { id: 'first', kind: 'standard', messageIndex: 0, message: { role: 'assistant', content: [{ type: 'text', text: 'done' }] } },
          { id: 'continued', kind: 'standard', messageIndex: 1, continuation: true, message: { role: 'assistant', content: [{ type: 'text', text: 'done' }] } },
        ] }));
        const rows = app.document.querySelectorAll('#chatMessages > .msg');
        assert.equal(rows.length, 2);
        assert.equal(rows[0].querySelectorAll('.msg-avatar').length, 1, 'group start renders one avatar');
        assert.ok(rows[0].querySelector('.msg-role'), 'group start renders role');
        assert.equal(rows[1].querySelector('.msg-avatar'), null, 'continuation omits avatar');
        assert.equal(rows[1].querySelector('.msg-role'), null, 'continuation omits role');
        assert.ok(rows[1].className.includes('msg-assistant-continuation'), 'continuation uses group class');
      } finally { await act(async () => renderer.dispose()); }
    });
  } finally { app.close(); }
});

test('continuation messages remain one visual assistant group', () => {
  const css = readFrontend('styles/features/conversation.css');
  const continuation = css.split('.msg-assistant-continuation {')[1]?.split('}')[0];
  assert.ok(continuation, 'continuation style should exist');
  assert.ok(!continuation.includes('padding-left'));
  assert.ok(css.includes('.msg-avatar { display: none; }'));
  assert.ok(continuation.includes('margin-top: -12px'));
});
