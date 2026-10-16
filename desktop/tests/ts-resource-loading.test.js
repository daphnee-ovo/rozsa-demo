import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createServer } from 'vite';

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

test('Vite HTTP prototype loads transformed TypeScript and shared templates and styles', async () => {
  const server = await createServer({
    configFile: resolve(desktopRoot, 'vite.renderer.config.mjs'),
    logLevel: 'silent',
    server: { host: '127.0.0.1', port: 0 },
  });
  await server.listen();
  try {
    const address = server.httpServer.address();
    assert.ok(address && typeof address !== 'string');
    const origin = `http://127.0.0.1:${address.port}`;
    const prototypeRoot = resolve(desktopRoot, '../prototype');
    const atFile = path => `${origin}/@fs${path}`;

    const page = await fetch(`${atFile(resolve(prototypeRoot, 'index.html'))}?scene=empty-session`);
    assert.equal(page.status, 200);
    const html = await page.text();
    assert.match(html, /@vite\/client/);
    assert.match(html, /rozsa-template-root/);
    assert.match(html, /scenarios\/registry\.ts/);

    const viteClient = await fetch(`${origin}/@vite/client`);
    assert.equal(viteClient.status, 200);
    assert.match(viteClient.headers.get('content-type') || '', /javascript/);

    const registry = await fetch(atFile(resolve(prototypeRoot, 'scenarios/registry.ts')));
    assert.equal(registry.status, 200);
    assert.match(registry.headers.get('content-type') || '', /javascript/);
    const javascript = await registry.text();
    assert.match(javascript, /sourceMappingURL=/);
    assert.doesNotMatch(javascript, /:\s*(?:string|number|boolean)\b/);

    const shell = await fetch(atFile(resolve(prototypeRoot, 'scripts/core/shell.ts')));
    assert.equal(shell.status, 200);
    assert.match(shell.headers.get('content-type') || '', /javascript/);
    assert.match(await shell.text(), /resolveTemplateRoot/);

    const templatePath = resolve(desktopRoot, 'renderer/templates/features/sidebar.html');
    const template = await fetch(atFile(templatePath));
    assert.equal(template.status, 200);
    assert.match(await template.text(), /id="sessionList"/);

    const stylesheetPath = resolve(desktopRoot, 'renderer/styles/legacy/main.css');
    const stylesheet = await fetch(atFile(stylesheetPath));
    assert.equal(stylesheet.status, 200);
    assert.match(stylesheet.headers.get('content-type') || '', /javascript/);
    assert.match(await stylesheet.text(), /@vite\/client/);
  } finally {
    await server.close();
  }
});
