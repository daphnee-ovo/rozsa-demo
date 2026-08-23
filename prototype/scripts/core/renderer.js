export function mountTemplate(fragment, selector) {
  const target = document.querySelector(selector);
  if (!target) {
    throw new Error("Prototype template mount not found: " + selector);
  }
  target.replaceChildren(fragment);
  target.removeAttribute("hidden");
  return target.firstElementChild;
}

export function mountTemplateChildren(fragment, selector) {
  const target = document.querySelector(selector);
  if (!target) {
    throw new Error("Prototype template mount not found: " + selector);
  }
  const nodes = Array.from(fragment.childNodes);
  target.replaceWith(...nodes);
  return nodes.find(node => node.nodeType === Node.ELEMENT_NODE) || null;
}

export function mountTemplateInto(fragment, target) {
  if (!(target instanceof Element)) {
    throw new TypeError("Prototype template target must be an Element");
  }
  target.replaceChildren(fragment);
  target.removeAttribute("hidden");
  return target.firstElementChild;
}
