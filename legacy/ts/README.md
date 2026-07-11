# Legacy TypeScript Code

This directory contains the original TypeScript implementation that has been superseded by the Rust rewrite in `crates/`.

It is preserved as reference only. No new development should happen here.

The Rust implementation lives in:
- `crates/rozsa-model` — LLM provider abstraction
- `crates/rozsa-core` — agent loop
- `crates/rozsa-app` — application layer (AgentSession)
- `crates/rozsa-tui` — terminal UI
- `crates/rozsa-cli` — CLI entry point
