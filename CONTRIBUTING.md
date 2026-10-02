# Contributing to Rózsa

## Development Tools

Development and synchronization tools live in `devtools/`.

### Synchronize the Codex models client version

The ChatGPT models endpoint filters its response using Codex CLI version semantics. Rózsa therefore keeps a dedicated `CODEX_MODELS_CLIENT_VERSION`; do not replace it with the Rózsa workspace version.

Synchronize the compatibility version directly from the newest valid `rust-v*` tag in the `openai/codex` GitHub repository:

```bash
./devtools/sync-codex-model-client-version.sh
```

The tool uses `git ls-remote` and does not require a local Codex checkout. Use `--check` in CI or review workflows to verify that the checked-in constant is already current. Prerelease suffixes such as `-alpha.30` and `-beta.2` are intentionally removed because the endpoint expects the whole `major.minor.patch` client version used by Codex's models manager. `--repo-url` exists for mirrors and automated tests; normal development should use the default GitHub repository.


## Code Style

- Rust: standard `rustfmt` and `clippy`
- Do not add or maintain file-header `FrameworkTree` comments or run `make-tree`. Existing tree comments may remain.

## Testing

Use the implementation language directly for tests of a small feature. Keep
JavaScript tests in the corresponding module's `js-tests/` directory; do not
wrap Node tests in Rust or Cargo subprocesses. GUI JavaScript tests live in
`crates/rozsa-gui/js-tests/`:

```bash
npm ci --prefix crates/rozsa-gui/js-tests
npm test --prefix crates/rozsa-gui/js-tests
```

Prefer shell scripts in `devtools/` to coordinate whole-module integration
and end-to-end tests across languages. `bash devtools/test-gui.sh` runs the GUI
JavaScript suite and Rust library/integration tests; `--js` or `--rust` runs one
language. DOM and logic tests do not establish visual parity or native Tauri
behavior; verify those in the actual prototype and app.

## Pull Requests

- Keep changes focused and minimal
- Include tests for new functionality
- Do not edit `CHANGELOG.md` — maintainers handle changelog entries
- Run the full check suite before submitting

## Project Name

Rózsa is named after **Rózsa Péter** (1905-1977), a Hungarian mathematician recognized as the founding mother of recursion theory. Her seminal work *Recursive Functions* (1951) systematized the theory that underpins all of modern computation. The name reflects both the recursive nature of an agent that reasons about and modifies code, and a tribute to a pioneer whose contributions were long underrecognized.

## Questions

Open an issue or reach out to maintainers directly.
