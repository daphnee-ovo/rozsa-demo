---
source: other
nums: 1
---

- [x] ISSUE-I069：OAuth auth.json fallback panics inside Tokio runtime
  - severity: P0
  - location：crates/rozsa-app/src/agent_session.rs:960
  - description：Submitting a GUI message can panic because resolve_api_key calls Handle::block_on while already running inside Tokio runtime; auth.json fallback should also be limited to OAuth-backed providers.
  - reproduce：Run target/debug/rozsa, select qwen3.5:latest, send a message; terminal prints Cannot start a runtime from within a runtime and the main message pane stays empty.
  - fix：Made app-layer API key resolution async, removed Tokio Handle::block_on, and limited auth.json fallback to OAuth-backed providers only.
