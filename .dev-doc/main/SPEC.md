# SPEC: Read-only Dev-flow Integration

## Goal

Add a project-scoped, read-only dev-flow integration to Rózsa. It must discover
and validate `dow`, manage reusable dashboard services, adapt the versioned
dashboard REST/SSE API behind an internal boundary, present open and claimed
work in the GUI, specialize supported successful `dow` Bash results, and add a
reusable notification center without coupling GUI code to dev-flow API details.

## Scope

### In

- Type-safe Dev-flow settings, CLI discovery, project/revision identity, dashboard
  process ownership, HTTP snapshot reads, SSE updates, lifecycle reclamation,
  and resource accounting.
- Open Task/Issue counts, claimed work rows, responsive read-only detail UI, and
  a Dashboard action in the sidebar.
- A dedicated Dev-flow settings pane with one resolved CLI Path control, a flat
  project Overview, installation guidance, and dependent enablement.
- Structured presentation for supported successful create, claim, task-done,
  and issue-close Bash calls while preserving raw execution details.
- A general app notification center with independent timers and unresolved-error
  aggregation.
- Focused deterministic tests, isolated real-`dow` contract tests, GUI
  validation, and synchronized GUI documentation/prototype updates.

### Out

- Calling dashboard mutation routes or adding Task/Issue edit actions.
- Parsing Task/Issue state directly from `.dev-doc`.
- Installing dev-flow, running `dow init`, or embedding the dashboard deeply.
- Supporting arbitrary compound shell scripts as structured `dow` results.
- Changing dev-flow itself or claiming compatibility with future API versions
  outside the private adapter.

## Requirements Trace

| Requirement | Source | Design coverage |
|-------------|--------|-----------------|
| R-001 Read-only API integration | Brainstorm: transport and mutation boundary | `dev_flow::dashboard` exposes only snapshot/SSE reads |
| R-002 API changes remain localized | User decision and Brainstorm architecture | Private DTO → domain → GUI snapshot layers |
| R-003 Project-scoped sharing | User correction | Registry groups services by canonical project root and branch |
| R-004 Sidebar open counts and claimed work | User-defined layout | `DevFlowSidebarSnapshot` and responsive row fitting |
| R-005 Dashboard action | Initial request | Reused Rózsa-owned service and system-browser opening |
| R-006 Settings and CLI discovery | Initial request and follow-up | Typed settings plus Homebrew/npm/Cargo discovery |
| R-007 Late `dow init` detection | User follow-up | Branch-aware initialization marker plus validated `dow status`, periodic probe, and immediate Bash-triggered rescan |
| R-008 Specialized `dow` results | Initial request and pipeline follow-up | Conservative recognizer plus project snapshot enrichment |
| R-009 Notification behavior | User-defined behavior | Generic notification center and unresolved-error registry |
| R-010 Bounded service growth | User-defined lifecycle | 15-minute idle sweep and memory soft budget |
| R-011 Isolated real CLI tests | User constraint | `tmp/test_env` contract-test harness |

## Implementation Contract

This section is normative for every implementation Task. It exists so a
cold-start implementer can make the same decisions without access to the design
conversation.

### Required reading and decision order

For each Task, read this contract, the Task's referenced design section, and
its referenced acceptance criteria before editing. The Task's file scope and
dependencies are mandatory. A narrower Task must not absorb later ownership
because an adjacent implementation looks convenient. When the Task and SPEC
cannot both be satisfied, stop and update the artifacts through dev-flow; do
not guess.

### Cross-task invariants

1. **Read-only boundary**: dashboard integration invokes only loopback GET
   `/api/v1/status`, `/api/v1/tasks`, `/api/v1/issues`, and `/api/v1/events`.
   API discovery at GET `/api/v1` is contract-test-only. No public generic
   request method, POST/PATCH/PUT/DELETE route, or Task/Issue mutation operation
   is allowed. Direct `.dev-doc` access is limited to locating and opening the
   selected `STATUS.yaml` marker; all Task/Issue data comes from the private
   dashboard adapter.
