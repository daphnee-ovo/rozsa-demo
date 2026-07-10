---
source: other
nums: 1
---

- [x] ISSUE-I075：codex-oauth login does not store account id
  - severity: P1
  - location：crates/rozsa-model/src/oauth/openai_codex.rs:233
  - description：Rozsa extracts Codex accountId from access token only; official Codex derives chatgpt_account_id from id_token, so GUI login can store auth.json without accountId and codex-oauth requests fail.
  - reproduce：Run /login in GUI, then send with a codex-oauth model; observe codex-oauth credential is missing accountId.
  - fix：Store codex-oauth idToken from the token response, derive accountId from the official id_token auth claim, and make auth.json account-id reads fall back through idToken/access JWT claims.
