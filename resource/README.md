# Resource Templates

Text templates loaded by `packages/coding-agent/src/core/system-prompt.ts`.

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

- Runtime loading prefers `./resource/system-prompt.md`.
- Binary packaging copies `../../resource/*.md` into `dist/resource/`.
