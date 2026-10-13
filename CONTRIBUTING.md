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

## Candidate resource build and watch (TASK-T009)

The default compiler is official stable `typescript@7.0.2` (TASK-T064), pinned
in the desktop lockfile. `typecheck`, build and watch invoke its local native
`tsc` launcher; a missing or mismatched installation fails without using a
global/older compiler. Subsequent TS migration work uses TS7. Install platform
optional dependencies with `npm ci`; do not omit them. TS7 ships macOS x64/arm64,
Linux x64/arm/arm64 and Windows x64/arm64 binaries (plus other published targets).
The compiler package requires Node >=16.20.0; this desktop toolchain requires
Node >=22.12.0 through Electron's install tooling. Only macOS arm64 was exercised
here; platform package availability does not establish native acceptance.

TS7 removed Node10 resolution, `module: None` and `outFile`. Main now uses
NodeNext/ESM with `import.meta.dirname`; renderer remains ES2022/Bundler with
DOM-only globals. Preload uses ES2022/Bundler with Node-only globals, emits via
TS7 to `dist/preload/`, then pinned esbuild 0.25.12 bundles that emitted JS to
`dist/preload.cjs` with the original TS source map. This is Electron's current
sandbox loading constraint, not backward compatibility; `electron` is the only
external runtime require. No TS5/TS6 compatibility layer or compiler API is used.
tsx 4.20.6 remains the module runtime test runner, using its own esbuild for
transpilation; it is not the typechecker or production compiler.

Verify the installed compiler with `node desktop/node_modules/typescript/bin/tsc --version`.
See [M064](docs/migration/evidence/M064.md) for release sources, compatibility
adjustments and actual validation. M009 below records the earlier TS5 baseline.

After `bash devtools/build-migration.sh`, the desktop's standalone build now
emits JS, source maps with embedded TS sources, the ESM package marker,
the `desktop/renderer/index.html` template and the two existing shared styles:
`crates/rozsa-gui/frontend/ui/styles/foundations/tokens.css` and
`styles/features/sidebar.css`. HTML is copied to
`desktop/dist/browser/renderer/index.html`; CSS to `desktop/dist/browser/ui/`.
The same generator is used for both build and watch; it copies resource bytes
verbatim. No additional templates or UI modules are migrated by this task.

```bash
npm --prefix desktop run build
npm --prefix desktop run build:resources
npm --prefix desktop run typecheck
npm --prefix desktop run test:resources
bash devtools/run-migration.sh --prepare-only
npm --prefix desktop run watch
# In a second terminal, after Watch ready:
bash devtools/run-migration.sh --session-dir /absolute/synthetic/sessions
```

Watch performs an initial TS/resource build, then polls source content every
300 ms, including atomic editor saves and added/removed TS files. HTML/CSS-only
changes regenerate resources; TS/config changes rebuild all three TS targets
serially. `Watch build failed` means the output is not ready; correct the source
and wait for `Watch ready`. Reload the window (`Cmd+R` on macOS, `Ctrl+R` on
Linux/Windows) for HTML/CSS/renderer changes; close and relaunch it for
main/preload changes. Stop watch with Ctrl+C. Go changes still require
`bash devtools/build-migration.sh` and an app restart. Watch does not start,
restart or silently reload the app. Default `./run.sh` continues to use Tauri.

`bash devtools/test-migration-build.sh` is the focused T009/T064 completion target:
fresh build, strict typecheck, resource/map/import loading and watcher tests,
session-list/history checks, then prepare-only. Tests exercise environment
boundaries, ESM output, atomic saves, missing inputs/compiler, rejected older
compiler, type-error non-emission and compiler failure recovery in project
`tmp/`; they do not launch a GUI.
The existing module test dependencies must be installed before direct resource
tests (`npm ci --prefix desktop/js-tests --cache ./tmp/migration/npm-cache`).

```bash
npm --prefix desktop/js-tests run test:native-resources
```

This explicit native test runs the actual candidate entry/preload/Go in an
isolated generated layout under `tmp/migration-native-resource-watch/`. It edits
only its tmp HTML/CSS inputs, waits for watch output, reloads the shown window,
checks the changed heading/computed background and captures before/after images.
It closes its Electron window and verifies its Go backend and watcher exit.
Use host GUI permissions on macOS. Build/watch support `--resource-root` and
`--out-dir` for this isolated verification layout; normal commands use the
repository sources and `desktop/dist/`. Source maps resolve to those TS sources.
See [M009](docs/migration/evidence/M009.md) for measured results and platform limits.

On the migration branch, the user authorized one commit per completed task
starting with TASK-T009: verify, immediately complete it through `dow task done`,
then commit that task's attributable changes. Do not backfill T001–T003, push,
iterate or automatically continue to another task. Stage explicit file paths;
preserve other sessions' working tree and index changes. When first tracking an
existing untracked file changed by the task, disclose the included base content.

## Pull Requests

- Keep changes focused and minimal
- Include tests for new functionality
- Do not edit `CHANGELOG.md` — maintainers handle changelog entries
- Run the full check suite before submitting

## Project Name

Rózsa is named after **Rózsa Péter** (1905-1977), a Hungarian mathematician recognized as the founding mother of recursion theory. Her seminal work *Recursive Functions* (1951) systematized the theory that underpins all of modern computation. The name reflects both the recursive nature of an agent that reasons about and modifies code, and a tribute to a pioneer whose contributions were long underrecognized.

## Questions

Open an issue or reach out to maintainers directly.
