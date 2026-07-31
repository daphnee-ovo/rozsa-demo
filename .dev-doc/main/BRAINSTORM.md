# Brainstorm Notes — Rózsa Dev-flow Integration

**Date**: 2026-07-31

## Background & Purpose

Rózsa should provide a lightweight, optional integration with dev-flow without
coupling the product UI to dev-flow's internal document format or exposing its
current dashboard HTTP schema throughout the codebase.

The integration should:

- replace supported successful `dow` Bash tool-call summaries with concise,
  structured Task/Issue results;
- show open Task/Issue counts and claimed work in the sidebar;
- open the dev-flow web dashboard from a button above Settings;
- provide a dedicated Dev-flow settings pane with CLI discovery and diagnostics;
- provide a reusable in-app notification system for integration failures;
- adapt to window size, sidebar width, and UI font-size changes.

## Key Decisions

| Decision Point | Choice | Rationale |
|----------------|--------|-----------|
| Integration transport | Use the existing dashboard `GET /api/data` and `GET /api/events` interfaces read-only | The current API already provides a complete snapshot and file-watcher-driven SSE updates |
| Mutation boundary | Do not call dashboard `done`, `close`, `reopen`, or `update` endpoints | Their current implementations directly modify `.dev-doc` and do not preserve all CLI semantics |
| Compatibility boundary | Hide all dashboard DTOs, routes, SSE details, and process handling behind an internal adapter | API changes should be localized instead of propagating into GUI code |
| Ownership scope | Key integration state and dashboard services by canonical project root, not session | Multiple sessions in one project share the same dev-flow state |
| Dashboard lifecycle | Keep services for visited projects, then reclaim inactive projects after 15 minutes or under memory pressure | Switching back to a project should normally be immediate without allowing unbounded growth |
| Memory budget | `max(physical_memory * 5%, 256 MiB)` across dev-flow caches and Rózsa-started dashboard processes | Provides a minimum useful budget while scaling with the machine |
| CLI readiness | Detect and validate `dow` before enabling integration behavior | Dev-flow features must not appear functional when the CLI is unavailable |
| Default enablement | When a compatible CLI is detected, integration and sidebar display default to enabled | A project initialized later with `dow init` should connect automatically |
| Sidebar content | Show open counts directly, with no separate “Dev-flow” heading; list claimed work that fits and end with `more N` | Keeps the sidebar compact and product-focused |
| Notifications | General app-wide notification center; errors toast for 6 seconds, then remain in an unresolved-error entry | Important failures remain discoverable without persistent large banners |
| Tool-call recognition | Recognize successful supported standalone commands and pipelines whose final stage is a supported `dow ... create` | JSON input may come from `echo`, a file reader, `jq`, or another producer |
| Detail UI | Read-only responsive detail overlay in the main content area | Sidebar WebView width cannot reliably contain dashboard-style detail cards |
| Real CLI tests | Permit real `dow` only inside `tmp/test_env/<test-name>-<unique-id>/` | Exercises the real contract without modifying the development project's `.dev-doc` |

## Design Approach

### Architecture

The integration belongs behind an app-level project service. GUI code consumes
Rózsa-owned domain snapshots and never consumes raw dashboard JSON.

```text
Sidebar / Settings / Detail Overlay / Tool Results
                         |
                  GUI IPC snapshots
                         |
               DevFlowIntegration facade
                         |
       +-----------------+------------------+
       |                 |                  |
CLI discovery   Project service registry   Command result enricher
                         |
                 DevFlowProjectService
                         |
             +-----------+------------+
             |                        |
     private HTTP/SSE client    dashboard child process
             |                        |
       /api/data + /api/events   validated dow executable
```

Three type layers remain separate:

1. private dashboard API DTOs that match the current dev-flow response;
2. stable Rózsa domain types such as `DevFlowSnapshot` and `DevFlowWorkItem`;
3. minimal serializable GUI snapshots shaped for presentation.

### Components

#### CLI discovery

Discovery checks candidates in this order:

