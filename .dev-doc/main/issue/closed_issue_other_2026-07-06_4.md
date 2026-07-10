---
source: other
nums: 1
---

- [x] ISSUE-I073：codex-oauth fallback models are stale
  - severity: P1
  - location：crates/rozsa-tui/src/backend/native.rs:2208
  - description：codex-oauth fallback model config still seeds old GPT-4/o3/o4 models instead of official bundled Codex catalog.
  - reproduce：Run /login codex-oauth on a fresh config and inspect ~/.rozsa/models/codex-oauth.json.
  - fix：Updated codex-oauth fallback models from official codex-rs bundled catalog, migrated exact legacy generated configs, and filtered hidden /wham models.
