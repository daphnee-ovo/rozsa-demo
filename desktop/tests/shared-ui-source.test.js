import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { frontendRoot, repositoryRoot, readFrontend, createDOM, withDOM } from './helpers/browser.js';
import { loadTemplate } from '../renderer/scripts/core/template-loader.ts';

function stylesheets(entry, seen = new Set()) {
  const url = new URL(entry, frontendRoot);
  if (seen.has(url.href)) return [];
  seen.add(url.href);
  const text = readFileSync(url, 'utf8');
  const imports = [...text.matchAll(/@import\s+url\(["']([^"']+)["']\);/g)];
  return [{ url, rules: text.replace(/@import[^;]+;/g, '').replace(/\/\*[\s\S]*?\*\//g, '') },
    ...imports.flatMap(([, path]) => stylesheets(new URL(path, url), seen))];
}

test('covered Dev Flow styles have shared owners in the actual native CSS graph', () => {
  const graph = stylesheets('styles/legacy/main.css');
  const owners = graph.filter(({ rules }) => /\.dev-flow-detail(?:\s|[.{:[\]])/.test(rules));
  assert(owners.length > 0);
  for (const { url } of owners) {
    assert(url.href.includes('/styles/') || url.href.endsWith('/styles/native-host.css'),
      'duplicate Dev Flow style owner: ' + fileURLToPath(url));
  }
  assert.deepEqual(stylesheets('styles/legacy/features/sidebar.css').filter(({ rules }) => rules.trim()).map(({ url }) => url.href),
    [new URL('styles/features/sidebar.css', frontendRoot).href],
    'legacy Sidebar path must forward to its shared owner');
  assert(readFrontend('styles/legacy/components/feedback.css').includes('.runtime-notification-legacy'),
    'bootstrap error feedback remains available at its blocked boundary');
});

test('legacy tool status keeps its visible marker in the shared stylesheet', () => {
  const dom = createDOM('<div class="tool-call" data-tool-ui="legacy"><span class="tool-call-status"></span></div>');
  try {
    const style = dom.window.document.createElement('style');
    style.textContent = readFrontend('styles/legacy/features/tools.css');
    dom.window.document.head.append(style);
    const marker = dom.window.document.querySelector('.tool-call-status');
    assert.equal(dom.window.getComputedStyle(marker).width, '6px');
    assert.equal(dom.window.getComputedStyle(marker).borderRadius, '50%');
  } finally { dom.window.close(); }
});

function sources(root) {
  return readdirSync(root, { withFileTypes: true }).flatMap(entry => {
    const url = new URL(entry.name + (entry.isDirectory() ? '/' : ''), root);
    return entry.isDirectory() ? sources(url) : /\.(js|ts|css|html)$/.test(entry.name) ? [url] : [];
  });
}

test('production renderer references stay in frontend or use the shared session contract', () => {
  const sessionContract = new URL('../main/session-contract.js', frontendRoot).href;
  for (const root of ['./']) {
    for (const url of sources(new URL(root, frontendRoot))) {
      const text = readFileSync(url, 'utf8');
      for (const match of text.matchAll(/(?:from\s+|import\s*\(|url\(|(?:src|href|data-ui-template-src)=)["'](\.[^"']+)["']/g)) {
        const path = match[1];
        const target = new URL(path, url);
        assert(target.href.startsWith(frontendRoot.href) || target.href === sessionContract,
          'production reference leaves frontend outside the shared session contract: ' + target.href);
      }
      if (url.href.startsWith(new URL('scripts/', frontendRoot).href)) assert(!/window\.__TAURI__|from\s+["'][^"']*mock\//.test(text),
        'shared UI must not contain a host or mock dependency: ' + fileURLToPath(url));
    }
  }
});

test('every prototype template loads the same DOM as its production shared source', async () => {
  const dom = createDOM();
  try {
    await withDOM(dom, async () => {
      const prototypeRoot = new URL('prototype/templates/', repositoryRoot);
      for (const url of sources(prototypeRoot).filter(url => url.href.endsWith('.html'))) {
        const relative = url.href.slice(prototypeRoot.href.length);
        const prototype = await loadTemplate(url);
        const shared = await loadTemplate(new URL('templates/' + relative, frontendRoot));
        const left = document.createElement('div');
        const right = document.createElement('div');
        left.append(prototype); right.append(shared);
        assert.equal(left.innerHTML, right.innerHTML, 'duplicate template DOM: ' + relative);
      }
    });
  } finally { dom.window.close(); }
});