2. **Compatibility boundary**: dashboard paths, DTOs, raw status strings, SSE
   framing, and child-process arguments remain private to app-layer adapters.
   Registry, GUI, session persistence, and frontend code consume Rózsa-owned
   typed domain models only.
3. **Identity boundary**: the service key is canonical root plus full
   `DevFlowRevisionKey`. `NamedBranch`, `UnbornBranch`, `DetachedCommit`, and
   `NonGit` are never collapsed. Detached is an explicit unsupported
   availability state; NonGit with more than one readable marker is explicit
   ambiguity.
4. **Readiness boundary**: readable marker → successful two-second JSON
   `dow status` → service startup → first valid snapshot → Ready. A failure at
   any earlier stage starts no dashboard and cannot be represented as Ready.
5. **Ownership boundary**: services and snapshots belong to a project key, not
   a session. Session selection changes association/subscription. It must not
   destroy another revision's service or enrich records across project keys.
6. **Lifecycle boundary**: only Rózsa-owned children may be stopped by disable,
   replacement, reclamation, or shutdown. `PossibleExternalClient` is protected.
   The memory threshold is soft and never authorizes killing active/displayed
   or possibly external work.
7. **Presentation boundary**: open counts exclude closed/done work; claimed is
   the API `InProgress` subset. Routine Ready/sync/open success is silent.
   Structured Bash cards require confirmed success and preserve the full raw
   Bash result when expanded.
8. **Persistence boundary**: restored presentation uses persisted
   execution-time project/revision identity and never executes commands. A
   current snapshot may add a title only on exact project-key equality.
9. **Test isolation boundary**: deterministic tests inject runners, clocks,
   factories, transports, and memory readers. A real `dow` test must use a
   unique `tmp/test_env/<test-name>-<unique-id>/` root, explicit per-command cwd,
   owned port/PID cleanup, and before/after proof that the development
   `.dev-doc` was untouched.
10. **Structure boundary**: preserve crate dependency direction, reuse existing
    public components, keep settings global, and run `make-tree --write` for
    every supported Rust file whose symbol structure changes.

### Task ownership matrix

| Task | Required output | Explicit non-goals |
|------|-----------------|--------------------|
| T003 | Resolver, marker/status probe, shared registry service handle, whole-worktree reassociation | Activity timestamps, reclamation, GUI/session event wiring |
| T004 | Activity state, sweep/LRU/memory accounting, protected child handling and stale reuse | Frontend behavior and settings IPC |
| T005 | Generic notification events/store/timers/accessibility | Dev-flow runtime orchestration and project errors |
| T006 | Integration facade, settings/diagnostics commands, discovery/restart/disable and activity/rescan wiring | Sidebar/detail and structured Bash card UI |
| T007 | Responsive sidebar/detail snapshots and constrained loopback Dashboard opening | Shell parsing, persistence, mutation controls |
| T008 | Side-effect-free shell recognizer and typed presentation model | Command execution, persistence, GUI rendering |
| T009 | Backward-compatible typed metadata and no-execution reconstruction | Visual rendering and cross-project title lookup |
| T010 | Structured collapsed card and evidence-preserving expansion | Recognition grammar and storage schema |
| T011 | Real supported-`dow` contract proof in isolated temporary projects | Feature implementation and skipped-as-passed tests |
| T012 | Final user/developer docs, backlinks, prototype and scoped checks | Full TEST phase, release, or undocumented product changes |

The dependency graph, rather than numeric order alone, determines scheduling.
T013 is a documentation gate before T003 and the independent notification Task
T005; all other unfinished work depends on those paths transitively.

## Design

### 1. Module and dependency boundaries

Add an app-layer module because process, network, settings, and project state are
product-runtime concerns rather than GUI rendering concerns:

```text
crates/rozsa-app/src/dev_flow/
├── mod.rs          facade, public domain types, errors
├── discovery.rs    executable candidates and `dow --version`
├── dashboard.rs    child process, bounded HTTP JSON, SSE decoder
├── registry.rs     project/revision services, activity, retries, reclamation
└── command.rs      conservative Bash recognition and presentation model
```

