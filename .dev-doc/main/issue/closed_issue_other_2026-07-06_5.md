---
source: other
nums: 1
---

- [x] ISSUE-I073：GUI migration: auth commands, invalid model send, and status bar git layout
  - severity: P1
  - location：crates/rozsa-gui
  - description：GUI lacks login/logout command wiring, rejects stale unsupported model selection before send, and status bar shows cwd basename instead of git branch with dirty marker while diff stats overlap quota.
  - reproduce：Use /login or /logout in GUI; select stale gpt-4o; inspect sidebar status in a dirty git worktree.
  - fix：Wired GUI /login and /logout commands, allowed custom stale model ids to send and surface provider errors, rendered prompt errors into chat, fixed AgentSession running cleanup on pre-loop failures, and added git branch/diff status snapshot plus sidebar spacing.
