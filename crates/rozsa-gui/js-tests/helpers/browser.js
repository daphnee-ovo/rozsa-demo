import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { Script } from 'node:vm';
import { JSDOM } from 'jsdom';

export const repositoryRoot = new URL('../../../../', import.meta.url);
export const frontendRoot = new URL('crates/rozsa-gui/frontend/', repositoryRoot);
export const readFrontend = path => readFileSync(new URL(path, frontendRoot), 'utf8');

export function createDOM(html = '<!doctype html><html><body></body></html>') {
  return new JSDOM(html, { url: 'http://127.0.0.1/', runScripts: 'outside-only', pretendToBeVisual: true });
}

// Run the complete classic entry, preserving browser global lexical bindings.
// Unit tests trigger events explicitly; native startup belongs to native tests.
export function loadApp({ html, globals = {} } = {}) {
  return loadClassic('app.js', { html, globals });
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
    if (url.protocol !== 'file:') throw new Error('Unexpected external fetch in UI unit test: ' + url);
    return new Response(readFileSync(fileURLToPath(url), 'utf8'));
  };
  try { return await action(); }
  finally {
    for (const [name, descriptor] of previous) {
      if (descriptor) Object.defineProperty(globalThis, name, descriptor);
      else delete globalThis[name];
    }
  }
}
