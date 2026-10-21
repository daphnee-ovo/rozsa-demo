import { test } from 'vitest';
import assert from 'node:assert/strict';
import { getAllByText } from '@testing-library/dom';
import { act } from 'react';
import { createConversationRenderer } from '../renderer/scripts/features/conversation-view.tsx';
import { loadApp, withDOM } from './helpers/browser.js';

globalThis.IS_REACT_ACT_ENVIRONMENT = true;

test('consecutive assistant messages omit repeated identity', async () => {
  const app = loadApp({ html: '<div id="chatMessages"></div>' });
  try {
    await withDOM(app.dom, async () => {
      const chat = app.document.getElementById('chatMessages');
      Object.defineProperties(chat, {
        clientHeight: { configurable: true, value: 720 },
        clientWidth: { configurable: true, value: 960 },
        getBoundingClientRect: { configurable: true, value: () => ({ top: 0, left: 0, right: 960, bottom: 720, width: 960, height: 720, x: 0, y: 0, toJSON() {} }) },
      });
      app.window.__messages = [
        { kind: 'standard', message: { role: 'assistant' } },
        { kind: 'standard', message: { role: 'assistant' } },
        { kind: 'standard', message: { role: 'user' } },
        { kind: 'standard', message: { role: 'assistant' } },
      ];
      const renderer = createConversationRenderer(chat);
      try {
        await act(async () => renderer.render({ empty: false, markdown: text => text, entries: [
          { id: 'first', kind: 'standard', messageIndex: 0, message: { role: 'assistant', content: [{ type: 'text', text: 'done' }] } },
          { id: 'continued', kind: 'standard', messageIndex: 1, continuation: true, message: { role: 'assistant', content: [{ type: 'text', text: 'done' }] } },
        ] }));
        assert.equal(getAllByText(chat, 'done').length, 2);
        assert.equal(getAllByText(chat, 'R').length, 1, 'the assistant identity appears once for adjacent messages');
      } finally { await act(async () => renderer.dispose()); }
    });
  } finally { app.close(); }
});
