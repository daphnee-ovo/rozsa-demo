const mockRoot = new URL("../../mock/", import.meta.url);

async function readMock(reference) {
  if (!reference) throw new Error("Prototype scene mock reference is missing");
  const url = new URL(reference + ".json", mockRoot);
  const response = await fetch(url);
  if (!response.ok) throw new Error(`Prototype mock could not be loaded: ${reference}`);
  try {
    return await response.json();
  } catch (error) {
    throw new Error(`Prototype mock is not valid JSON: ${reference}`, { cause: error });
  }
}

export async function loadMockSet(references) {
  const entries = Object.entries(references || {});
  if (!entries.length) throw new Error("Prototype scene has no mock references");
  const loaded = await Promise.all(entries.map(async ([key, reference]) => [key, await readMock(reference)]));
  return Object.fromEntries(loaded);
}

function resolveMessages(messages, tools, sceneId) {
  const source = messages?.[sceneId];
  if (!Array.isArray(source)) throw new Error(`Prototype message mock is missing scene: ${sceneId}`);
  return source.map(message => {
    if (!message.toolRefs) return message;
    const resolvedTools = message.toolRefs.map(reference => {
      const tool = tools?.[reference];
      if (!tool) throw new Error(`Prototype tool mock is missing: ${reference}`);
      return tool;
    });
    const { toolRefs, ...rest } = message;
    return { ...rest, tools: resolvedTools };
  });
}

export function adaptSceneData(definition, mocks) {
  const session = mocks.sessions;
  const runtime = mocks.devFlow;
  const sessions = session?.sessions || runtime?.sessions;
  if (!Array.isArray(sessions)) throw new Error(`Prototype session mock is missing scene: ${definition.id}`);

  return {
    id: definition.id,
    definition,
    sessions,
    activeSession: session?.activeSession ?? 0,
    messages: resolveMessages(mocks.messages, mocks.tools, definition.id),
    workspace: session?.workspace || mocks.workspace?.workspace || null,
    running: mocks.tools?.running?.[definition.id] || {},
    permission: mocks.permission || null,
    question: mocks.question || null,
    settings: mocks.settings || null,
    notifications: mocks.notifications || null,
    devFlow: runtime || null,
    responses: {
      success: mocks.success || null,
      error: mocks.error || null,
      cancel: mocks.cancel || null
    }
  };
}