`rozsa-gui` owns only Tauri wiring, session-to-project activity signals, GUI
snapshots, browser opening, and frontend components. `rozsa-core` and
`rozsa-model` remain unchanged unless the existing session-log owner must gain
the backward-compatible typed metadata variant required by
`DevFlowPresentationRecord`; that schema change must not add dev-flow runtime
dependencies to a lower crate.

Use the existing workspace `reqwest` dependency in `rozsa-app`. Add `sysinfo` as
a reviewed workspace dependency for physical-memory and child-RSS accounting.
Decode the narrow SSE protocol internally over `reqwest::Response::chunk()` to
avoid another event-source dependency. Add the official Tauri opener plugin to
`rozsa-gui` for system-browser opening and grant only its URL-opening capability.

All subprocesses use `tokio::process::Command`/`std::process::Command` argument
arrays; no shell is used for discovery or dashboard startup. HTTP clients reject
redirects and accept only loopback URLs created by the process manager.

### 2. Domain model and compatibility adapter

Private dashboard DTOs mirror only fields Rózsa consumes. Unknown JSON fields
are ignored. The required compatibility surface is:

```rust
struct DashboardSnapshotDto {
    status: DashboardStatusDto,
    tasks: Vec<DashboardTaskDto>,
    issues: Vec<DashboardIssueDto>,
}
```

`tasks`, `issues`, and each item's `id`, `title`, and `status` are required.
Priority/severity, complexity/type, refs, files, done criteria, description, and
project status details are optional so a missing decorative field does not
disable counts. Missing required fields, invalid IDs, or unknown required status
values produce `DevFlowError::IncompatibleApi`.

The adapter maps DTOs into stable Rózsa-owned types:

```rust
struct DevFlowSnapshot {
    revision: u64,
    project: DevFlowProjectStatus,
    tasks: Vec<DevFlowTask>,
    issues: Vec<DevFlowIssue>,
    received_at: SystemTime,
    stale: bool,
}

enum DevFlowTaskStatus { Pending, InProgress, Done }
enum DevFlowIssueStatus { Open, InProgress, Closed }
```

Every REST resource response and each SSE event is capped at 16 MiB. Rózsa does
not request `/api/v1/docs`, so document bodies never cross the adapter boundary.
The status, task, and issue DTOs are combined and validated completely before
atomically replacing the last good snapshot.

The private client has no generic public request method and exposes only:

```rust
async fn fetch_snapshot(&self) -> Result<DevFlowSnapshot, DevFlowError>;
async fn subscribe(&self) -> Result<DevFlowEventStream, DevFlowError>;
```

No dashboard mutation method or path is represented or invoked.

### 3. CLI discovery and settings

Add typed settings with serde defaults:

```rust
struct DevFlowSettings {
    enabled: bool,                 // default true
    show_sidebar_status: bool,     // default true
    show_dashboard_button: bool,   // default true
    executable_path: Option<PathBuf>, // None = automatic
}
```

All four settings are global application settings. This matches the master
switch's process-wide ownership semantics and prevents one project-scoped value
from ambiguously enabling or stopping services shared by several sessions.
`SettingsManager` gains field-preserving typed Dev-flow update methods instead
of routing these values through the stringly typed generic GUI setting command.

Automatic discovery checks, in order:

1. current process `PATH`;
2. `/opt/homebrew/bin/dow`, `/usr/local/bin/dow`, and the prefix returned by a
   discovered standard Homebrew executable;
3. `$CARGO_HOME/bin/dow`, or the user's `.cargo/bin/dow`;
4. `<npm prefix -g>/bin/dow` and supported platform npm global-bin locations.

A configured custom absolute path is the sole candidate until the user selects
Auto again. Existing candidates are canonicalized and deduplicated. Discovery
helper commands and `dow --version` run under a two-second timeout. Validation
requires exit zero and `dow <semver>`. The validated absolute path is cached and
used directly for dashboard startup.

Setting `enabled=false` cancels connection work, resolves integration-owned
errors, terminates and reaps every Rózsa-owned dashboard child, and removes
Dev-flow controls from project views while retaining no live snapshot as
authoritative. Re-enabling rediscovers the CLI and starts services only for
currently relevant initialized projects.

