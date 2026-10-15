import { sessionSnapshot, type SessionTarget, type SessionSnapshot, type SessionListEntry } from "../../main/session-contract.js";
import { SceneState } from "../scene-state.js";

export interface HistorySource { switchSession(target: SessionTarget, options?: { signal?: AbortSignal }): Promise<SessionSnapshot> }
export class SessionHistory {
  private revision = 0;
  private snapshotRevision = 0;
  private receiving: AbortController | undefined;
  private current: SessionSnapshot | undefined;
  private requested: SessionListEntry | undefined;
  private readonly views = new SceneState();
  constructor(private readonly source: HistorySource, private readonly container: HTMLElement,
    private readonly title: HTMLElement, private readonly status: HTMLElement, private readonly retry: HTMLButtonElement,
    private readonly selected: (path: string) => void = () => {}) {
    retry.addEventListener("click", () => { if (this.requested) void this.switchTo(this.requested); });
  }
  async switchTo(entry: SessionListEntry): Promise<void> {
    const revision = ++this.revision;
    const snapshotRevision = this.snapshotRevision;
    this.receiving?.abort();
    const receiving = this.receiving = new AbortController();
    this.requested = entry;
    this.retry.hidden = true;
    this.status.setAttribute("role", "status");
    this.status.textContent = `Loading history: ${entry.name || entry.id}…`;
    try {
      const target = { path: entry.path, catalogId: entry.id };
      const snapshot = sessionSnapshot(await this.source.switchSession(target, { signal: receiving.signal }), target);
      if (revision !== this.revision || snapshotRevision !== this.snapshotRevision) return;
      this.show(snapshot, entry.name);
    } catch (error) {
      if (revision !== this.revision) return;
      this.status.setAttribute("role", "alert");
      this.status.textContent = `Could not load history: ${error instanceof Error ? error.message : String(error)}`;
      this.retry.hidden = false;
    }
  }
  restore(snapshot: SessionSnapshot | null): void {
    const name = snapshot && this.requested?.path === snapshot.path ? this.requested.name : snapshot && this.current?.path === snapshot.path ? this.title.textContent || undefined : undefined;
    // A canonical snapshot supersedes late success, but must not swallow an
    // error from the user's still-pending switch. New user intent cancels that
    // reception in switchTo; synchronization is not a product/UI abort.
    this.snapshotRevision++;
    this.retry.hidden = true;
    this.status.setAttribute("role", "status");
    if (snapshot) { this.show(snapshot, name); return; }
    if (this.current) this.views.capture(this.current, this.container);
    this.current = undefined;
    this.requested = undefined;
    delete this.container.dataset.sessionId; delete this.container.dataset.sessionPath;
    this.container.replaceChildren(); this.title.textContent = "Session history";
    this.status.textContent = "Select a session to view its history.";
    this.selected("");
  }
  syncFailed(error: unknown, resynchronizing: boolean): void {
    this.status.setAttribute("role", "alert");
    this.status.textContent = `Session sync ${resynchronizing ? "interrupted; resynchronizing" : "failed"}: ${error instanceof Error ? error.message : String(error)}`;
  }
  private show(snapshot: SessionSnapshot, name?: string): void {
    if (this.current) this.views.capture(this.current, this.container);
    this.current = snapshot;
    this.title.textContent = name || snapshot.sessionId;
    this.container.dataset.sessionId = snapshot.sessionId;
    this.container.dataset.sessionPath = snapshot.path;
    this.render(snapshot);
    this.views.restore(snapshot, this.container);
    this.status.textContent = snapshot.messages.length ? `${snapshot.messages.length} history messages · ${snapshot.contextMessageCount} in effective context · Read only` : "No messages yet. · Read only";
    this.selected(snapshot.path);
  }
  private render(snapshot: SessionSnapshot): void {
    this.container.replaceChildren();
    const doc = this.container.ownerDocument;
    for (const message of snapshot.messages) {
      const article = doc.createElement("article");
      article.className = "history-message"; article.dataset.entryId = message.entryId;
      const heading = doc.createElement("h3");
      heading.textContent = message.role === "toolResult" ? `Tool result · ${message.toolName}${message.isError ? " · Error" : ""}` : message.role === "user" ? "You" : "Assistant";
      article.append(heading);
      message.content.forEach((block, index) => {
        if (block.type === "thinking") {
          const detail = doc.createElement("details"); detail.dataset.block = JSON.stringify([message.entryId, index]);
          const summary = doc.createElement("summary"); summary.textContent = "Thinking";
          const text = doc.createElement("p"); text.textContent = block.text ?? "";
          detail.append(summary, text); article.append(detail);
        } else {
          const text = doc.createElement("p");
          text.textContent = block.type === "text" ? block.text ?? "" : block.type === "image" ? "[Image — full preview pending]" : `[Tool call: ${block.name} — full details pending]`;
          article.append(text);
        }
      });
      this.container.append(article);
    }
  }
}
