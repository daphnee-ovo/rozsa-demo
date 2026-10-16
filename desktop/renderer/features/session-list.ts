import type { SessionListEntry } from "../../main/session-contract.js";

export interface SessionSource { getSessions(): Promise<SessionListEntry[]> }
// The candidate owns this view; it does not import the retained Tauri frontend.
export class SessionList {
  private activePath: string | undefined;
  constructor(private readonly source: SessionSource, private readonly list: HTMLElement, private readonly status: HTMLElement, private readonly refresh: HTMLButtonElement,
    private readonly select?: (entry: SessionListEntry) => void) {
    refresh.addEventListener("click", () => { void this.load(); });
  }
  async load(): Promise<void> {
    this.refresh.disabled = true;
    this.list.replaceChildren();
    this.status.textContent = "Loading sessions…";
    this.status.setAttribute("role", "status");
    try {
      const sessions = await this.source.getSessions();
      for (const entry of sessions) {
        const row = this.list.ownerDocument.createElement("li");
        const name = this.list.ownerDocument.createElement("span");
        const meta = this.list.ownerDocument.createElement("span");
        row.className = "session-item";
        const button = this.list.ownerDocument.createElement("button");
        button.type = "button"; button.className = "session-select";
        button.dataset.sessionPath = entry.path;
        button.setAttribute("aria-pressed", String(entry.path === this.activePath));
        button.addEventListener("click", () => this.select?.(entry));
        name.className = "session-name"; name.textContent = entry.name;
        meta.className = "session-meta"; meta.textContent = `${entry.message_count} messages · ${entry.modified}`;
        row.title = entry.path; row.dataset.sessionId = entry.id;
        button.append(name, meta); row.append(button); this.list.append(row);
      }
      this.status.textContent = sessions.length === 0 ? "No sessions yet." : `${sessions.length} sessions`;
    } catch (error) {
      this.status.setAttribute("role", "alert");
      this.status.textContent = `Could not load sessions: ${error instanceof Error ? error.message : String(error)}`;
    } finally { this.refresh.disabled = false; }
  }
  setActive(path: string): void {
    this.activePath = path;
    for (const button of Array.from(this.list.querySelectorAll<HTMLButtonElement>(".session-select"))) button.setAttribute("aria-pressed", String(button.dataset.sessionPath === path));
  }
}
