import { loadTemplate } from "./template-loader.js";
import { mountTemplate, mountTemplateChildren } from "./renderer.js";

export const templateRoot = new URL("../../templates/", import.meta.url);

// Hosts choose covered regions; shared owners define their stable DOM.
// Persistent scene roots are retained when mounting their initial children.
export async function mountShell(mounts) {
  for (const { url, selector, children = false } of mounts) {
    const fragment = await loadTemplate(url);
    if (children) mountTemplateChildren(fragment, selector);
    else mountTemplate(fragment, selector);
  }
}
