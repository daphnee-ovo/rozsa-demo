const templateCache = new Map();

export async function loadTemplate(path) {
  if (!templateCache.has(path)) {
    const response = await fetch(path);
    if (!response.ok) {
      throw new Error("Unable to load prototype template: " + path + " (" + response.status + ")");
    }
    templateCache.set(path, response.text());
  }
  const markup = await templateCache.get(path);
  const template = document.createElement("template");
  template.innerHTML = markup.trim();
  return template.content.cloneNode(true);
}

export function clearTemplateCache() {
  templateCache.clear();
}

