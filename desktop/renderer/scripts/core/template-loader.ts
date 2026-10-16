const templateCache = new Map<string, Promise<string>>();

export async function loadTemplate(path: URL | string): Promise<DocumentFragment> {
  const key = String(path);
  if (!templateCache.has(key)) {
    const response = await fetch(path);
    if (!response.ok) {
      throw new Error("Unable to load UI template: " + path + " (" + response.status + ")");
    }
    templateCache.set(key, response.text());
  }
  const markup = await templateCache.get(key)!;
  const template = document.createElement("template");
  template.innerHTML = markup.trim();
  const reference = template.content.querySelector<HTMLTemplateElement>("template[data-ui-template-src]");
  if (reference) return loadTemplate(new URL(reference.dataset.uiTemplateSrc!, path));
  return template.content.cloneNode(true) as DocumentFragment;
}

export function clearTemplateCache(): void {
  templateCache.clear();
}
