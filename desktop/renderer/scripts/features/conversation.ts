export type ConversationRole = "user" | "assistant";

export interface RunningPanel {
  title: string;
  count?: string | number;
  items: string[];
}

export interface ThinkingOptions {
  expanded?: boolean;
  active?: boolean;
  duration?: string;
}

export interface ConversationAPI {
  boundModules: Set<string>;
  byId(id: string): HTMLElement | null;
  renderMessages(): void;
  toggleThinking(header: Element): void;
}

export function init(api: ConversationAPI): void {
  if (api.boundModules.has("conversation")) return;
  api.boundModules.add("conversation");
  const chat = api.byId("chatMessages");
  chat?.addEventListener("click", event => {
    const header = (event.target as Element | null)?.closest(".thinking-header");
    if (header) api.toggleThinking(header);
  });
  chat?.addEventListener("keydown", event => {
    if (event.key !== "Enter" && event.key !== " ") return;
    const header = (event.target as Element | null)?.closest(".thinking-header");
    if (!header) return;
    event.preventDefault();
    api.toggleThinking(header);
  });
  api.renderMessages();
}

export function emptyConversationMarkup(): string {
  return '<div class="chat-empty" data-od-id="chat-empty"><div class="chat-empty-icon">R</div>' +
    '<div class="chat-empty-title">Start a new conversation</div>' +
    '<div class="chat-empty-hint">Describe your coding task to Rózsa' +
    '<div class="chat-empty-kbd"><kbd>Enter</kbd> Send <kbd>⇧ Enter</kbd> New line</div></div></div>';
}

export function messageFrame(role: ConversationRole, body: string, index: number, continuation = false): string {
  if (role !== "user" && role !== "assistant") throw new TypeError("Unsupported conversation role");
  const hiddenIdentity = continuation && role === "assistant";
  const avatar = hiddenIdentity ? "" : '<div class="msg-avatar">' + (role === "user" ? "U" : "R") + "</div>";
  const identity = hiddenIdentity ? "" : '<div class="msg-role">' + (role === "user" ? "You" : "Rozsa") + "</div>";
  return '<div class="msg msg-' + role + (continuation ? " msg-assistant-continuation" : "") +
    '" data-od-id="message-' + index + '">' + avatar + '<div class="msg-body">' + identity +
    '<div class="msg-content markdown-body">' + body + "</div></div></div>";
}

export function createMessageFrame(role: ConversationRole, body: string, index: number, continuation = false): HTMLDivElement | null {
  const template = document.createElement("template");
  template.innerHTML = messageFrame(role, body, index, continuation);
  return template.content.firstElementChild as HTMLDivElement | null;
}

export function runningPanelMarkup(panel: RunningPanel | null): string {
  if (!panel) return "";
  if (!Array.isArray(panel.items)) throw new TypeError("Running panel items must be an array");
  const count = panel.count ? " <span>" + escapeDisplayText(panel.count) + "</span>" : "";
  return '<div class="running-messages-title">' + escapeDisplayText(panel.title) + count + "</div><ol>" +
    panel.items.map(item => "<li>" + escapeDisplayText(item) + "</li>").join("") + "</ol>";
}

function escapeDisplayText(value: string | number): string {
  return String(value).replace(/[&<>"']/g, character => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[character]!);
}

export function thinkingMarkup(content: string, { expanded = true, active = false, duration = "" }: ThinkingOptions = {}): string {
  const icon = (paths: string, className = "") => '<svg class="lucide ' + className + '" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' + paths + "</svg>";
  return '<div class="thinking-block' + (expanded ? " expanded" : "") + (active ? " active" : "") + '">' +
    '<div class="thinking-header" role="button" tabindex="0" aria-expanded="' + String(expanded) + '">' +
    icon('<path d="M9 18h6"/><path d="M10 22h4"/><path d="M15.09 14c.18-.7.66-1.22 1.18-1.75A6 6 0 1 0 7.73 12.25c.52.52 1 1.04 1.18 1.75Z"', "thinking-icon") +
    '<span class="thinking-label">' + (active ? "THINKING" : "THINKED") + "</span>" +
    (duration ? '<span class="thinking-duration">' + escapeDisplayText(duration) + "</span>" : "") +
    '<span class="thinking-chevron">' + icon('<path d="m9 18 6-6-6-6"/>') + "</span></div>" +
    '<div class="thinking-content"' + (active ? ' data-stream-cursor-target="thinking"' : "") +
    '><div class="thinking-markdown markdown-body">' + content + "</div></div></div>";
}
