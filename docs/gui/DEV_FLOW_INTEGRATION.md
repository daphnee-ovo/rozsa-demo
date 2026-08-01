# Dev Flow GUI integration

Rózsa treats Dev Flow as an optional, project-scoped integration. The GUI only
enables it after finding a compatible `dow` executable through the shared
discovery layer; Homebrew, npm, Cargo and `PATH` installations are searched by
default, while Settings may hold one explicit absolute path.

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
the same project share the service and snapshot. Project switches retain old
services for later reuse, subject to idle and memory reclamation. Memory Use is
the current service child RSS plus its project-local registry cache estimate.

The GUI talks to Dev Flow only through the internal adapter in `rozsa-app` and
the typed `DevFlowRuntime` facade in `rozsa-gui`; frontend code does not depend
on Dev Flow transport payloads. Transport/API changes must be absorbed by that
adapter and its contract tests before changing GUI snapshots.

Errors use the shared notification center. They appear for six seconds, then
remain reachable through the unresolved-error indicator until explicitly
resolved by recovery. Routine ready/success states stay silent.

## Related documents

- [GUI architecture](./ARCHITECTURE.md)
- [Frontend terminology](./FRONTEND_TERMINOLOGY.md)
- [UI usage guidelines](./UI_USAGE_GUIDELINES.md)
- [Deferred product work](../TODO.md)