Changing the executable selection or pressing Rescan validates the replacement
before adopting it. A valid changed executable restarts currently relevant
Rózsa-owned services through the new absolute path and marks old snapshots stale
until new initial snapshots arrive. An invalid custom path is shown as an
actionable CLI error and stops owned services; it never falls back to automatic
discovery. Selecting Auto restores automatic discovery.

The settings pane uses dedicated typed commands:

```text
get_dev_flow_settings
set_dev_flow_enabled
set_dev_flow_sidebar_status
set_dev_flow_dashboard_button
set_dev_flow_executable_path
rescan_dev_flow
```

All Dev-flow setting mutations are serialized by the backend and return the
authoritative post-mutation snapshot. The frontend coalesces rapid interactions
and ignores stale completions, so an earlier request cannot overwrite a newer
master-switch intent. Master/sidebar/dashboard visibility mutations publish a
fresh sidebar snapshot immediately.

The pane remains visible when `dow` is missing and shows the official Homebrew,
npm, and Cargo commands without executing them. Automatic installation/setup
and Dev-flow-specific system-prompt injection are deferred in `docs/TODO.md`
because both require a coordinated Rózsa/dev-flow contract. The master switch
must not claim to control either feature until that contract is implemented.

The pane uses the same title, typography, spacing, hairline separation, toggle,
input, and button components as the other Settings tabs. It has no gray
diagnostic-card background. The header combines `Dev Flow` with the detected
version and a short description. A flat `Overview` group contains Dashboard
Availability, Dashboard address, current-project Memory Use, and one Path row.
The Path row shows the resolved executable and a native `Choose…` action; a
custom selection additionally exposes a quiet `Use automatic path` action.
There is no second Executable/Auto/Custom block and no duplicated CLI summary.

The `Settings` group contains exactly the master switch, `Show Dev Flow status
in sidebar`, and `Show Dashboard button`. Behavior switches are dependent on
the master switch and compatible CLI, while Path recovery and missing-CLI
guidance remain usable. When `dow` is missing, the header recommends installation
and exposes `Check again`; it does not expose a nonfunctional install button.

### 4. Project identity and service registry

Resolve each session's current cwd from `AgentSession::current_cwd()` when
active, or `SessionMeta.cwd` when inactive. Resolve a canonical Git root with
`git -C <cwd> rev-parse --show-toplevel`; fall back to the canonical cwd for a
non-Git project.

Dev-flow's current doc-root selection is branch-aware. Model revision states
explicitly instead of collapsing detached or unborn repositories into `None`:

```rust
struct DevFlowProjectKey {
    root: PathBuf,
    revision: DevFlowRevisionKey,
}

enum DevFlowRevisionKey {
    NamedBranch(String),
    UnbornBranch(String),
    DetachedCommit(String),
    NonGit,
}
```

Resolve named/unborn branches with `git symbolic-ref`; resolve detached identity
with the full commit OID. The top-level registry groups entries by canonical
root, while each supported revision has its own service/snapshot. Sessions in
the same worktree and revision share one service. A branch change selects or
creates another service without destroying the previous one. Successful Bash
completion, session switching, and the two-second selected-project probe
re-evaluate the identity for every session associated with that worktree, so a
branch-changing command cannot leave sibling sessions attached to the old
snapshot.

For an enabled project with a compatible CLI:

- for a named or unborn branch, require a readable
  `<root>/.dev-doc/<branch>/STATUS.yaml`;
- for a non-Git root, require exactly one readable
  `<root>/.dev-doc/*/STATUS.yaml`; zero means `ProjectNotInitialized` and more
  than one is an explicit ambiguous-project error;
- after the marker check, run read-only `dow status` in the project root under a
  two-second timeout and require exit zero plus a valid JSON response before
  starting the dashboard;
- probe readiness every two seconds while the project is relevant and rescan
  immediately after any successful Bash completion, including a late
  `dow init`;
- treat the project as Ready only after a valid first dashboard snapshot.

The current `dow dashboard` cannot be directed to a detached commit's matching
doc root. Detached HEAD therefore has a distinct, explicit
`UnsupportedRevision` availability state and starts no dashboard rather than
risk showing another branch. This boundary can be removed when dev-flow exposes
explicit revision selection.

