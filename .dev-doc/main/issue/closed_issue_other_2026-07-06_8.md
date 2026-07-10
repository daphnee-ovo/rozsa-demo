---
source: other
nums: 1
---

- [x] ISSUE-I077：Responses replay omits reasoning summary
  - severity: P1
  - location：crates/rozsa-model/src/providers/openai_responses/convert.rs
  - description：Second codex-oauth turn can fail with Missing required parameter input[n].summary because prior assistant reasoning content is replayed without the summary field required by the ChatGPT Codex Responses backend.
  - reproduce：Send two messages in GUI using a codex-oauth Responses model; observe Provider HTTP error 400 Missing required parameter input[2].summary.
  - fix：Make Responses reasoning replay always serialize the required summary field. Assistant Thinking blocks now replay as reasoning summary parts when text exists, or summary: [] when only encrypted_content exists.