1. a user-configured absolute executable path;
2. `dow` available in the current process `PATH`;
3. Homebrew locations, including `/opt/homebrew/bin/dow`,
   `/usr/local/bin/dow`, and the result of a discovered `brew --prefix`;
4. `$CARGO_HOME/bin/dow`, or the user's `.cargo/bin/dow` when
   `CARGO_HOME` is unset;
5. the global npm prefix's `bin/dow` and supported common npm global bin
   locations.

Candidates are canonicalized, deduplicated, checked for executability, and
validated with a short-timeout `dow --version`. The expected CLI version output
is `dow <semver>`. `dow version` is not used because it reports the current
project version.

A custom path never silently falls back to auto-discovery if it becomes invalid.
The user must explicitly select automatic discovery again.

Availability is explicit:

```text
CliMissing
CliIncompatible
ProjectNotInitialized
Starting
Ready
Disconnected
Dormant
```

#### Project service registry

`DevFlowProjectRegistry` is keyed by canonical project root. Sessions with the
same root share:

- one validated `dow` path;
- one Rózsa-started dashboard child;
- one dashboard URL and SSE connection;
- one latest snapshot;
- one unresolved-error set;
- one project activity record.

Switching sessions changes the active project subscription. It does not destroy
the previous project's service.

The registry records precise runtime session stop times because the existing
session `modified` value is not an exact stop time. A project is protected from
reclamation while it is displayed or any related session is running, waiting
for permission, or waiting for a user answer.

An undisplayed project with no active session becomes reclaimable 15 minutes
after the latest related session stop time. Reclamation closes Rózsa's SSE and
releases the service while retaining a compact stale snapshot. If the dashboard
process remains alive after Rózsa disconnects, the registry treats it as having
another browser client and avoids immediately killing the user's open page.
Returning to the project probes the old URL before starting a replacement.

Under the soft memory budget, eligible projects are reclaimed least-recently
used first. The current project and projects with active sessions are never
terminated merely to satisfy the soft budget. Closing the integration master
switch or exiting Rózsa terminates services started by Rózsa; external services
are never terminated.

#### Read-only dashboard client

The current read interfaces are:

```text
GET /api/data
GET /api/events
```

`/api/data` returns project status, all Task and Issue records, and the current
Brainstorm/PRD/SPEC documents. `/api/events` is SSE; an `update` event carries a
complete project snapshot after the dashboard's debounced `.dev-doc` watcher
observes a change.

The client performs an initial snapshot request, then subscribes to SSE. Raw API
status values are mapped into internal enums. Active claims currently appear as
Task/Issue status `in_progress`; the current API does not expose claim TTL,
agent identity, or an independent claim collection.

Required-field absence is an incompatibility error. Unknown fields and missing
document content that is not needed for the sidebar are tolerated. No mutation
route is represented in the public integration facade.

#### Sidebar and detail overlay

For a ready project, the status area displays:

```text
3 Tasks · 1 Issue
● T001  Implement integration
● I003  Fix dashboard startup
more 2
```

Task count includes only `pending` and `in_progress`. Issue count includes only
`open` and `in_progress`. Claimed work is a subset of those counts.

The number of claimed rows is calculated from actual available height and font
metrics using resize observation. Sessions remain usable and the project block
has a maximum share of sidebar height. Titles truncate at narrow widths and
expose their full text through an accessible tooltip.

Clicking the count opens all open work. Clicking a claimed row opens that item.
Clicking `more N` opens all claimed work. The read-only overlay shows ID, title,
priority/severity, complexity/type, refs, files, and done criteria or issue
description. It is anchored near the sidebar divider on wide windows and becomes
a main-content sheet on narrow windows. It provides no Task/Issue mutation
controls.

When a disconnected project has a last good snapshot, the UI retains it with a
visible stale marker. An uninitialized project renders no count row. A ready
project with no open items renders `0 Tasks · 0 Issues`.

#### Dashboard button

A Dashboard action appears immediately above Settings.

