# Migration residue audit

Date: 2026-07-30

## Scope and method

This audit covers tracked active-product files after removal of `legacy/`, the
terminal frontend, and migration-only documents. It intentionally excludes the
deleted `legacy/` and `packages/` trees plus `.dev-doc/`, whose archive and
historical task records are managed development evidence rather than supported
product documentation.

The scan used:

```text
git grep -n -i -E '(ratatui|crossterm|rozsa-tui|tui-rs|\btui\b|legacy|typescript|\bts\b|bridge|originator=pi|\bpi\b)' -- ':!legacy/**' ':!packages/**' ':!.dev-doc/**' ':!docs/MIGRATION_RESIDUE_AUDIT.md'
```

No active runtime path references a terminal frontend, a TypeScript package, or
a process bridge. The removed `devtools/smoke-anthropic-rust.mts` script was the
last active-file import of the retired `packages/ai` bridge.

## Intentional retained matches

| Match | Locations | Classification and reason |
| --- | --- | --- |
| `originator=pi` | `crates/rozsa-model/src/oauth/openai_codex.rs`, `crates/rozsa-model/tests/oauth_openai_codex.rs` | External OpenAI Codex authorization-URL query parameter and its regression assertion. Kept by explicit product decision because changing it can affect new OAuth logins. |
| `originator=pi` documentation | `docs/TODO.md`, `docs/model/oauth-architecture.md` | Required follow-up record. It identifies the source and test, requires upstream compatibility evidence and an authorization-flow regression before any replacement. |
| `--tui` | `crates/rozsa-cli/tests/argument_contract_test.rs` | Negative CLI contract test: it proves the removed terminal flag is rejected, rather than supporting a terminal code path. |
| `TypeScript` / `bridge` in guard text | `AGENTS.md`, `docs/model/oauth-architecture.md` | Negative architectural constraints: they prohibit reintroducing retired implementations and do not describe a supported dependency. |
| `.ts` / `TypeScript` in the GUI prototype | `docs/gui/prototype/prototype.js` | User-facing language/file-pattern support. This match is unrelated to the retired project implementation. |

## Removed categories

- Retired `legacy/` source tree and terminal-specific Cargo dependencies.
- Terminal CLI flag, tests, launcher, and TUI comparison/audit documents.
- TypeScript migration plans, bridge smoke script, and framework/difference
  records that no longer describe a supported architecture.
- Migration-only wording in active runtime comments and documentation.

## Maintenance rule

Run the scan above after architecture-wide cleanup. A future match is allowed
only when it is either product-domain text (for example a source-file extension)
or an explicitly documented external compatibility requirement. Otherwise,
remove it or record a time-bounded follow-up before merging.

## Verification (2026-07-30)

| Check | Result | Notes |
| --- | --- | --- |
| `cargo fmt --all -- --check` | Pass | No formatting drift. |
| `cargo build` | Pass | All five active crates build. |
| `cargo test` | Pass | Full workspace suite passed. Existing `edit_fuzzy_test` unused-import warnings remain non-fatal. |
| `cargo clippy --all-targets -- -D warnings` | Existing failure | Fails on 36 lint findings in pre-existing credential/provider expressions (for example `collapsible_if`), including lines outside every cleanup diff. They are recorded rather than mechanically rewritten because that would expand this cleanup into an unrelated provider refactor. |
| `git diff --check` | Pass | No whitespace errors. |
| Removed-path and migration-term scans | Pass with classified exceptions | No active terminal frontend, TypeScript package, or process-bridge dependency remains; the `originator=pi` and rejected `--tui` test exceptions are classified above. |
| Markdown internal links affected by this cleanup | Pass | Deleted migration-document identifiers have no active documentation references; retained links point to current crate and GUI documentation. |
