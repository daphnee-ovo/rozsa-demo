---
source: other
nums: 1
---

- [x] ISSUE-I077：GUI: Responses reasoning summary, settings persistence, thinking state, and input chrome glitches
  - severity: P1
  - location：crates/rozsa-gui/frontend/app.js; crates/rozsa-model/src/providers/openai_responses
  - description：GUI has follow-up failures after Codex OAuth integration: Responses replay misses summary item type, selected model does not persist across restart, streaming block cursor renders at the end instead of after latest text, THINKING does not settle to THINKED duration, and focused composer shows an internal divider line.
  - reproduce：Use GUI with codex-oauth model, send a second message after a reasoning response; change model and restart; observe streaming assistant text and focused composer.
  - fix：Added summary_text discriminator for Responses reasoning replay; persisted selected model/provider and thinking level to global settings; aligned GUI settings field names with Rust snapshots; moved stream cursor to latest content block; rendered THINKING/THINKED state; removed textarea inner focus outline.
