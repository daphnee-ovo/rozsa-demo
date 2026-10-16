# Contributing to Rózsa

## 后端命名规范

- 变量命名需要清晰表达语义，做到自解释。必须使用完整单词或通用缩写，如`name`,`cnt`,`msg`,`dir`,在循环中使用`i` or `j`; 禁止使用语义晦涩的缩写，如用`h`代替`header`。


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
wrap Node tests in Rust or Cargo subprocesses. All Electron JS/TS tests—main,
preload, bridge, renderer and native host—live in `desktop/tests/`. The retained
Rust/Tauri GUI owns a separate JS behavior suite and lockfile:

```bash
npm ci --prefix crates/rozsa-gui/js-tests
bash devtools/test-gui.sh
# New frontend and prototype, independently:
npm ci --prefix desktop/tests
npm --prefix desktop/tests run test:ui
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
`/prototype/index.html`; it reuses the production UI under
`desktop/renderer/`. Rust/Tauri independently embeds
`crates/rozsa-gui/frontend/index.html`, `sidebar.html` and their local resources.
`./run.sh --prepare-only` builds the Rust bundle without Node, Vite, desktop,
backend or prototype inputs; `./run.sh` launches it. Close validation instances after testing. See [GUI architecture](docs/gui/ARCHITECTURE.md)
for host boundaries and design gaps.

## Electron + Go development and acceptance

The Electron + Go implementation is the target Rózsa desktop application. Until
T061, the current `./run.sh` still launches Tauri. Run the combined desktop
acceptance target with:

```bash
bash devtools/test-desktop.sh
```

Go dependencies and Electron/TypeScript/tsx/jsdom versions are defined by their module manifests and lockfiles. Use those files as the single source of version truth.

The combined script is the shared T001/T003/T010 cross-language task target. It
runs Go session/transport checks, list/history/RPC-contract module tests and one
resource prepare check; Forge performs the package, TypeScript check and Go build
once. While developing a slice, Go tests can run directly with
`go -C backend test -count=1 ./internal/app/session/... ./internal/transport/...`,
and JS module tests with `npm --prefix desktop/tests run test:session-list`,
`session-history`, or `npm --prefix desktop/tests run test:session-contract`.
Native acceptance remains separate; it never enters full TEST or starts a GUI
from the task completion hook. This uses dow's supported Shell adapter because
dow 0.3.10 does not recognize the tsx TypeScript test runner.

Launch against explicit synthetic directories in layer order:

```bash
npm --prefix desktop start -- -- --user-data-dir="$PWD/tmp/migration/electron-profile" --session-dir /absolute/synthetic/global --session-dir /absolute/synthetic/project
```

The renderer cannot supply paths or arbitrary IPC. No default user session or
credential directories are discovered. Missing configured layers return an
empty list; no configured layers show the existing `No session directories
configured` error. Closing the last candidate window shuts down its owned Go
process. Session rows select Go-owned Loaded tabs via controlled `switch_session`;
the renderer shows active-branch display history, separately from effective
model context, and retains scroll/Thinking expansion on re-entry. The Go session
module provides read-only `Open`, `Entries`, and `ContextMessages`, plus a writable
`Manager` for internal JSONL persistence; desktop RPC remains read-only and agent
startup is not connected yet. History is a text projection;
full Markdown, image/tool content and running sessions remain later slices.
See [M003](docs/migration/evidence/M003.md) for identity, errors and coverage.

T002's focused reader checks consume the same small fixtures in Go and Rust:

```bash
go -C backend test ./internal/app/session/...
cargo test -p rozsa-app --test session_reader_reference_test --test agent_session_restore_test
```

See [M002](docs/migration/evidence/M002.md) for fixture origins, restoration rules
and the remaining session tasks. These checks do not enter full TEST.

`npm --prefix desktop/tests run test:native` opens its own Electron window
with generated fixtures and an isolated profile under `tmp/`. It checks real
rendering, refresh, restricted preload, unchanged JSONL, and normal shutdown
with no owned Go process left. It writes a screenshot, result JSON and logs to
the displayed evidence directory, then closes its instance. Run it with host
permissions where the macOS application sandbox prevents GUI startup. This is
focused slice acceptance; DOM tests do not prove native behavior, and the macOS
result does not verify Linux/Windows or complete migration parity. See
[TASK_ROADMAP](docs/migration/TASK_ROADMAP.md) and [M001](docs/migration/evidence/M001.md).

`npm --prefix desktop/tests run test:native-history` likewise starts and closes
only its isolated Electron/Go instance. It exercises actual selection, branch
history versus context count, empty history, reader failure and manual retry,
scroll/expansion re-entry, and unchanged JSONL bytes/mtime. It repairs only its
own deliberately invalid synthetic input for the retry case. Results and four
screenshots live under `tmp/migration-native-session-history/`; macOS evidence
does not establish Linux/Windows or full native shell parity. Native runs need
host GUI permissions and are separate from task completion and full TEST.

## Current session RPC checks (TASK-T010)

The approved library, framing, error, identity, and cancellation decisions are in
the migration [SPEC](.dev-doc/refactor/electron-go-migration/SPEC.md) and
[M010](docs/migration/evidence/M010.md). Implement them with the native library
connections; do not add a second protocol parser or custom Stream. Run focused
checks with:

```bash
npm --prefix desktop/tests run test:session-contract
```

The combined `bash devtools/test-desktop.sh` target includes this module
test alongside Go session/transport, list/history and resource checks. It does
not launch a GUI or enter full TEST. See
[M010](docs/migration/evidence/M010.md) for results and remaining slice boundaries.

## Desktop Forge/Vite development and package (TASK-T065)

Tool versions and the supported Node range are defined by `desktop/package.json`
and its lockfile. Install the declared dependencies with `npm ci`; build/package/run
scripts place temporary working files under project `tmp/migration/`.

```bash
bash devtools/build-desktop.sh
npm --prefix desktop start -- -- --user-data-dir="$PWD/tmp/migration/electron-profile" --session-dir /absolute/synthetic/sessions
bash devtools/package-desktop.sh
bash devtools/package-desktop.sh make
bash devtools/test-desktop.sh
```

`test-desktop.sh` resolves the repository root from its own location, validates
the module manifests, packages the current tree and runs the `desktop/tests`
behavior suite plus Go/resource checks. Run it from a tree containing the
reviewed task changes and declared dependency inputs.

Forge owns start/package. The trusted launcher runs electron-forge start;
directly use `npm --prefix desktop start -- -- --session-dir /absolute/synthetic/sessions`
Hooks first run the local strict noEmit typecheck and build
the host Go binary. Vite transpiles/bundles main, sandbox preload and
renderer. There is no TS emit step or separate default build/watch chain.
Run typecheck separately during TS editing: Vite does not check types on each
edit. tsx remains the module test runner.

The shared HTML/CSS and migrated TypeScript source lives in `desktop/renderer/`; all frontend and
Electron behavior/native tests live in `desktop/tests/`, with one test manifest,
lockfile and runner. Only the new prototype and Electron consume these assets.
The old GUI owns its frontend, JS tests, Rust tests and static resource graph;
neither project builds, runs or tests against the other project's assets.
Vite packages only the candidate entry. T013 resumes in the final
`desktop/renderer/` tree: it converts shared core/port and prototype loading to
TypeScript. The HTTP prototype and Electron share Vite-generated renderer
assets; Rust/Tauri uses its independent frontend under `crates/rozsa-gui/`.
CSS HMR retains renderer state;
HTML/renderer TS changes automatically reload. Forge reloads preload changes
and restarts Electron for main changes. Go changes require closing and
restarting Forge start. Renderer updates reuse the Go process; close and Forge
SIGTERM use owned backend shutdown. Full renderer reload preserves the existing
candidate reload behavior, not additional cross-reload UI persistence.

Generated bundles/maps live in desktop/.vite/; Forge packages in desktop/out/.
`npm --prefix desktop run build:renderer` builds the production renderer for
resource inspection; `npm --prefix desktop/tests run test:ui` includes the
shared host-contract and HTTP prototype resource checks.
Main/preload follow Forge's main.cjs/preload.cjs convention. Preload is a single
sandbox bundle with Electron as the only external runtime require. Strict
Bundler-resolution TS configs keep Node globals out of renderer and DOM out of
main/preload. Shared assets are emitted by Vite without copied hand-written
JS/CSS sources.

Forge puts app code in app.asar and the Go binary separately under Resources
(resources on Linux/Windows). Runtime resolves Go using resourcesPath in
packages and dist/rozsa-backend in development, independently of launch CWD.
No user session/credential paths are discovered. Native tests pass an isolated
user-data-dir; packaged default storage uses its Rózsa
app userData directory. Foreign platform/architecture packaging is rejected
here so a host Go binary cannot enter an incompatible package. Corresponding
platform tasks own cross-build and native acceptance.

test:resources checks Vite assets/maps/sandbox preload, strict environment
boundaries and compiler failure gates. The completion script runs this resource
check once through `--prepare-only`, alongside Go session/transport and the
list/history/contract module tests; it does not launch a GUI or enter full TEST.

```bash
npm --prefix desktop/tests run test:native-resources
# macOS package acceptance, after package:
ROZSA_NATIVE_EXECUTABLE="$PWD/desktop/out/Rózsa-darwin-arm64/Rózsa.app/Contents/MacOS/rozsa" npm --prefix desktop/tests run test:native
ROZSA_NATIVE_EXECUTABLE="$PWD/desktop/out/Rózsa-darwin-arm64/Rózsa.app/Contents/MacOS/rozsa" npm --prefix desktop/tests run test:native-history
ROZSA_TEST_APP_BUNDLE="/Applications/Rózsa.app" npm --prefix desktop/tests exec -- tsx desktop/tests/package-resources.test.ts
```

Native Forge validation edits only copied HTML/CSS/TS inputs under project tmp,
then checks actual Electron automatic updates, controlled bridge, stdio history
and owned process/server cleanup. Package checks reuse existing catalog/history
scenarios against the actual executable from an unrelated CWD. The package
resource check inspects the app archive, runs the packaged entry with its Go
resource missing in a temporary copy, and verifies the startup diagnostic. Every validation
instance closes after testing. Use host GUI permissions on macOS. Actual
results and platform limits are in [M065](docs/migration/evidence/M065.md).

T060 establishes the final frontend source and test paths before T013 converts
shared core/port and prototype TS loading; T014 adds Forge makers/installer
acceptance after T012/T013.
Forge `make` creates a macOS DMG, Windows Squirrel installer set, and Debian
package. Build the host installer with `bash devtools/package-desktop.sh make`;
the default command above still creates the runnable app package. T014 checks
the installer contents and host install/runtime resources. Signing, actual
install/uninstall acceptance and Linux/Windows native acceptance remain
T056/T057/T058. Current package behavior and its limits are summarized in
[M065](docs/migration/evidence/M065.md).

On the migration branch, after focused verification run `dow task done` before
committing only that task's attributable changes once. Do not backfill T001–T003,
push, iterate or start the next task automatically. Preserve other sessions'
worktree/index changes; disclose included base content when first tracking an
existing untracked file modified by this task.

## Pull Requests

- Keep changes focused and minimal
- Include tests for new functionality
- Do not edit `CHANGELOG.md` — maintainers handle changelog entries
- Run the full check suite before submitting

## Questions

Open an issue or reach out to maintainers directly.
