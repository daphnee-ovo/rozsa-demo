import type { SessionSnapshot } from "../main/session-contract.js";

interface HistoryView { scrollTop: number; expanded: Set<string>; focusedBlock?: string }
// Candidate-owned view state. Loaded messages and effective context are owned by Go.
export class SceneState {
  private readonly views = new Map<string, HistoryView>();
  private key(session: SessionSnapshot): string { return JSON.stringify([session.path, session.sessionId]); }
  capture(session: SessionSnapshot, container: HTMLElement): void {
    const expanded = new Set<string>();
    for (const detail of Array.from(container.querySelectorAll<HTMLDetailsElement>("details[data-block]"))) {
      if (detail.open) expanded.add(detail.dataset.block!);
    }
    const active = container.ownerDocument.activeElement?.closest<HTMLElement>("[data-block]");
    this.views.set(this.key(session), { scrollTop: container.scrollTop, expanded,
      ...(active && container.contains(active) ? { focusedBlock: active.dataset.block } : {}) });
  }
  restore(session: SessionSnapshot, container: HTMLElement): void {
    const saved = this.views.get(this.key(session));
    for (const detail of Array.from(container.querySelectorAll<HTMLDetailsElement>("details[data-block]"))) {
      detail.open = saved?.expanded.has(detail.dataset.block!) === true;
      if (saved?.focusedBlock === detail.dataset.block) detail.querySelector("summary")?.focus({ preventScroll: true });
    }
    container.scrollTop = saved ? Math.min(saved.scrollTop, Math.max(0, container.scrollHeight - container.clientHeight)) : container.scrollHeight;
  }
}