Rózsa reads only the initialization marker; it does not parse Task/Issue state
from `.dev-doc`. Because marker validation happens before dashboard startup,
Rózsa does not invoke `dow dashboard` merely to create a missing branch
directory.

### 5. Dashboard process and SSE lifecycle

Start the validated executable as:

```text
dow dashboard --port <candidate> --no-open
```

Use an explicit loopback port from 9800–9900 so the URL is known without parsing
human stderr. Probe candidates and retry on bind/start races. Startup succeeds
only when GET `/api/v1/status`, `/api/v1/tasks`, and `/api/v1/issues` combine
into a valid snapshot within five seconds. Capture bounded stderr for
diagnostics and always reap children.

Loopback connects have a one-second deadline. The initial snapshot, refresh
requests, and SSE response headers each have a five-second overall deadline.
Once subscribed, no bytes, comment keep-alive, or valid update for 45 seconds is
treated as a stalled connection. Disable, project reclamation, executable
change, and application shutdown cancel all pending requests and retries.

Connect GET `/api/v1/events` immediately after the first snapshot. The decoder
supports comments, CRLF/LF, multiline `data`, and blank-line event termination.
Only `event: update` is mapped. Its `{"resource":"..."}` data is an
invalidation signal; the adapter re-fetches the three read-only resources and
publishes only the fully validated combined snapshot. Keep-alives do not
increment revision.

On unexpected disconnect:

- retain the last snapshot with `stale=true`;
- retry after 1, 2, 4, 8, 16, then at most 30 seconds while enabled;
- register one deduplicated unresolved error after the third consecutive
  failed retry or seven elapsed seconds from disconnect, whichever comes first;
- clear it and reset backoff on recovery.

Closing the master switch or exiting Rózsa terminates and reaps every child
started by Rózsa. Processes not started by Rózsa are never terminated.

Dashboard startup, snapshot/SSE connection, and browser opening use stable
per-project notification IDs:
`dev-flow.dashboard-start:<project-hash>`,
`dev-flow.connection:<project-hash>`, and
`dev-flow.dashboard-open:<project-hash>`, where the hash covers canonical root
and full revision identity. CLI discovery uses the global `dev-flow.cli` ID.
The matching successful recovery resolves each condition. Intentional
disable/reclamation resolves the affected integration-owned conditions without
emitting a new notification.

### 6. Activity, reclamation, and memory

The GUI reports session start/stop/project-change activity to the registry.
`finish_interaction`, abort/failure completion, permission/user-question
resolution, session close, and session switch must leave the registry with an
accurate active/inactive state. Waiting permission or user input counts as
active. Store exact runtime `last_stop_at`; do not substitute session
`modified`.

Sweep once per minute. A revision service is time-reclaimable when:

- it is not the currently displayed project/revision;
- no associated session is active;
- the newest associated stop time is at least 15 minutes old.

The soft budget is:

```text
max(total physical memory × 5%, 256 MiB)
```

Count Rózsa-started dashboard RSS plus serialized retained-snapshot size and a
documented fixed registry overhead. When over budget, reclaim eligible services
in least-recently-used order, preferring those past 15 minutes. If necessary,
reclaim the oldest undisplayed inactive service before 15 minutes. Never reclaim
the displayed service or an active-session service merely for the soft budget.

Reclamation first closes Rózsa's SSE and waits up to the known dashboard
no-client shutdown window, capped at 35 seconds. If a supported tested `dow`
child exits, reap it. If it remains alive, classify it
`PossibleExternalClient`, infer that a browser dashboard may still be connected,
and never force-kill it for time or memory pressure. Retain only a compact stale
snapshot and URL, and recheck protected children once per minute. Revisit probes
the old URL before starting a replacement. Intentional reclamation never creates
an error.

The memory threshold is a soft budget, not a hard cap: displayed/active services
and `PossibleExternalClient` children may keep usage above it. The registry
reports that protected usage in diagnostics and continues reclaiming other
eligible services. This explicitly favors not destroying a dashboard the user
may have open over strict budget enforcement.

