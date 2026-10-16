// Prototype contracts previously lived in Rust GUI tests. They belong to the new UI.
import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync, readdirSync, realpathSync } from 'node:fs';
import { basename, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const read = path => readFileSync(resolve(root, path), 'utf8');
const json = path => JSON.parse(read(path));
const ui = path => read('desktop/renderer/' + path);
const sorted = values => [...new Set(values)].sort();
const contains = (source, values) => values.forEach(value => assert.ok(source.includes(value), value));
function owners(path, visiting = new Set()) {
  path = realpathSync(resolve(root, path));
  assert.ok(!visiting.has(path), `cyclic CSS import: ${path}`);
  visiting.add(path);
  const imports = [...readFileSync(path, 'utf8').matchAll(/@import\s+url\(["']([^"']+)["']\);/g)];
  const result = imports.length ? imports.flatMap(([, relative]) => owners(resolve(dirname(path), relative), visiting)) : [path];
  visiting.delete(path);
  return result;
}

test('prototype coverage inventory stays bidirectional with scenes, events and design gaps', () => {
  const manifest = json('prototype/docs/UI_COVERAGE.json');
  const areas = ['application-shell-sidebar', 'conversation-composer-queue-steering', 'tool-call-evidence-diff-markdown',
    'permission-ask-user-question', 'settings-appearance-permissions', 'notification-feedback', 'dev-flow', 'gallery'];
  assert.deepEqual(sorted(manifest.areas), sorted(areas));
  assert.deepEqual(sorted(manifest.items.map(item => item.area)), sorted(areas));
  assert.equal(new Set(manifest.items.map(item => item.id)).size, manifest.items.length);
  for (const item of manifest.items) {
    assert.ok(['covered', 'partial', 'missing', 'non-visual'].includes(item.status));
    assert.ok(typeof item.runtime_trigger === 'string' && item.runtime_trigger.length);
    for (const field of ['runtime_entry', 'prototype_entry', 'css', 'markup', 'behavior', 'used_by', 'verification']) {
      assert.ok(Array.isArray(item[field]) && item[field].every(value => typeof value === 'string'), `${item.id}.${field}`);
      assert.ok(item[field].length || item.status === 'non-visual' && ['css', 'markup'].includes(field));
    }
    for (const reference of [...item.runtime_entry, ...item.prototype_entry]) {
      const path = reference.split('#')[0];
      if (/^(desktop|prototype|docs)\//.test(path)) assert.ok(existsSync(resolve(root, path)), path);
    }
  }
  const prototype = manifest.prototype_inventory;
  const actualScenes = readdirSync(resolve(root, 'prototype/scenes')).filter(path => path.endsWith('.html')).map(path => path.slice(0, -5));
  assert.deepEqual(sorted(actualScenes), sorted(prototype.scenes));
  const references = manifest.items.flatMap(item => [...item.prototype_entry, ...item.used_by]);
  for (const scene of prototype.scenes) assert.ok(references.some(entry => entry === scene || entry === `scene:${scene}` || entry.startsWith(`scene:${scene}#`)), scene);
  for (const field of ['application_root', 'gallery', 'script', 'gallery_script']) assert.ok(existsSync(resolve(root, prototype[field])));
  assert.ok(references.includes(prototype.application_root));
  assert.ok(references.some(path => path.startsWith(prototype.gallery)));
  assert.ok(manifest.items.flatMap(item => item.behavior).some(path => path.startsWith(prototype.gallery_script)));
  const gallery = manifest.items.filter(item => item.area === 'gallery');
  assert.equal(gallery.length, 4);
  for (const item of gallery) {
    assert.deepEqual(item.used_by, ['gallery:visual-direction']);
    assert.equal(item.markup.length, 1); assert.equal(item.behavior.length, 1);
  }
  const runtimeText = manifest.items.flatMap(item => [...item.runtime_entry, ...item.behavior]).join('\n');
  for (const path of [...manifest.runtime_inventory.entries, ...manifest.runtime_inventory.scripts]) {
    assert.ok(existsSync(resolve(root, path)), path); assert.ok(runtimeText.includes(basename(path)), path);
  }
  const itemEvents = manifest.items.flatMap(item => item.runtime_events || []);
  for (const [path, expected] of Object.entries(manifest.runtime_inventory.visible_events)) {
    const prefix = path.endsWith('runtime/adapters/notifications.js') ? "port.subscribe('" : path.endsWith('sidebar.js') ? "sidebarListen('" : "listen('";
    const actual = read(path).split(prefix).slice(1).map(tail => tail.split("'")[0]);
    assert.deepEqual(sorted(actual), sorted(expected), path);
    expected.forEach(event => assert.ok(itemEvents.includes(event), event));
  }
  const summary = read('prototype/docs/UI_COVERAGE.md'), gaps = read('prototype/docs/DESIGN_GAPS.md');
  const missing = manifest.items.filter(item => ['partial', 'missing'].includes(item.status));
  manifest.items.forEach(item => assert.ok(summary.includes(item.id), item.id));
  assert.deepEqual(sorted([...gaps.matchAll(/^- `coverage_id`：`([^`]+)`$/gm)].map(match => match[1])), sorted(missing.map(item => item.id)));
  for (const field of ['runtime_trigger', 'current_runtime', 'prototype_missing', 'required_prototype', 'blocked_scope']) {
    assert.equal(gaps.split(`- \`${field}\`：`).length - 1, missing.length, field);
  }
});

test('prototype scenes retain shared layout, fixture and component ownership', () => {
  const shell = read('prototype/scripts/core/shell.ts');
  assert.doesNotMatch(shell, /loadSceneStylesheet|prototypeSceneStylesheet|getSceneDefinition/);
  contains(shell, ['mountTemplateChildren(inputFragment']);
  contains(ui('scripts/core/renderer.ts'), ['export function mountTemplateChildren']);
  const registry = read('prototype/scenarios/registry.ts');
  const states = json('prototype/mock/tools/call-states.json');
  for (const scene of ['complete-session', 'ask-user-question', 'dev-flow-runtime']) {
    const start = registry.indexOf(`id: "${scene}"`); assert.ok(start >= 0, scene);
    assert.doesNotMatch(registry.slice(start, registry.indexOf('\n  },', start)), /stylesheet:/);
    assert.doesNotMatch(read(`prototype/scenes/${scene}.html`), /styles\/scenes\/|<style>/);
    assert.doesNotMatch(read(`prototype/styles/scenes/${scene}.css`), /data-rozsa-scene/);
  }
  for (const [scene, panels] of [['complete-session', ['subagentPanel', 'forkPicker', 'queuedMessages', 'steeringConversation']],
    ['dev-flow-runtime', ['queuedMessages', 'steeringConversation']]]) {
    panels.forEach(panel => assert.equal(typeof states.running[scene][panel], 'object'));
  }
  contains(read('prototype/scripts/adapters/mock-adapter.ts'), ['const sessions = runtime?.sessions || session?.sessions;']);
  contains(ui('templates/features/sidebar.html'), ['<aside id="sidebar" data-od-id="sidebar"']);
  contains(read('prototype/rozsa-gui.js'), ['document.body?.dataset.rozsaEntry === "application-shell"']);
  contains(read('prototype/scripts/components/interruption-panel.js'), ['desktop/renderer/scripts/components/interruption-panel.js']);
  contains(ui('scripts/components/interruption-panel.js'), ['document.querySelector(\'[data-od-id="chat-input"]\')']);
  const composer = ui('templates/composites/composer.html'), input = ui('templates/primitives/input.html');
  assert.equal(ui('templates/features/sidebar.html').split('id="subagentPanel"').length - 1, 1);
  assert.ok(!composer.includes('id="subagentPanel"'));
  const ordered = ['id="forkPicker"', 'class="input-wrapper"', 'id="queuedMessages"', 'id="steeringConversation"', 'data-template-mount="input"'];
  const indices = ordered.map(value => { assert.equal(composer.split(value).length - 1, 1); return composer.indexOf(value); });
  assert.deepEqual(indices, [...indices].sort((a, b) => a - b));
  assert.ok(input.includes('id="msgInput"') && !input.includes('class="input-wrapper"'));
  contains(ui('styles/layouts/app-shell.css'), ['[data-application-shell]', 'flex: 1 1 0', 'min-height: 0', 'overflow: hidden',
    'grid-template-columns: var(--main-sidebar-width) minmax(0, 1fr)', '[data-od-id="app-body"] { display: block; }',
    '[data-od-id="app-body"] > [data-template-mount="sidebar"],\n[data-od-id="app-body"] > [data-template-mount="conversation"] { display: contents; }']);
  contains(ui('styles/layouts/sidebar-shell.css'), ['data-ui-max-width~="780"', 'position: absolute', '.sidebar-edge-visible']);
  contains(ui('styles/features/sidebar.css'), ['.subagent-sidebar-panel', '.subagent-sidebar-panel ol', '.subagent-sidebar-panel li::before']);
  contains(ui('styles/composites/composer.css'), ['.input-wrapper > .running-messages:not([hidden]) ~ .rich-input', '[data-od-id="chat-input"] { position: absolute;']);
  contains(ui('styles/features/queue.css'), ['#queuedMessages']);
  contains(ui('styles/features/steering.css'), ['#steeringConversation']);
  contains(ui('styles/features/dev-flow-runtime.css'), ['.dev-flow-detail']);
  contains(ui('styles/composites/permission-question.css'), [':has(> .question-panel.visible)']);
});

test('prototype stylesheet graph has explicit owners, order and external entry resources', () => {
  const manifest = json('prototype/styles/source-order.json');
  for (const block of manifest.blocks) {
    const source = read(block.target);
    assert.ok(!source.includes('@import') || Array.isArray(block.owners), block.target);
    for (const owner of block.owners || []) {
      assert.ok(owners(block.target).includes(realpathSync(resolve(root, owner))), owner);
      assert.ok(!read(owner).includes('@import'), owner);
    }
  }
  assert.equal(manifest.responsive_layout.sidebar_auto_collapse_max_width, 780);
  assert.deepEqual(manifest.responsive_layout.minimum_window, { width: 640, height: 480 });
  assert.deepEqual(manifest.extracted_inline_styles, []);
  for (const entry of ['main', 'sidebar']) {
    assert.equal(read(`prototype/styles/${entry}.css`).trim(), `@import url("./entries/${entry}.css");`);
    assert.doesNotMatch(read(`prototype/styles/entries/${entry}.css`), /\.\.\/entries\/(main|sidebar)\.css|utilities\.css/);
    owners(`prototype/styles/${entry}.css`);
  }
  const main = read('prototype/styles/entries/main.css');
  contains(main, ['../composites/composer.css', '../features/queue.css', '../features/steering.css']);
  assert.ok(!main.includes('../primitives/forms.css'));
  for (const path of ['composites/composer.css', 'features/queue.css', 'features/steering.css', 'features/dev-flow-runtime.css']) {
    assert.ok(owners('prototype/styles/entries/main.css').includes(realpathSync(resolve(root, 'desktop/renderer/styles', path))), path);
  }
  assert.deepEqual(owners('prototype/styles/entries/sidebar.css'), [...owners('desktop/renderer/styles/entries/sidebar.css'),
    ...['features/sidebar.css', 'features/sidebar-window.css', 'foundations/motion.css'].map(path => realpathSync(resolve(root, 'desktop/renderer/styles', path)))]);
  assert.equal(read('prototype/gallery/visual-direction.css').trim(), '@import url("../styles/features/visual-demo.css");');
  contains(read('prototype/gallery/visual-direction.html'), ['href="visual-direction.css"', 'src="visual-direction.js"']);
  assert.ok(!read('prototype/gallery/visual-direction.html').includes('<style>'));
  for (const scene of readdirSync(resolve(root, 'prototype/scenes')).filter(path => path.endsWith('.html'))) {
    const source = read('prototype/scenes/' + scene);
    assert.ok(source.includes('href="../styles/main.css"'));
    assert.ok(!source.includes('href="../rozsa-gui.css"'));
  }
  assert.ok(!existsSync(resolve(root, 'prototype/rozsa-gui.css')));
});

test('root prototype remains complete with no retired entries', () => {
  for (const path of ['index.html', 'gallery/visual-direction.html', 'gallery/visual-direction.css', 'gallery/visual-direction.js',
    'scenarios/registry.ts', 'scripts/core/renderer.ts', 'scripts/core/shell.ts', 'scripts/core/template-loader.ts',
    'scripts/core/scene-loader.ts', 'scripts/adapters/mock-adapter.ts', 'templates/features/sidebar.html',
    'styles/main.css', 'styles/entries/main.css', 'mock/sessions/empty.json',
    ...['ARCHITECTURE', 'COMPONENTS', 'SCENARIOS', 'FLOWS', 'VALIDATION', 'MOCK_DATA', 'UI_COVERAGE', 'DESIGN_GAPS'].map(name => `docs/${name}.md`), 'docs/UI_COVERAGE.json']) {
    assert.ok(existsSync(resolve(root, 'prototype', path)), path);
  }
  for (const duplicate of ['scenarios/registry.js', 'scripts/core/renderer.js', 'scripts/core/shell.js',
    'scripts/core/template-loader.js', 'scripts/core/scene-loader.js', 'scripts/adapters/mock-adapter.js']) {
    assert.ok(!existsSync(resolve(root, 'prototype', duplicate)), `retired duplicate JavaScript source: ${duplicate}`);
  }
  for (const duplicate of ['scripts/core/ports.js', 'scripts/core/renderer.js', 'scripts/core/shell.js',
    'scripts/core/template-loader.js', 'scripts/core/viewport.js', 'runtime/entries/main.js',
    'runtime/entries/sidebar.js', 'runtime/adapters/tauri.js']) {
    assert.ok(!existsSync(resolve(root, 'desktop/renderer', duplicate)), `retired duplicate JavaScript source: ${duplicate}`);
  }
  for (const retired of ['docs/gui/prototype', 'docs/gui/new-version', 'docs/gui/UI_COVERAGE.md', 'docs/gui/UI_COVERAGE.json',
    'docs/gui/DESIGN_GAPS.md', 'prototype/rozsa-gui.html', 'prototype/rozsa-visual-demo.html']) assert.ok(!existsSync(resolve(root, retired)), retired);
});
