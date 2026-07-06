---
source: other
nums: 1
---

- [x] ISSUE-I078：Responses replay: tool call ids can serialize as empty call_id
  - severity: P1
  - location：crates/rozsa-model/src/providers/openai_responses/convert.rs
  - description：Responses API rejects follow-up turns when replayed function_call_output has an empty call_id. Stream handling currently falls back missing call_id to an empty string, and replay serializes empty tool ids directly.
  - reproduce：Use codex-oauth with a turn that includes a tool call whose stream delta lacks call_id, then send a follow-up. Provider returns Invalid input call_id empty string.
  - fix：Prevented Responses replay from serializing empty function call ids; stream normalizer now uses item_id only as temporary correlation and replaces it with the non-empty call_id from output_item.done; replay skips empty tool call/result ids; added regressions for empty id replay and item_id-to-call_id normalization.
