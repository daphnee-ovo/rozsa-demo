# Native TUI

Pi's interactive mode uses a Rust terminal frontend built with Ratatui and Crossterm. The TypeScript runtime still owns sessions, tools, providers, permissions, extensions, and persistence. The Rust process only owns terminal rendering and keyboard input.

## Run

From a development checkout:

```bash
cargo build --manifest-path packages/tui-rs/Cargo.toml
TMPDIR="$PWD/temp" ./pi-test.sh --tui-backend rust
```

`rust` is the default interactive backend, so this is equivalent:

```bash
TMPDIR="$PWD/temp" ./pi-test.sh
```

To test the built CLI:

```bash
npm run build
TMPDIR="$PWD/temp" node packages/coding-agent/dist/cli.js --tui-backend rust
```

To use a specific Rust TUI binary:

```bash
PI_NATIVE_TUI_PATH=/absolute/path/to/pi-tui-rs pi
```

## Fallback

Use the TypeScript TUI when debugging old extension component UI or comparing behavior:

```bash
pi --tui-backend typescript
```

The environment variable works too:

```bash
PI_TUI_BACKEND=typescript pi
```

## Keyboard

| Key | Action |
| --- | --- |
| `Enter` | Submit input. While the agent is streaming, send it as steering input. |
| `Alt-Enter` | Queue a follow-up message. |
| empty `Enter` | Exit interactive mode. |
| `!command` then `Enter` | Run a local bash command through the existing TypeScript backend. |
| `!!command` then `Enter` | Run bash and exclude the result from session context. |
| `Tab` | Request slash command, model argument, path, or `@file` autocomplete. |
| `Esc` / `Ctrl-C` | Abort the current request. |
| `Ctrl-D` | Exit. |
| `Ctrl-P` / `Ctrl-N` | Cycle model forward/backward. |
| `Ctrl-T` | Cycle thinking level when the current model supports thinking. |
| `Ctrl-E` | Cycle edit mode. |
| `PageUp` / `PageDown` | Scroll the conversation. |
| `Home` / `End` | Move the input cursor. |
| `Left` / `Right` | Move within the input line. |

Rust receives the effective keybinding map from `KeybindingsManager`; the table above describes defaults, not hardcoded keys.

Slash commands still go through the TypeScript backend. Native parity is implemented for `/help`, `/hotkeys`, `/permissions`, `/session`, `/name`, `/model`, `/scoped-models`, `/export`, `/import`, `/share`, `/copy`, `/tree`, `/fork`, `/clone`, `/new`, `/compact`, `/reload`, `/changelog`, `/lsp`, `/resume`, `/gc`, `/search`, `/quit`, `/main`, `/subagent`, `/subagents`, and `/graph`. Extension commands, prompt templates, and skill commands continue through `AgentSession.prompt()`.

Inline `@file` references are expanded before normal prompts. Text files are prepended as `<file>` blocks and supported image files are attached through the existing image resize path. Nonexistent `@` tokens are left as plain text.

## Extension UI

The native backend keeps the extension lifecycle and command APIs in TypeScript. The supported native UI bridge is intentionally serializable:

- `ctx.ui.select()`
- `ctx.ui.confirm()`
- `ctx.ui.input()`
- `ctx.ui.editor()`
- `ctx.ui.notify()`
- `ctx.ui.setStatus()`
- `ctx.ui.setWidget()` with `string[]`
- `ctx.ui.setTitle()`
- `ctx.ui.setEditorText()`
- `ctx.ui.pasteToEditor()`
- `ctx.ui.addAutocompleteProvider()` wrappers around the default native autocomplete provider
- theme lookup and theme switching by name

The old TypeScript component APIs are not rendered by the Rust backend:

- `ctx.ui.custom()`
- `ctx.ui.setHeader()` with a component factory
- `ctx.ui.setFooter()` with a component factory
- `ctx.ui.setWidget()` with a component factory
- `ctx.ui.setEditorComponent()`
- raw terminal input listeners

Use `--tui-backend typescript` for extensions that still depend on those APIs. New native extension UI should use serializable dialog, status, and widget messages until a dedicated Rust extension UI API is added.

## Built-in Panels

The native TUI implements built-in panels that are part of Pi's core interactive flow:

- Permission requests render as a native approval panel and return choices to the existing TypeScript `PermissionManager`.
- `/graph` opens a native session graph panel using session entries from the TypeScript backend.
- `/tree` uses the existing session tree and `navigateTree()` backend path.
- `/subagents` switches the visible message stream and forwards input/interrupts to the selected subagent through `AgentSession`.
- Runtime sidebar data comes from `RuntimeStateSnapshot`; Rust should not query git, tools, models, or subagents directly.

## Parity Status

Completed native parity:

- Rust rendering for chat, pending messages, extension text widgets, input, footer, notifications, runtime sidebar, dialogs, permission panel, graph panel, and autocomplete popup.
- Host-backed keymap mapping from `KeybindingsManager`, used by input, dialogs, permission, graph, and autocomplete selection.
- Built-in commands listed in the Keyboard section, including session export/import/share/copy/search/GC/resume and subagent view routing.
- `@file` autocomplete and prompt-time file expansion.
- Serializable extension UI primitives: select, confirm, input, editor, notify, status, string-array widgets, title, input text updates, theme lookup/switching, and autocomplete provider wrappers.

Remaining native parity gaps:

- `/settings`, `/login`, and `/logout` still need dedicated native selector/dialog flows.
- External editor handoff (`app.editor.external`) is not implemented in Rust mode yet.
- Full old graph/tree component behavior is partial: graph markdown rendering, tree filter controls, folding, labels, and summarize-on-navigation prompts still need native equivalents.
- Extension-owned TypeScript component factories, `ctx.ui.custom()`, custom header/footer/editor components, and raw terminal input listeners need a future serializable Rust extension UI API.

## Architecture

The native UI owns the interactive process. In normal Rust mode, the TypeScript CLI delegates to the Rust TUI launcher; Rust creates a socket path under the project `temp/` directory, starts the TypeScript backend in backend-only mode, then connects to that socket. The wire protocol is JSON Lines: TypeScript sends one JSON object per line to Rust, and Rust sends user actions back as one JSON object per line.

High-level flow:

1. The TypeScript CLI starts `rozsa-tui` when `--tui-backend rust` is selected.
2. Rust creates `temp/rozsa-native-tui-*.sock`.
3. Rust starts the TypeScript CLI again with `ROZSA_NATIVE_TUI_BACKEND_ONLY=1` and `ROZSA_NATIVE_TUI_SOCKET=<socket>`.
4. The TypeScript backend creates `AgentSessionRuntime`, listens on the socket, and waits for Rust to connect.
5. TypeScript streams `state`, `dialog`, `autocomplete`, `permission`, `graph`, `notify`, `set_input`, and `set_title` messages.
6. Rust sends `submit`, `follow_up`, `autocomplete_request`, `abort`, model cycling, thinking cycling, dialog responses, and permission responses.

This keeps the backend stable while allowing the terminal UI to be replaced completely.

## Development Notes

The Rust crate has its own development document at `packages/tui-rs/README.md`. Update it when changing:

- Rust module boundaries
- JSONL protocol messages
- supported extension UI APIs
- built-in panels
- testing commands or known verification limits
