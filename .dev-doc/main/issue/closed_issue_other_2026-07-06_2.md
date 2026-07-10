---
source: other
nums: 1
---

- [x] ISSUE-I071：GUI silently renders empty assistant message for provider errors
  - severity: P0
  - location：crates/rozsa-gui/frontend/app.js:281
  - description：When a selected model cannot be used, core emits an assistant message with error_message, but the GUI assistant renderer ignores errorMessage and renders an empty Rozsa bubble.
  - reproduce：Select an unavailable model such as qwen3.5:latest, send a message, and observe a blank Rozsa response instead of the provider error.
  - fix：Rendered assistant errorMessage in the GUI using the existing message layout and understated error styling, so provider failures no longer appear as blank Rozsa replies.
