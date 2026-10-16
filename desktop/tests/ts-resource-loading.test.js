import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const desktopRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const resourceRoot = resolve(desktopRoot, '.vite/renderer/sessions');

function filesUnder(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
    const path = resolve(directory, entry.name);
    return entry.isDirectory() ? filesUnder(path) : [path];
  });
}

test('Vite emits relative Electron resources with source maps', () => {
  for (const page of ['index.html']) {
    const path = resolve(resourceRoot, page);
    assert.ok(existsSync(path), `generated page missing: ${page}`);
    const html = readFileSync(path, 'utf8');
    assert.doesNotMatch(html, /\.tsx?(?:["'?#\s])/i, `${page} must load generated JavaScript`);
    for (const [, reference] of html.matchAll(/(?:src|href)=["']([^"']+)["']/g)) {
      if (/^https?:\/\//.test(reference)) continue;
      assert.ok(existsSync(new URL(reference, `file://${path}`).pathname), `${page} has a missing relative resource: ${reference}`);
    }
    assert.doesNotMatch(html, /mock-adapter|prototype\/mock|\/mock\//);
  }


  const generated = filesUnder(resourceRoot);
  const scripts = generated.filter(path => path.endsWith('.js'));
  const maps = new Set(generated.filter(path => path.endsWith('.js.map')));
  assert.ok(scripts.some(path => path.endsWith('/assets/index-') && path.endsWith('.js')) || scripts.some(path => /\/assets\/index-[^/]+\.js$/.test(path)));
  assert.ok(maps.size >= 1, 'renderer bundles must include source maps');
  for (const path of generated.filter(file => /\.(?:html|js|css)$/.test(file))) {
    const source = readFileSync(path, 'utf8');
    assert.doesNotMatch(source, /\.tsx?(?:["'?#\s])/i, `${path} must not reference TypeScript at runtime`);
    assert.doesNotMatch(source, /mock-adapter|prototype\/mock|\/mock\//, `${path} must not package prototype mocks`);
  }
});