### 7. GUI IPC and presentation

Extend `SidebarSnapshot` with an optional `DevFlowSidebarSnapshot` containing:

```text
project key/revision
open task count
open issue count
claimed item summaries
stale flag
availability
dashboard availability
```

Count Tasks in `Pending|InProgress` and Issues in `Open|InProgress`; claimed
items are the `InProgress` subset and are not double-counted. An initialized
Ready project with no open work displays `0 Tasks · 0 Issues`. An uninitialized
project displays no count row.

The sidebar uses `ResizeObserver` and measured row height. It reserves a usable
minimum session-list area, displays as many claimed rows as fit, and ends with
`more N` for hidden rows. The summary, claimed rows, and `more N` invoke a typed
detail request. The backend emits a main-WebView request containing project key,
snapshot revision, and target; the main view rejects a stale/mismatched request.

The read-only overlay shows the fields available in the domain snapshot. It is
an anchored panel beside the divider at wide sizes and a main-content sheet at
narrow sizes. It supports focus management, Escape, outside-click dismissal,
and keyboard navigation, and contains no mutation controls.

Add Dashboard immediately above Settings. `open_dev_flow_dashboard` ensures or
reuses the current project service, then uses the Tauri opener plugin. Missing
CLI, disabled integration, uninitialized project, startup, and failure states
have explicit disabled/loading/error UI. Successful opening is silent.

### 8. Structured Bash result presentation

The GUI frontend derives this presentation directly from the existing assistant
`ToolCall` and matching Bash `ToolResult` already present in the session
messages. This is a display transformation only: it must not execute commands,
add session metadata, or require a GUI snapshot field dedicated to presentation.

For a successful, non-truncated Bash result, the frontend may recognize the
Dev-flow resource action set: `task create`, `task update`, `task remove`,
`task done`, `task reopen`, `issue create`, `issue update`, `issue remove`,
`issue close`, `issue reopen`, `claim`, and `claim --revoke`. The collapsed
summary shows Created, Updated, Removed, Completed, Reopened, Claimed,
Released, or Closed with the entity, short ID, and optional title. Read-only
resource queries (`show`, `list`, and `schema`) and project/status/workflow
commands remain generic Bash tool calls; they must not be represented as
Task/Issue mutation cards. Failed, truncated, incomplete, or unsafe compound
commands also remain generic Bash tool calls.

Title enrichment uses one bounded, read-only detail GET per Task/Issue ID:
`GET /api/v1/tasks/:id` or `GET /api/v1/issues/:id`. It does not fetch the
collection endpoints. The GUI renders `Details unavailable` when the lookup
does not complete; removed resources are not fetched after deletion because
their detail endpoint may already return 404. This title enrichment is
separate from the Dev-flow status/sidebar/dashboard presentation and must not
introduce session record persistence or app-layer presentation state.

The recognizer accepts output-only flags such as `-H`/`--human`, known option
values such as `--timeout` and `--confirm`, input redirection, and a captured
`2>&1` redirect. A redirect such as `2&>1` and a semicolon compound command
such as `dow task update T001 2>&1; echo "==="` remain generic because the
single Bash result cannot prove the individual `dow` command succeeded.

Session activation and restoration recompute the card from persisted messages
without re-executing Bash. The Dev-flow status/sidebar/dashboard data flow is
independent of this Bash tool-call card.

### 9. Notification center

Replace the single appended chat notification behavior with a reusable
main-WebView notification layer outside Main/Settings roots. Keep compatibility
for existing string `notification` events by mapping them to nonpersistent info
toasts. Add a structured event:

```rust
enum AppNotificationEvent {
    Upsert {
        id: String,
        severity: NotificationSeverity,
        title: String,
        message: String,
        timeout_ms: u64,
    },
    Resolve { id: String },
}
```

Each toast owns its timer; hovering pauses only that timer. Toasts stack from the
safe top-right area and animate upward independently when an earlier toast
leaves. Info/success events are emitted only when user feedback is necessary.
Warnings disappear after six seconds. Errors show for six seconds, then remain
as unresolved entries behind a circled `!` with the unresolved count.

