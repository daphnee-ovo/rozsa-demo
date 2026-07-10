---
source: other
nums: 1
---

- [x] ISSUE-I076：codex-oauth models use Platform endpoint instead of Codex backend
  - severity: P1
  - location：crates/rozsa-model/src/models_endpoint.rs:120
  - description：Rozsa generated codex-oauth model configs with https://api.openai.com/v1. Official Codex routes ChatGPT/PAT auth to https://chatgpt.com/backend-api/codex, so Rozsa sends ChatGPT OAuth tokens to the Platform Responses API and receives 401 missing api.responses.write.
  - reproduce：Run GUI /login, send with a codex-oauth model, observe Provider HTTP error 401 missing api.responses.write.
  - fix：Route generated codex-oauth model configs to https://chatgpt.com/backend-api/codex, matching official Codex ChatGPT/PAT auth routing, and bump fallback config version so existing generated codex-oauth.json files migrate.
