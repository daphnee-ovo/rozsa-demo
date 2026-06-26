# pi-tui-rs

Rust terminal frontend for Pi interactive mode. Rendering is Ratatui + Crossterm. The TypeScript backend still owns sessions, tools, models, permissions, extensions, slash commands, persistence, and settings.

## File Layout

- `src/main.rs`: entrypoint only.
- `src/app.rs`: process lifecycle, Unix socket reader, shared app state.
- `src/protocol.rs`: JSONL protocol structs shared with `packages/coding-agent/src/modes/native/protocol.ts`.
- `src/keymap.rs`: Crossterm key events matched against serialized Pi keybindings.
- `src/input.rs`: keyboard dispatch for global input, dialogs, graph, and permission prompts.
- `src/autocomplete.rs`: autocomplete popup state and completion application.
- `src/ui.rs`: main chat layout, header, footer, editor, dialogs.
- `src/sidebar.rs`: runtime sidebar rendered from `RuntimeStateSnapshot` and `ContextUsage`.
- `src/permission.rs`: native permission approval panel.
- `src/graph.rs`: native session graph panel.

Keep each source file below 500 lines. Split by feature when a file approaches that limit.

## Runtime Contract

Node starts `pi-tui-rs` as a child process and sets `PI_NATIVE_TUI_SOCKET`. Rust connects to that Unix socket and both sides exchange one JSON object per line.

Host to Rust:

- `state`: full render snapshot: messages, pending input, widgets, stats, runtime sidebar state, context usage.
- `dialog`: serializable select, confirm, input, and editor dialogs.
- `autocomplete`: slash, model, path, and `@file` suggestions produced by the TypeScript provider.
- `permission`: permission request with request/context details and precomputed trust levels.
- `graph`: session graph nodes.
- `notify`: transient status line entry.
- `set_title`: terminal title.
- `set_input`: replace the input editor text.
- `shutdown`: exit native UI.

Rust to host:

- `submit`: user submitted editor text.
- `follow_up`: user explicitly queued a follow-up.
- `autocomplete_request`: request suggestions for the current editor text and cursor.
- `abort`: abort current agent work.
- `cycle_model`, `cycle_thinking`, `cycle_edit_mode`: forward shortcuts to the backend.
- `dialog_response`: response for `dialog`.
- `permission_response`: response for `permission`.
- `exit`: close interactive mode.

When adding a message, update both protocol files in the same change.

## Parity Targets

Implemented native surfaces:

- Chat stream, pending messages, widgets above/below editor, input editor, footer, runtime sidebar.
- Permission approval flow, including approve once, trust for session, reject, and reject with alternative.
- Session graph via `/graph`, plus built-in `/tree`, `/model`, `/session`, `/permissions`, `/help`, `/hotkeys`, `/name`, `/fork`, `/clone`, `/new`, `/compact`, `/reload`, `/main`, `/subagent`, `/subagents`, and `/quit` dispatch through the TypeScript backend.
- Default slash command, model argument, path, and `@file` autocomplete. Extension autocomplete provider wrappers are applied on top of the default provider.
- Inline `@file` expansion for normal prompts, including text blocks and supported image attachments.
- Extension UI primitives that serialize cleanly: select, confirm, input, editor, notify, status, string-array widgets, title, input text updates, theme lookup/switching by name.

Not implemented as native parity yet:

- Arbitrary TypeScript `Component` factories from extensions.
- `ctx.ui.custom()` overlays and extension games.
- Custom header/footer component factories.
- Custom editor components.
- Raw terminal input listeners.
- External editor handoff.
- Full original graph markdown rendering and tree labeling/filter controls.

Those APIs need a dedicated serializable Rust extension UI contract instead of trying to execute TypeScript components inside Rust.

## Development

Build Rust:

```bash
cargo build --manifest-path packages/tui-rs/Cargo.toml
```

Format Rust:

```bash
cargo fmt --manifest-path packages/tui-rs/Cargo.toml
```

Run Pi with the native TUI from the repo root:

```bash
TMPDIR="$PWD/temp" ./pi-test.sh --tui-backend rust
```

Use the TypeScript TUI for comparison:

```bash
TMPDIR="$PWD/temp" ./pi-test.sh --tui-backend typescript
```

Full repo check after code changes:

```bash
npm run check
```

If `tsx` cannot create an IPC pipe under sandboxed execution, run the same command with approval and keep `TMPDIR="$PWD/temp"`.

## Design Rules

- Rust is a frontend only. Do not move agent/session/tool/provider logic into Rust.
- Prefer serializable protocol state over callbacks.
- Keep UI state minimal in Rust; backend state should remain authoritative.
- Do not leave build artifacts tracked. `packages/tui-rs/target/` is ignored.
- Temporary files belong in the repo `tmp/` directory.
