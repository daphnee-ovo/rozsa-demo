import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { Script } from 'node:vm';
import { JSDOM } from 'jsdom';

export const repositoryRoot = new URL('../../../', import.meta.url);
export const frontendRoot = new URL('desktop/renderer/', repositoryRoot);
export const readFrontend = path => readFileSync(new URL(path, frontendRoot), 'utf8');
export const readPrototype = path => readFileSync(new URL('prototype/' + path, repositoryRoot), 'utf8');

export function createDOM(html = '<!doctype html><html><body></body></html>', url = 'http://127.0.0.1/') {
  return new JSDOM(html, { url, runScripts: 'outside-only', pretendToBeVisual: true });
}

// Run the complete classic entry, preserving browser global lexical bindings.
// Unit tests trigger events explicitly; native startup belongs to native tests.
export function loadApp({ html, globals = {} } = {}) {
  const dom = createDOM(html);
  const { window } = dom;
  Object.assign(window, globals);
  const addWindowListener = window.addEventListener.bind(window);
  window.addEventListener = (name, callback, options) => {
    if (name !== 'DOMContentLoaded') addWindowListener(name, callback, options);
  };
  const context = dom.getInternalVMContext();
  new Script(readFrontend('gui_shared.js'), { filename: 'gui_shared.js' }).runInContext(context);
  const index = readFrontend('index.html');
  if (!index.includes('<script src="./runtime/app.js"></script>')) throw new Error('Renderer index.html has no classic app entry');
  const appModules = readFrontend('runtime/app/order.txt').trim().split('\n');
  let appSource = '';
  for (const path of appModules) {
    const source = readFrontend(`runtime/app/${path}`);
    appSource += `${source}\n`;
  }
  new Script(appSource, { filename: 'runtime/app.js' }).runInContext(context);
  return { dom, window, document: window.document,
    evaluate: source => new Script(source, { filename: 'test-action.js' }).runInContext(context),
    close: () => window.close(),
  };
}

export function loadClassic(path, { html, globals = {} } = {}) {
  const dom = createDOM(html);
  const { window } = dom;
  Object.assign(window, globals);
  const addWindowListener = window.addEventListener.bind(window);
  window.addEventListener = (name, callback, options) => {
    if (name !== 'DOMContentLoaded') addWindowListener(name, callback, options);
  };
  const context = dom.getInternalVMContext();
  new Script(readFrontend(path), { filename: path }).runInContext(context);
  return { dom, window, document: window.document,
    evaluate: source => new Script(source, { filename: 'test-action.js' }).runInContext(context),
    close: () => window.close(),
  };
}

// Shared modules see a real DOM and local templates for one bounded test.
export async function withDOM(dom, action) {
  const names = ['window', 'document', 'Node', 'Element', 'DOMParser', 'fetch'];
  const previous = new Map(names.map(name => [name, Object.getOwnPropertyDescriptor(globalThis, name)]));
  for (const name of names.slice(0, -1)) globalThis[name] = dom.window[name];
  globalThis.fetch = async input => {
    const url = new URL(input);
    if (url.protocol === 'file:') return new Response(readFileSync(fileURLToPath(url), 'utf8'));
    if (url.origin === 'http://127.0.0.1') {
      const match = /(?:^|\/)templates\/(.+)$/.exec(decodeURIComponent(url.pathname));
      if (match && !match[1].split('/').includes('..')) {
        return new Response(readFileSync(new URL(`templates/${match[1]}`, frontendRoot), 'utf8'));
      }
    }
    throw new Error('Unexpected external fetch in UI unit test: ' + url);
  };
  try { return await action(); }
  finally {
    for (const [name, descriptor] of previous) {
      if (descriptor) Object.defineProperty(globalThis, name, descriptor);
      else delete globalThis[name];
    }
  }
}
