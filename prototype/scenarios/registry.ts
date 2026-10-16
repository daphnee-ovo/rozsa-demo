export interface SceneDefinition {
  id: string; layout: string; components: string[];
  initialState: { settings: string; interruption: string; thinkingEffort?: number };
  mocks: Record<string, string>; actions: string[];
}
export interface GalleryDefinition { id: string; entry: string; stylesheet: string; behavior: string; verification: string[] }
const sceneDefinitions: Record<string, SceneDefinition> = {
  "empty-session": {
    id: "empty-session",
    layout: "application-shell",
    components: ["sidebar", "conversation", "composer", "popover"],
    initialState: { settings: "closed", interruption: "closed" },
    mocks: { sessions: "sessions/empty", messages: "messages/conversation" },
    actions: ["send-message", "open-thinking-effort"]
  },
  "complete-session": {
    id: "complete-session",
    layout: "application-shell",
    components: ["sidebar", "conversation", "composer", "popover", "tool-call", "queue", "steering"],
    initialState: { settings: "closed", interruption: "closed", thinkingEffort: 3 },
    mocks: { sessions: "sessions/complete", messages: "messages/conversation", tools: "tools/call-states" },
    actions: ["expand-tool", "toggle-thinking", "open-fork", "open-queue", "open-steering"]
  },
  "tool-calls": {
    id: "tool-calls",
    layout: "application-shell",
    components: ["sidebar", "conversation", "composer", "tool-call"],
    initialState: { settings: "closed", interruption: "closed" },
    mocks: { sessions: "sessions/empty", messages: "messages/conversation", tools: "tools/call-states" },
    actions: ["expand-tool", "toggle-thinking"]
  },
  "permission-request": {
    id: "permission-request",
    layout: "application-shell",
    components: ["sidebar", "conversation", "composer", "tool-call", "interruption-panel"],
    initialState: { settings: "closed", interruption: "permission" },
    mocks: { sessions: "sessions/empty", messages: "messages/conversation", tools: "tools/call-states", permission: "permissions/request", success: "responses/success", error: "responses/error", cancel: "responses/cancel" },
    actions: ["allow-permission", "trust-permission", "enter-hint", "cancel-permission"]
  },
  "ask-user-question": {
    id: "ask-user-question",
    layout: "application-shell",
    components: ["sidebar", "conversation", "composer", "tool-call", "interruption-panel"],
    initialState: { settings: "closed", interruption: "question" },
    mocks: { sessions: "sessions/empty", workspace: "sessions/complete", messages: "messages/conversation", tools: "tools/call-states", question: "questions/request", success: "responses/success", error: "responses/error", cancel: "responses/cancel" },
    actions: ["select-question", "edit-other", "submit-question", "cancel-question"]
  },
  "settings-appearance": {
    id: "settings-appearance",
    layout: "application-shell",
    components: ["sidebar", "conversation", "composer", "settings"],
    initialState: { settings: "appearance", interruption: "closed" },
    mocks: { sessions: "sessions/empty", messages: "messages/conversation", settings: "settings/appearance" },
    actions: ["switch-settings-tab", "change-theme", "change-font-size"]
  },
  "permissions": {
    id: "permissions",
    layout: "application-shell",
    components: ["sidebar", "conversation", "composer", "settings"],
    initialState: { settings: "permissions", interruption: "closed" },
    mocks: { sessions: "sessions/empty", messages: "messages/conversation", settings: "settings/permissions" },
    actions: ["open-permission-rule-editor", "toggle-permission-setting"]
  },
  "notifications": {
    id: "notifications",
    layout: "application-shell",
    components: ["sidebar", "conversation", "composer", "notification", "interruption-panel"],
    initialState: { settings: "closed", interruption: "permission" },
    mocks: { sessions: "sessions/empty", workspace: "sessions/complete", messages: "messages/conversation", tools: "tools/call-states", permission: "permissions/request", notifications: "notifications/states", success: "responses/success", error: "responses/error", cancel: "responses/cancel" },
    actions: ["dismiss-notification", "open-error-tray", "cancel-permission"]
  },
  "dev-flow-runtime": {
    id: "dev-flow-runtime",
    layout: "application-shell",
    components: ["sidebar", "conversation", "composer", "notification", "dev-flow", "tool-call"],
    initialState: { settings: "closed", interruption: "closed" },
    mocks: { sessions: "sessions/empty", workspace: "sessions/complete", messages: "messages/conversation", tools: "tools/call-states", devFlow: "dev-flow/runtime", notifications: "notifications/states" },
    actions: ["open-dev-flow-detail", "close-dev-flow-detail", "open-dashboard"]
  },
  "dev-flow-active": {
    id: "dev-flow-active",
    layout: "application-shell",
    components: ["sidebar", "conversation", "composer", "notification", "settings", "dev-flow"],
    initialState: { settings: "dev-flow", interruption: "closed" },
    mocks: { sessions: "sessions/empty", messages: "messages/conversation", devFlow: "dev-flow/runtime", notifications: "notifications/states" },
    actions: ["open-dashboard", "toggle-dev-flow-setting"]
  },
  "dev-flow": {
    id: "dev-flow",
    layout: "application-shell",
    components: ["sidebar", "conversation", "composer", "notification", "settings", "dev-flow"],
    initialState: { settings: "dev-flow", interruption: "closed" },
    mocks: { sessions: "sessions/empty", messages: "messages/conversation", devFlow: "dev-flow/runtime", notifications: "notifications/states" },
    actions: ["open-dashboard", "toggle-dev-flow-setting"]
  }
};

const galleryDefinitions: Record<string, GalleryDefinition> = {
  "visual-direction": {
    id: "visual-direction",
    entry: "gallery/visual-direction.html",
    stylesheet: "gallery/visual-direction.css",
    behavior: "gallery/visual-direction.js",
    verification: ["BASE-P-GALLERY-001", "BASE-P-GALLERY-002", "BASE-P-GALLERY-003", "BASE-P-GALLERY-004"]
  }
};

export const sceneRegistry = Object.freeze(
  Object.fromEntries(Object.entries(sceneDefinitions).map(([id, definition]) => [id, Object.freeze(definition)]))
);

export const galleryRegistry = Object.freeze(
  Object.fromEntries(Object.entries(galleryDefinitions).map(([id, definition]) => [id, Object.freeze(definition)]))
);

export function resolveSceneId(value: unknown) {
  const id = String(value || "empty-session");
  if (!Object.hasOwn(sceneRegistry, id)) {
    throw new Error(`Unknown prototype scene: ${id}`);
  }
  return id;
}

export function getSceneDefinition(value: unknown) {
  return sceneRegistry[resolveSceneId(value)];
}

export function resolveGalleryId(value: unknown) {
  const id = String(value || "");
  if (!Object.hasOwn(galleryRegistry, id)) {
    throw new Error(`Unknown prototype gallery: ${id || "(empty)"}`);
  }
  return id;
}

export function getGalleryDefinition(value: unknown) {
  return galleryRegistry[resolveGalleryId(value)];
}