- Ready projects open their existing dashboard URL in the system browser.
- Starting projects show transient progress and open when ready.
- Uninitialized projects, missing/incompatible CLI states, and a disabled master
  switch disable the action with an explanatory tooltip or Settings route.
- Repeated clicks reuse the same project service.
- Successful opening is silent; startup or browser-open failures create errors.

#### Settings

The Dev-flow tab remains visible even when the CLI is missing. It shows:

- CLI detection, version, resolved executable, and rescan action;
- current project readiness, dashboard URL, last synchronization, and resource
  usage diagnostics;
- an integration master switch;
- a dependent “Show project status in sidebar” switch;
- automatic discovery or a custom absolute executable path with file selection;
- official Homebrew, npm, and Cargo installation commands when `dow` is absent.

The custom executable path is global-only. Integration and sidebar preferences
use the existing settings merge model. When a compatible CLI exists, both
switches default to enabled. An uninitialized project is watched lightly without
starting a dashboard. Creation of a valid `.dev-doc/STATUS.yaml`, including from
`dow init` executed during a conversation or in another terminal, starts the
project service automatically. Rózsa never runs `dow init` implicitly.

#### Structured `dow` results

Successful supported Bash calls replace their collapsed generic summary with:

```text
Created Task T001  Implement integration
Claimed Task T001  Implement integration
Completed Task T001  Implement integration
Created Issue I001  Dashboard startup failure
Claimed Issue I001  Dashboard startup failure
Closed Issue I001  Dashboard startup failure
```

The recognizer accepts:

- supported standalone `dow task/issue create`, `dow claim`,
  `dow task done`, and `dow issue close` commands;
- a single pipeline whose final command is exactly a supported
  `dow task create` or `dow issue create`;
- stdin redirection into those create commands.

The JSON producer before a create command is intentionally unrestricted. Shell
lists using `&&`, `||`, `;`, background execution, loops, and indirect script
execution remain generic Bash tool calls.

Recognition occurs only after exit code zero. Created IDs come from CLI output;
other IDs come from parsed arguments. Titles are resolved through the
project-scoped integration snapshot. Create triggers an immediate snapshot
refresh with a bounded wait for the dashboard watcher debounce. If details
cannot be confirmed, the UI shows only the confirmed action and ID with
`Details unavailable`.

Expanded results retain the original command, stdout, stderr, exit code, and
error information. The representation can be reconstructed from persisted Bash
tool calls/results when a session is reopened. It never reruns a command.

#### Notification center

The notification center is a generic main-WebView component outside the Main
and Settings scene roots. Toasts stack downward from the top right. Each has an
independent timer; removal causes lower notifications to animate upward.
Hovering a toast pauses only that toast.

Info and success notifications are emitted only when necessary. Routine
connection, synchronization, count changes, and dashboard readiness are silent.
Warnings disappear after six seconds. Errors display for six seconds, then
collapse into a top-right circled `!` entry whose count represents unresolved
errors, not unread notifications.

Errors use stable deduplication keys. Repeated retries update one error rather
than increasing the count. Recovery removes the error automatically. Closing an
error toast hides the toast early but does not mark the condition resolved.
Hover or keyboard focus opens the unresolved list; pointer movement into the
list keeps it open. Clicking pins it, and Escape closes it.

### Data Flow

#### Startup and late project initialization

1. Resolve the active session's canonical project root.
2. Load settings and discover/validate `dow`.
3. If integration is enabled but `.dev-doc/STATUS.yaml` is absent, enter
   `ProjectNotInitialized` and watch for initialization without starting a child.
4. When project initialization appears, allocate or reuse a project service.
5. Start `dow dashboard --no-open` in the project root, obtain its URL, request
   the initial snapshot, and establish SSE.
6. Map the snapshot into Rózsa domain state and emit a sidebar snapshot.

#### Project/session switching

1. Map the selected session to a canonical project root.
2. Reuse that project's existing service or stale cache if present.
3. Subscribe the sidebar/settings/detail consumers to that project.
4. Leave other project services running until lifecycle reclamation.

