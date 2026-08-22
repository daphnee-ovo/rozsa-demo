export function mountTemplate(fragment, selector) {
  const target = document.querySelector(selector);
  if (!target) {
    throw new Error("Prototype template mount not found: " + selector);
  }
  target.replaceChildren(fragment);
  target.removeAttribute("hidden");
  return target.firstElementChild;
}

export function mountTemplateInto(fragment, target) {
  if (!(target instanceof Element)) {
    throw new TypeError("Prototype template target must be an Element");
  }
  target.replaceChildren(fragment);
  target.removeAttribute("hidden");
  return target.firstElementChild;
}

