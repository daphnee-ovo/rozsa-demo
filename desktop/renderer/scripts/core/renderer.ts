import { createPort, applyRevision } from "./ports.ts";
import type { UIAdapter } from "./ports.ts";

export function createUIHost(adapter: UIAdapter) {
  return Object.freeze({ port: createPort(adapter), applyRevision });
}

export function mountTemplate(fragment: DocumentFragment, selector: string) {
  const target = document.querySelector(selector);
  if (!target) throw new Error("UI template mount not found: " + selector);
  return mountTemplateInto(fragment, target);
}

export function mountTemplateChildren(fragment: DocumentFragment, selector: string) {
  const target = document.querySelector(selector);
  if (!target) throw new Error("UI template mount not found: " + selector);
  const nodes = Array.from(fragment.childNodes);
  target.replaceWith(...nodes);
  return nodes.find(node => node.nodeType === Node.ELEMENT_NODE) || null;
}

export function mountTemplateInto(fragment: DocumentFragment, target: Element) {
  if (!(target instanceof Element)) throw new TypeError("UI template target must be an Element");
  target.replaceChildren(fragment);
  target.removeAttribute("hidden");
  return target.firstElementChild;
}
