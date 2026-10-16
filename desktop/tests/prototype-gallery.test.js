import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
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
    for (const scene of ['empty', 'active', 'settings']) {
      dom.window.document.querySelector(`.scene-tab[data-scene="${scene}"]`).click();
      assert.equal(dom.window.document.querySelector('.scene-tab[aria-selected="true"]').dataset.scene, scene);
      assert.deepEqual([...dom.window.document.querySelectorAll('[data-scene-panel]')]
        .filter(panel => !panel.hidden).map(panel => panel.dataset.scenePanel), [scene]);
    }
    const toggle = dom.window.document.getElementById('themeToggle');
    assert.equal(dom.window.document.documentElement.dataset.theme, 'light');
    toggle.click();
    assert.equal(dom.window.document.documentElement.dataset.theme, 'dark');
    assert.equal(toggle.getAttribute('aria-label'), 'Switch to light theme');
  } finally {
    dom.window.close();
  }
});
