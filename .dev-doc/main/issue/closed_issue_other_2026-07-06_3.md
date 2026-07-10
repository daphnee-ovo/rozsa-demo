---
source: other
nums: 1
---

- [x] ISSUE-I072：codex-oauth requests do not forward ChatGPT account id
  - severity: P0
  - location：crates/rozsa-app/src/agent_session.rs:927
  - description：codex-oauth auth.json credentials are read as bearer tokens, but agent_session does not forward accountId as x-rozsa-account-id, so OpenAI Responses requests cannot set ChatGPT-Account-ID.
  - reproduce：Log in with codex-oauth, select a codex-oauth model, send a message; request path has Authorization but no account id header.
  - fix：Expanded app-layer credential resolution to return OAuth request headers, forwarding codex-oauth accountId as x-rozsa-account-id for OpenAI Responses requests and failing clearly when accountId is missing.
