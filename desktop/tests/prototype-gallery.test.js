import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'vitest';
import { getByRole, queryByRole } from '@testing-library/dom';
import { JSDOM } from 'jsdom';

test('gallery HTML loads its script and switches views and theme', () => {
  const entry = new URL('../../prototype/gallery/visual-direction.html', import.meta.url);
  const dom = new JSDOM(readFileSync(entry, 'utf8'), {
    url: 'http://localhost/prototype/gallery/visual-direction.html',
    runScripts: 'outside-only',
  });
  try {
    for (const script of dom.window.document.scripts) {
      const source = script.getAttribute('src');
      dom.window.eval(source ? readFileSync(new URL(source, entry), 'utf8') : script.textContent);
    }
    const scenes = [
      ['Empty', 'Make room for the next thought.'],
      ['Active', 'Refine the release flow.'],
      ['Appearance', 'A softer edge for serious work.'],
    ];
    for (const [label, heading] of scenes) {
      const button = getByRole(dom.window.document, 'button', { name: label });
      button.click();
      assert.equal(button.getAttribute('aria-selected'), 'true');
      assert.ok(getByRole(dom.window.document, 'heading', { name: heading }));
      for (const [, otherHeading] of scenes.filter(([otherLabel]) => otherLabel !== label)) {
        assert.equal(queryByRole(dom.window.document, 'heading', { name: otherHeading }), null);
      }
    }
    const toggle = getByRole(dom.window.document, 'button', { name: 'Switch to dark theme' });
    assert.equal(dom.window.document.documentElement.dataset.theme, 'light');
    toggle.click();
    assert.equal(dom.window.document.documentElement.dataset.theme, 'dark');
    assert.equal(toggle.getAttribute('aria-label'), 'Switch to light theme');
  } finally {
    dom.window.close();
  }
});
