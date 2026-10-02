const mockRoot = new URL("../../mock/", import.meta.url);

export function createMockConversationAdapter(api) {
  return Object.freeze({
    attachFile: api.attachFileReference,
    attachDirectory: api.attachDirectoryReference,
    slashCommand: api.insertSlashCommandPrefix,
    chooseModel: api.showModelPicker,
    submit: api.addMessage,
    refreshSendState: api.updateAbortButton,
  });
}

async function readMock(reference, signal) {
  if (!reference) throw new Error("Prototype scene mock reference is missing");
  const url = new URL(reference + ".json", mockRoot);
  const response = await fetch(url, { signal });
  if (!response.ok) throw new Error(`Prototype mock could not be loaded: ${reference}`);
  try {
    return await response.json();
  } catch (error) {
    throw new Error(`Prototype mock is not valid JSON: ${reference}`, { cause: error });
  }
}

export async function loadMockSet(references, signal) {
  const entries = Object.entries(references || {});
  if (!entries.length) throw new Error("Prototype scene has no mock references");
  const loaded = await Promise.all(entries.map(async ([key, reference]) => [key, await readMock(reference, signal)]));
  return Object.fromEntries(loaded);
}

export function createMockAdapter() {
  return Object.freeze({
    async request(intent, definition, { signal } = {}) {
      if (intent !== "load-scene") throw new Error(`Unsupported prototype intent: ${intent}`);
      return adaptSceneData(definition, await loadMockSet(definition.mocks, signal));
    },
    subscribe(event, receive) {
      const listener = message => receive(message.detail);
      document.addEventListener(event, listener);
      return () => document.removeEventListener(event, listener);
    },
  });
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
  const sessions = runtime?.sessions || session?.sessions;
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
