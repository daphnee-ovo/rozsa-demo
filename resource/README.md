# Resource Templates

Text templates compiled into `rozsa-app` by `crates/rozsa-app/src/resources/mod.rs`.

## Files

- [`system-prompt.md`](./system-prompt.md): core system prompt template for the coding agent

## Template placeholders

Current placeholders:

- `{{AVAILABLE_TOOLS}}`
- `{{GUIDELINES}}`
- `{{README_PATH}}`
- `{{DOCS_PATH}}`
- `{{EXAMPLES_PATH}}`

## Build notes

- `include_str!` embeds the built-in system prompt in the Rust binary.
- Project and user instruction files are loaded by `ResourceLoader` at runtime.
