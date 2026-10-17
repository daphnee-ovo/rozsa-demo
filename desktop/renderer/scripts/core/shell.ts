import { loadTemplate } from "./template-loader.ts";
import { mountTemplate, mountTemplateChildren } from "./renderer.ts";

export interface TemplateMount { url: URL; selector: string; children?: boolean }

export function resolveTemplateRoot(): URL {
  const templateRootUrl = document.querySelector<HTMLMetaElement>('meta[name="rozsa-template-root"]')?.content;
  if (templateRootUrl) return new URL(templateRootUrl, document.baseURI);
  return new URL("../../templates/", import.meta.url);
}

// Hosts choose covered regions; shared owners define their stable DOM.
// Persistent scene roots are retained when mounting their initial children.
export async function mountShell(mounts: TemplateMount[]): Promise<void> {
	const fragments = await Promise.all(mounts.map(({ url }) => loadTemplate(url)));
	for (const [index, { selector, children = false }] of mounts.entries()) {
		const fragment = fragments[index];
    if (children) mountTemplateChildren(fragment, selector);
    else mountTemplate(fragment, selector);
  }
}
