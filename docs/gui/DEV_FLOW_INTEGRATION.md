# Dev Flow GUI integration

Rózsa treats Dev Flow as an optional, project-scoped integration. The GUI only
enables it after finding a compatible `dow` executable through the shared
discovery layer; Homebrew, npm, Cargo and `PATH` installations are searched by
default, while Settings may hold one explicit absolute path.

Discovery validates candidates with bounded `dow --version` execution. An
explicit invalid custom path is reported as an error and never silently falls
back to another installation. A project becomes available only after its
current revision has a readable `STATUS.yaml` and `dow status` succeeds; a
later `dow init` is detected without restarting Rózsa.

## Settings pane

The Dev Flow pane follows the same flat groups, typography, hairlines, switches,
inputs and buttons as the other Settings panes. It contains:

- the detected version beneath the title and a short integration description;
- missing-CLI installation guidance plus `Check again`;
- an `Overview` with Dashboard Availability, Dashboard address, current-project
  Memory Use, and one Path row with `Choose…` and optional `Use automatic`;
- `Enable Dev Flow`, `Show Dev Flow status in sidebar`, and `Show Dashboard
  button` switches.

The master switch always remains operable. The two dependent switches are
disabled when the master switch is off or the CLI is unavailable. Setting
mutations are serialized in the backend and carry a monotonically increasing
frontend intent revision so a late response cannot restore stale controls.
Every successful mutation publishes a new sidebar snapshot immediately.

Automatic installation/setup and Dev Flow-owned system-prompt injection are
not implemented. Their cross-project contracts are tracked in
[`docs/TODO.md`](../TODO.md).

## Sidebar behavior

The Status section has no separate Dev Flow heading. When enabled and ready it
shows open, non-closed work counts as `N Tasks · N Issues`, followed by as many
claimed rows as fit without shrinking Sessions below its minimum height. Hidden
rows collapse into `more N`.

Hover opens the read-only detail surface after a short delay and closes it when
the pointer leaves; click pins the same surface. The Dashboard action is placed
directly above Settings and is absent, rather than merely disabled, when its
preference or the master integration is off.

## Ownership and failure behavior

Dashboard services belong to a project identity, not to a session. Sessions for
the same canonical project root and revision share the service and snapshot.
Named branches, unborn branches, detached commits and non-Git projects remain
distinct identities. Project switches retain old services for later reuse,
subject to 15-minute inactive-session reclamation and the soft
`max(system memory × 5%, 256 MiB)` budget. Active/current services and possible
external dashboard clients are protected. Memory Use is the current service
child RSS plus its project-local registry cache estimate.

The GUI talks to Dev Flow only through the internal adapter in `rozsa-app` and
the typed `DevFlowRuntime` facade in `rozsa-gui`; frontend code does not depend
on Dev Flow transport payloads. Transport/API changes must be absorbed by that
adapter and its contract tests before changing GUI snapshots.

The adapter uses only the loopback REST v1 read routes: `GET /api/v1/status`,
`GET /api/v1/tasks`, and `GET /api/v1/issues`. `GET /api/v1/events` supplies SSE
invalidation signals; each update causes the adapter to re-fetch and atomically
publish a fully validated combined snapshot. Rózsa does not expose or invoke
the dashboard's mutation or document routes. A real-`dow` test validates
discovery at `GET /api/v1` (without a trailing slash) and these read contracts
inside an isolated `tmp/test_env/` project.

Errors use the shared notification center. They appear for six seconds, then
remain reachable through the unresolved-error indicator until explicitly
resolved by recovery. Routine ready/success states stay silent.

## Verification and diagnostics

Real-`dow` contract tests create a unique Git project beneath
`tmp/test_env/<test-name>-<unique-id>/`, give every command an explicit temporary
working directory, reserve a port in 9800–9900, reap every owned dashboard PID,
and verify that the development project's `.dev-doc` fingerprint is unchanged.
On macOS, tests that depend on FSEvents or watcher-driven SSE must run with host
permissions: a sandbox can allow REST/socket traffic while suppressing file
events and producing a false watcher timeout.

When status is unavailable, diagnose in this order: resolved `dow` path and
version, current project/revision identity, readable initialization marker,
`dow status`, dashboard process/loopback URL, initial REST resources, then SSE
updates. API or transport changes stop at the app-layer adapter; frontend code
must not parse raw Dev Flow payloads or inspect `.dev-doc` task/issue files.

## Related documents

- [GUI architecture](./ARCHITECTURE.md)
- [GUI runtime terminology](./TERMINOLOGY.md)
- [Frontend terminology](./FRONTEND_TERMINOLOGY.md)
- [UI usage guidelines](./UI_USAGE_GUIDELINES.md)
- [Deferred product work](../TODO.md)
