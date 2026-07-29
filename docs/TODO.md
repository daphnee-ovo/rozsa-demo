# TODO

## OAuth follow-up

- **Verify and replace OpenAI Codex `originator=pi` only after an authorization-flow regression test exists.** It is intentionally retained today in [`build_auth_url`](../crates/rozsa-model/src/oauth/openai_codex.rs) and [`test_build_auth_url`](../crates/rozsa-model/tests/oauth_openai_codex.rs). Before changing it, compare the current OpenAI Codex login implementation, establish an official replacement value or acceptance evidence, and add a test that exercises the resulting authorization URL. Do not change this query parameter on inference alone: it is sent only when a user begins a new OpenAI Codex OAuth login.

## Deferred product work

- **Model metadata reduction** — evaluate whether embedded model API and pricing metadata should be reduced or sourced differently, without weakening offline model selection.
- **Graph diff rendering** — decide whether graph views should render file diffs and define the interaction and performance contract first.
- **Codex OAuth compatibility review** — periodically compare the supported OpenAI Codex OAuth flow with the upstream Codex implementation and record any deliberate behavior changes.
- **Additional OAuth providers** — do not expose an OAuth provider until its login, credential refresh, and failure paths have focused integration coverage.

## Delayed items (2026-07-12)

- **GUI packaging and update configuration** (former TASK-T086) — defer cross-platform installers, update configuration, and signing endpoints until the release channel and platform prerequisites are decided.
- **Agent loop async hooks** (former ISSUE-I033) — defer the synchronous-to-async hook interface until compaction, context transforms, and steering-queue requirements are defined.
- **Package Manager** (former ISSUE-I057) — defer extension and skill installation management until the npm/git scope, package format, lock file, and offline behavior are defined.
- **Auto-approve small-model permission reviewer** (former TASK-T044) — `auto-approve` remains an explicitly unsupported mode and must not persist. A future implementation must preserve `deny > ask > allow`, redact reviewer inputs, and cover runtime, error, and timeout paths.

## Longer-term product capabilities

- **Additional provider protocols** — prioritize only providers with a stable protocol and focused contract tests.
- **Extension system dynamic loading** — define module lifecycle, UI context, and tool/command registration boundaries before implementation.
- **LSP integration** — design client lifecycle, diagnostics, and navigation behavior for the Rust runtime.
- **HTML export** — add a standalone export format only after defining how messages, tool calls, and ANSI formatting are represented.
- **Image generation API** — add a provider client and focused tests before exposing image-generation models to users.
- **RPC mode** — define a stable stdin/stdout JSONL command contract for editor and programmatic integrations.