#### Snapshot update

1. Dashboard file watcher emits an SSE `update` carrying a complete API snapshot.
2. The private client validates and maps it atomically.
3. The registry replaces the last good project snapshot.
4. GUI events update counts, claimed rows, details, and stale state.

#### Structured command completion

1. Persist and execute the normal Bash tool call.
2. On completion, parse the supported command shape and confirmed IDs.
3. Resolve the project registry entry and request a refresh when required.
4. Enrich the stored/rendered result with an internal structured presentation.
5. Keep the raw Bash result available in the expanded body.

### Error Handling

- CLI-not-found, incompatible CLI, uninitialized project, dashboard startup,
  malformed snapshot, SSE disconnect, and intentional shutdown are distinct
  states.
- Startup validates prerequisites before allocating a project service.
- SSE reconnect uses bounded exponential backoff and preserves the last good
  snapshot as stale.
- Missing required API fields fail visibly rather than producing empty counts.
- Child exit, cancellation, settings changes, and app shutdown are idempotent
  cleanup paths.
- Intentional reclamation and user-requested shutdown do not create errors.
- Errors are registered with stable unresolved-error IDs and clear automatically
  only when their underlying condition recovers.

### Verification Strategy

Unit and integration coverage should include:

- CLI candidate ordering, custom-path behavior, timeouts, and semver parsing;
- API DTO mapping, required/unknown fields, open-count filtering, and claims;
- canonical project keys and multi-session project sharing;
- session stop-time tracking, 15-minute reclamation, LRU ordering, and the
  `max(physical_memory * 5%, 256 MiB)` budget;
- dashboard startup, first snapshot, SSE reconnect, stale cache, and cleanup;
- sidebar row fitting, `more N`, detail overlay breakpoints, settings dependency
  states, and Dashboard action behavior;
- supported/unsupported Bash grammar, multi-ID operations, failed commands, and
  session reload reconstruction;
- independent notification timing, reflow, deduplication, hover/focus behavior,
  unresolved counts, and recovery.

Deterministic tests use fake process and HTTP/SSE adapters. Real `dow` integration
tests are also permitted, but every invocation must use an isolated directory:

```text
tmp/test_env/<test-name>-<unique-id>/
```

Each real test owns its cwd, git repository if needed, `.dev-doc`, port, config
environment, child processes, and cleanup. It must never point a real `dow`
command or dashboard process at the development project's `.dev-doc`. Tests
must avoid process-global cwd mutation so parallel tests cannot cross projects.

Manual macOS validation covers window/sidebar resizing, font-size changes,
Main/Settings switching, hover/focus behavior, dashboard browser opening, and
notification stacking.

## Constraints & Boundaries

- The integration reads dev-flow state only through the dashboard HTTP/SSE API.
- The feature does not invoke dashboard mutation endpoints.
- It does not read or parse `.dev-doc` Task/Issue files directly.
- It does not install dev-flow or execute `dow init`.
- It does not deeply embed the dashboard web application.
- It does not add Task/Issue editing actions to the detail overlay.
- It does not claim that current unversioned dev-flow API paths are stable;
  compatibility changes remain isolated in the private adapter.
- It does not hide raw Bash commands or failures behind structured summaries.
- It does not terminate dashboard services that Rózsa did not start.
- Existing GUI visual language, accessibility rules, and two-WebView scene
  architecture remain authoritative.
- Relevant Rust structural changes require generated FrameworkTree updates.
- Documentation work includes a new `docs/gui/DEV_FLOW_INTEGRATION.md` plus
  synchronized architecture, UI guidelines, terminology, frontend terminology,
  Related Docs/backlinks, and prototype updates.

## Next Steps

Proceed to `/prd` to formalize feature requirements and acceptance criteria for
the integration, notification center, settings, and responsive GUI. Then enter
`/spec` for concrete Rust module boundaries, IPC types, child-process ownership,
shell parsing, HTTP/SSE implementation, test fixtures, and file-by-file scope.
