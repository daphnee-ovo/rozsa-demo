import test from 'node:test';
import assert from 'node:assert/strict';
import { cp, mkdir, mkdtemp, readdir, readFile, symlink } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { build } from '../node_modules/vite/dist/node/index.js';

test('Electron bundles from its own source tree without the Rust GUI or prototype', async () => {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
  const base = join(root, 'tmp/project-independence');
  await mkdir(base, { recursive: true });
  const fixture = await mkdtemp(join(base, 'electron-'));
  await mkdir(join(fixture, 'desktop'));
  for (const path of ['renderer', 'main', 'vite.renderer.config.mjs']) {
    await cp(join(root, 'desktop', path), join(fixture, 'desktop', path), { recursive: true });
  }
  await cp(join(root, 'contracts'), join(fixture, 'contracts'), { recursive: true });
  await symlink(join(root, 'desktop/node_modules'), join(fixture, 'desktop/node_modules'), process.platform === 'win32' ? 'junction' : 'dir');
  await build({ configFile: join(fixture, 'desktop/vite.renderer.config.mjs'), logLevel: 'error' });
  const output = join(fixture, 'desktop/.vite/renderer/sessions');
  const files = await readdir(output);
  assert.ok(files.includes('index.html'));
  for (const retired of ['tauri.html', 'sidebar.html', 'app.js', 'sidebar.js', 'gui_shared.js']) {
    assert.ok(!files.includes(retired), `Electron package must not build the retired host entry: ${retired}`);
  }
  const html = await readFile(join(output, 'index.html'), 'utf8');
  for (const [, reference] of html.matchAll(/(?:src|href)=["']([^"']+)["']/g)) {
    assert.ok(reference.startsWith('./'), `package asset must be relative: ${reference}`);
    await readFile(resolve(output, reference));
  }
});
