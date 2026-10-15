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

`bash devtools/test-shared-gui-acceptance.sh` runs the focused shared-UI
acceptance checks. It does not replace full-project testing or native visual
verification. Serve the prototype from the repository HTTP root (for example,
`uv run python -m http.server 4173 --bind 127.0.0.1 --directory .`) and open
`/prototype/index.html`; it imports the production UI under
`crates/rozsa-gui/frontend/ui/`. Launch the native app with `./run.sh` and close
validation instances after testing. See [GUI architecture](docs/gui/ARCHITECTURE.md)
for host boundaries and design gaps.

## Current session RPC contract (TASK-T010)

T010 supersedes the initial standard-library-only bootstrap note: Go now pins
`go.lsp.dev/jsonrpc2@v1.0.1` with dependencies in `backend/go.sum`, and Electron
main uses `vscode-jsonrpc@9.0.3`. Use their native connection/framing/ID handling;
do not implement an extra protocol parser or custom Stream. Unsupported library
configuration requirements return to the main chat for a decision.

Business contracts live in `contracts/ipc/session.ts` and Go
`internal/transport/types`. Missing/null empty params are equivalent; required
business fields are validated. Notifications receive no reply and cannot invoke
this slice's product methods. Malformed JSON/envelopes terminate the Go connection
with a diagnostic, without an extra wire response; bad business params in a valid
call return a typed RPC error. UI cancellation does not send product abort or
discard Go transport correlation. Timeouts/exits fail pending calls without
replay. After the shutdown reply, main ends stdin and waits for its owned Go process.

Candidate test temporary files and generated outputs still live under `tmp/`. Run focused checks with:

```bash
npm --prefix desktop/js-tests run test:session-contract
bash devtools/test-migration-session-contract.sh
```

The shell target coordinates Go session/transport tests, strict TS7 typecheck,
fresh Forge package/resources checks, contract/list/history JS tests and the
prepare check. It does not launch a GUI or enter full TEST. See
[M010](docs/migration/evidence/M010.md) for results and remaining slice boundaries.

## Candidate Forge/Vite development and package (TASK-T065)

Pinned tools: Electron 44.7.0, Forge CLI/Vite plugin 8.0.1, Vite 8.3.4 and
official TS7 7.0.2. Node >=22.17.0 is required; validation used Node 26.10.0.
Install exact dependencies with npm ci. Keep npm/Electron caches and TMPDIR
under project tmp/migration/; the build/package/run scripts configure them.

```bash
bash devtools/build-migration.sh
npm --prefix desktop run typecheck
bash devtools/run-migration.sh --session-dir /absolute/synthetic/sessions
bash devtools/package-migration-candidate.sh
npm --prefix desktop run test:resources
bash devtools/run-migration.sh --prepare-only
bash devtools/test-migration-build.sh
```

Forge owns start/package. The trusted launcher runs electron-forge start;
directly use `npm --prefix desktop start -- -- --session-dir /absolute/synthetic/sessions`
with the project cache environment. Hooks first run TS7 strict noEmit and build
the host Go binary. Vite alone transpiles/bundles main, sandbox preload and
renderer. There is no TS emit step or separate default build/watch chain.
Run typecheck separately during TS editing: Vite does not check types on each
edit. tsx remains the module test runner.

Vite consumes the existing desktop/renderer/index.html and shared CSS under
crates/rozsa-gui/frontend/ui/styles/ directly. CSS HMR retains renderer state;
HTML/renderer TS changes automatically reload. Forge reloads preload changes
and restarts Electron for main changes. Go changes require closing and
restarting Forge start. Renderer updates reuse the Go process; close and Forge
SIGTERM use owned backend shutdown. Full renderer reload preserves the existing
candidate reload behavior, not additional cross-reload UI persistence.

Generated bundles/maps live in desktop/.vite/; Forge packages in desktop/out/.
Main/preload follow Forge 8's main.cjs/preload.cjs output. Preload is a single
sandbox bundle with Electron as the only external runtime require. Strict
Bundler-resolution TS configs keep Node globals out of renderer and DOM out of
main/preload. Shared assets are emitted by Vite without copied hand-written
JS/CSS sources.

Forge puts app code in app.asar and the Go binary separately under Resources
(resources on Linux/Windows). Runtime resolves Go using resourcesPath in
package and dist/migration/ in development, independently of launch CWD.
No user session/credential paths are discovered. Native tests pass an isolated
user-data-dir; packaged default storage uses its distinct Rózsa Migration
app userData directory. Foreign platform/architecture packaging is rejected
here so a host Go binary cannot enter an incompatible package. Corresponding
platform tasks own cross-build and native acceptance.

test:resources checks Vite assets/maps/sandbox preload, TS7 noEmit, strict
environment boundaries and compiler failure gates. The completion script also
runs Go session/transport and existing list/history module tests; it does not
launch a GUI or enter full TEST.

```bash
npm --prefix desktop/js-tests run test:native-resources
# macOS package acceptance, after package:
ROZSA_NATIVE_EXECUTABLE="$PWD/desktop/out/Rozsa Migration-darwin-arm64/Rozsa Migration.app/Contents/MacOS/rozsa-migration" npm --prefix desktop/js-tests run test:native
ROZSA_NATIVE_EXECUTABLE="$PWD/desktop/out/Rozsa Migration-darwin-arm64/Rozsa Migration.app/Contents/MacOS/rozsa-migration" npm --prefix desktop/js-tests run test:native-history
```

Native Forge validation edits only copied HTML/CSS/TS inputs under project tmp,
then checks actual Electron automatic updates, controlled bridge, stdio history
and owned process/server cleanup. Package checks reuse existing catalog/history
scenarios against the actual executable from an unrelated CWD. Every validation
instance closes after testing. Use host GUI permissions on macOS. Actual
results and platform limits are in [M065](docs/migration/evidence/M065.md).

T013 extends this resource graph to shared TS/templates and prototype/Tauri
loading; T014 adds Forge makers/installer acceptance after T012/T013.
This task stops at runnable package. Signing, installer, distribution and
Linux/Windows native acceptance remain corresponding later tasks. Historical
emit/watch evidence remains in [M009](docs/migration/evidence/M009.md) and
[M064](docs/migration/evidence/M064.md).

On the migration branch, immediately complete each verified task through dow,
then commit only its attributable changes once. Do not backfill T001–T003,
push, iterate or start the next task automatically. Preserve other sessions'
worktree/index changes; disclose included base content when first tracking an
existing untracked file modified by this task.

## Pull Requests

- Keep changes focused and minimal
- Include tests for new functionality
- Do not edit `CHANGELOG.md` — maintainers handle changelog entries
- Run the full check suite before submitting

## Project Name

Rózsa is named after **Rózsa Péter** (1905-1977), a Hungarian mathematician recognized as the founding mother of recursion theory. Her seminal work *Recursive Functions* (1951) systematized the theory that underpins all of modern computation. The name reflects both the recursive nature of an agent that reasons about and modifies code, and a tribute to a pioneer whose contributions were long underrecognized.

## Questions

Open an issue or reach out to maintainers directly.
