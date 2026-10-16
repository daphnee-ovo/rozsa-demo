import test from 'node:test';
import assert from 'node:assert/strict';
import { frontendRoot, loadApp, readFrontend, withDOM } from './helpers/browser.js';

test('consecutive assistant messages omit repeated identity', async () => {
  const app = loadApp({ html: '<div id="chatMessages"></div>' });
  try {
    await withDOM(app.dom, async () => {
      const conversation = await import(new URL('scripts/features/conversation.js', frontendRoot));
      app.window.RozsaConversation = conversation;
      app.window.RozsaTools = { messageTextMarkup: text => String(text) };
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
      const initial = app.evaluate("renderMessage({kind:'standard',message:{role:'assistant',content:[{type:'text',text:'done'}]}},{},false,null,null,false,true)");
      const continuation = app.evaluate("renderMessage({kind:'standard',message:{role:'assistant',content:[{type:'text',text:'done'}]}},{},false,null,null,false,false)");
      assert.equal((initial.innerHTML.match(/msg-avatar/g) || []).length, 1, 'group start renders one avatar');
      assert.ok(initial.innerHTML.includes('msg-role'), 'group start renders role');
      assert.ok(!continuation.innerHTML.includes('msg-avatar'), 'continuation omits avatar');
      assert.ok(!continuation.innerHTML.includes('msg-role'), 'continuation omits role');
      assert.ok(continuation.className.includes('msg-assistant-continuation'), 'continuation uses group class');
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
