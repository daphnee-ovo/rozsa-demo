import type { SceneDefinition } from '../../scenarios/registry.ts';
import type { RequestOptions } from '../../../desktop/renderer/scripts/core/ports.ts';

export interface ConversationAPI {
  attachFileReference: (...args: never[]) => unknown;
  attachDirectoryReference: (...args: never[]) => unknown;
  insertSlashCommandPrefix: (...args: never[]) => unknown;
  showModelPicker: (...args: never[]) => unknown;
  addMessage: (...args: never[]) => unknown;
  updateAbortButton: (...args: never[]) => unknown;
}
interface MessageFixture { toolRefs?: string[]; [field: string]: unknown }
interface SessionFixture { sessions?: unknown[]; activeSession?: number; workspace?: unknown }
interface ToolFixtures { running?: Record<string, unknown>; [reference: string]: unknown }
export interface MockSet {
  sessions?: SessionFixture; devFlow?: SessionFixture;
  messages?: Record<string, MessageFixture[]>; tools?: ToolFixtures;
  workspace?: { workspace?: unknown };
  permission?: unknown; question?: unknown; settings?: unknown; notifications?: unknown;
  success?: unknown; error?: unknown; cancel?: unknown;
}

const mockRoot = new URL("../../mock/", import.meta.url);

export function createMockConversationAdapter(api: ConversationAPI) {
  return Object.freeze({
    attachFile: api.attachFileReference,
    attachDirectory: api.attachDirectoryReference,
    slashCommand: api.insertSlashCommandPrefix,
    chooseModel: api.showModelPicker,
    submit: api.addMessage,
    refreshSendState: api.updateAbortButton,
  });
}

async function readMock(reference: string, signal?: AbortSignal) {
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

export async function loadMockSet(references: Record<string, string>, signal?: AbortSignal): Promise<MockSet> {
  const entries = Object.entries(references || {});
  if (!entries.length) throw new Error("Prototype scene has no mock references");
  const loaded = await Promise.all(entries.map(async ([key, reference]) => [key, await readMock(reference, signal)]));
  return Object.fromEntries(loaded);
}

export function createMockAdapter() {
  return Object.freeze({
    async request(intent: string, data: unknown, { signal }: RequestOptions = {}) {
      if (intent !== "load-scene") throw new Error(`Unsupported prototype intent: ${intent}`);
      const definition = data as SceneDefinition;
      return adaptSceneData(definition, await loadMockSet(definition.mocks, signal));
    },
    subscribe(event: string, receive: (payload: unknown) => void) {
      const listener = (message: Event) => receive((message as CustomEvent<unknown>).detail);
      document.addEventListener(event, listener);
      return () => document.removeEventListener(event, listener);
    },
  });
}

function resolveMessages(messages: MockSet['messages'], tools: MockSet['tools'], sceneId: string) {
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

export function adaptSceneData(definition: SceneDefinition, mocks: MockSet) {
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