Stable IDs deduplicate repeated failures. Closing a toast does not resolve the
condition. Recovery emits `Resolve` and decrements the count. Hover/focus opens
the error list, pointer transition into the list keeps it open, click pins it,
and Escape closes it. The component is accessible without color and adapts to
font/window changes.

Integration failures use the per-project IDs defined in the dashboard lifecycle
section. Startup, connection, incompatible API, invalid CLI, and browser-open
failures are errors; successful recovery or the relevant intentional shutdown
resolves them. Routine readiness and successful dashboard opening remain silent.

### 10. Documentation and generated structure

Create `docs/gui/DEV_FLOW_INTEGRATION.md`. Synchronize
`docs/gui/ARCHITECTURE.md`, `UI_USAGE_GUIDELINES.md`, `TERMINOLOGY.md`,
`FRONTEND_TERMINOLOGY.md`, Related Docs/backlinks, and
`docs/gui/prototype/`. Update `AGENTS.md` only if implementation changes project
structure or workflow conventions. Run `make-tree --write` for every supported
Rust file whose symbol structure changes; never edit generated trees manually.

## Acceptance

- SPEC-AC-001: With no custom path, tests prove discovery order covers PATH,
  Homebrew, Cargo, and npm; each executable is validated by a two-second
  `dow --version` call, and an invalid custom path does not silently fall back.
- SPEC-AC-002: With a compatible CLI and integration enabled, a project lacking
  the current branch's readable `STATUS.yaml` or a successful two-second
  `dow status` starts no dashboard; completing `dow init` during the run causes
  readiness detection without restarting Rózsa. Detached and ambiguous non-Git
  roots fail explicitly rather than selecting an arbitrary branch.
- SPEC-AC-003: Sessions with the same canonical root and revision share one
  child and snapshot; named, unborn, detached, and non-Git identities remain
  distinct; a worktree branch change re-associates all its sessions; and
  switching projects or branches never shows another project's snapshot.
- SPEC-AC-004: Contract tests prove the integration client sends only loopback
  GET requests to `/api/v1/status`, `/api/v1/tasks`, `/api/v1/issues`, and
  `/api/v1/events`; real-`dow` coverage also validates GET `/api/v1` discovery.
  It exposes and invokes no dashboard mutation route.
- SPEC-AC-005: A valid combined initial snapshot and an SSE-triggered REST
  refresh atomically replace state; malformed/oversized data preserves the last
  good snapshot as stale. Tests
  enforce one-second connect, five-second request/header, 45-second SSE-stall,
  deterministic retry/error timing, cancellation, and one deduplicated
  incompatibility/connection error that resolves on recovery.
- SPEC-AC-006: Sidebar counts include only open Task/Issue statuses, claimed
  items appear once beneath the count, and available height/font changes yield
  the correct visible rows and `more N` without making Sessions unusable.
- SPEC-AC-007: Summary, claimed-row, and `more N` interactions open the correct
  read-only responsive detail UI with keyboard focus, Escape, and no mutation
  controls.
- SPEC-AC-008: Dashboard is immediately above Settings; one click starts or
  reuses one project service and opens its loopback URL, while unavailable states
  are explicit; startup/open failures create resolvable per-project errors; and
  successful opening emits no notification.
- SPEC-AC-009: Recognizer tests cover the supported Task/Issue resource actions,
  direct commands, file/stdin and arbitrary producer pipelines ending in create,
  multi-ID output, absolute `dow`, known option values, quoting,
  failure/truncation, unsafe redirects, and rejection of unsupported compound
  commands.
- SPEC-AC-010: Successful supported calls render the corresponding Created,
  Updated, Removed, Claimed, Released, Completed, Closed, or Reopened
  Task/Issue cards with short ID and title when available; reload reconstructs
  them from persisted messages without execution or cross-project enrichment,
  and expansion preserves raw Bash evidence.
- SPEC-AC-011: Dev-flow Settings matches the shared Settings visual language;
  its header shows version/description, its flat Overview shows availability,
  address, measured current-project memory and one editable Path row, and its
  Settings group contains master/sidebar/dashboard switches without duplicated
  executable controls or a gray diagnostic card. Missing-CLI guidance remains
  actionable without pretending automatic installation exists.
