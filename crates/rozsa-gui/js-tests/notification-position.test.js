import assert from 'node:assert/strict';
import test from 'node:test';
import { createSidebarFeedback } from '../frontend/runtime/adapters/notifications.js';

test('sidebar feedback forwards messages and preserves bridge failures', async () => {
  const events = [];
  await createSidebarFeedback({ emit: async (...args) => events.push(args) })('Invalid session header');
  assert.deepEqual(events, [['error', 'Invalid session header']]);
  const failure = new Error('bridge unavailable');
  await assert.rejects(createSidebarFeedback({ emit: async () => { throw failure; } })('failed'), error => error === failure);
});