- SPEC-AC-012: Notification tests prove downward stacking, independent six-second
  timers, per-toast hover pause, upward reflow, deduplication, hover/focus error
  expansion, click pinning, and automatic unresolved-count reduction on Resolve.
- SPEC-AC-013: Lifecycle tests prove 15-minute idle reclamation, the
  `max(memory*5%, 256 MiB)` soft budget, LRU pressure ordering, active/current
  protection, tested no-client shutdown, `PossibleExternalClient` protection
  with observable temporary budget excess, stale-cache reuse, and idempotent
  child cleanup.
- SPEC-AC-014: Real-`dow` contract tests run only in unique
  `tmp/test_env/<test-name>-<unique-id>/` roots with per-command cwd and owned
  ports/processes; they never access or modify the development `.dev-doc`.
- SPEC-AC-015: Relevant focused Cargo/frontend tests, formatting, clippy, real
  isolated contract verification, responsive macOS GUI validation, generated
  FrameworkTrees, and synchronized GUI docs/prototype all pass before delivery.
- SPEC-AC-016: Rapid master-switch changes are serialized/latest-authoritative;
  dependent controls always recover after re-enable, setting failures remain
  visible, and master/sidebar/dashboard mutations immediately publish correct
  sidebar visibility without requiring a session event or restart.
- SPEC-AC-017: The master switch controls currently implemented integration
  behavior (service ownership, status rows, Dashboard action, notifications and
  tool presentation) without claiming prompt/setup behavior that remains in
  `docs/TODO.md`. Current-project memory comes from measured dashboard RSS plus
  owned snapshot/cache overhead and is formatted in MiB.

## Risks

- Dashboard API v1 may evolve within its version. Required-field validation,
  fixture and real-CLI contract tests, response caps, and the private adapter
  contain the blast radius.
- `dow dashboard` currently resolves branch-specific doc roots and may create a
  missing branch directory. Rózsa validates the exact branch marker before
  startup and refuses detached/ambiguous selection, containing this behavior
  until dev-flow exposes explicit revision selection.
- SSE/no-client shutdown behavior may change. Process lifecycle assumptions stay
  inside `dashboard.rs`; supported versions receive real contract coverage, and
  a surviving child is protected as a possible external-browser service.
- RSS collection adds a dependency and is approximate. Resource decisions use a
  soft budget and never kill current/active work solely for budget compliance.
- Conservative shell recognition may leave valid complex commands generic. This
  is preferred to false success presentation.
- The dashboard exposes document and mutation resources that Rózsa does not
  need. The private client has no generic public request surface, keeping those
  routes unreachable from GUI and registry code.

## Test Plan

- Pure app tests with fake executable runners, clock, memory reader, process
  launcher, HTTP transport, cancellation, session metadata, and SSE chunks.
- App integration tests against a loopback mock server covering startup, schema
  compatibility, deadlines, stalled streams, reconnect/error timing,
  branch/project isolation, and read-only requests.
- GUI integration/contract tests for IPC snapshots, settings, tool presentation,
  details, notification behavior, and responsive DOM.
- Required real CLI test using the discovered `dow`, a
  `tempfile::Builder::tempdir_in("tmp/test_env")` root, `Command::current_dir`,
  a unique Git repository and owned port. The harness owns/reaps every child and
  retains actionable diagnostics on failure.
- Manual macOS validation at minimum/normal/large window sizes and UI font sizes,
  including sidebar fit, scene switching, dashboard opening, notification
  stacking, and returning to a cached project.

## Self Check

- [x] Goal is clear
- [x] Scope and non-goals are explicit
- [x] Requirements trace to confirmed Brainstorm decisions
- [x] Module and dependency boundaries preserve crate direction
- [x] API mutation paths are excluded
- [x] Project/branch/session ownership is explicit
- [x] Failure, stale-data, resource, and cleanup paths are covered
- [x] Acceptance criteria are testable
- [x] Real `dow` tests are isolated from the development project
- [x] Matches quick mode without task decomposition
