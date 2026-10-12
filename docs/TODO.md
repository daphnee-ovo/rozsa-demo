# TODO

## OAuth follow-up

- **Verify and replace OpenAI Codex `originator=pi` only after an authorization-flow regression test exists.** It is intentionally retained today in [`build_auth_url`](../crates/rozsa-model/src/oauth/openai_codex.rs) and [`test_build_auth_url`](../crates/rozsa-model/tests/oauth_openai_codex.rs). Before changing it, compare the current OpenAI Codex login implementation, establish an official replacement value or acceptance evidence, and add a test that exercises the resulting authorization URL. Do not change this query parameter on inference alone: it is sent only when a user begins a new OpenAI Codex OAuth login.

## Deferred product work

- **权限面板状态与超时设计缺陷（统一 UI 完成后处理）**（进入日期：2026-09-30） — 关联现有 [GAP-008A / GAP-008B](../prototype/docs/DESIGN_GAPS.md#permissionrequest-与-askuserquestion)。本条只记录后续需求，不扩展当前共享 UI 迭代或授权现在修改实现。
  - 未决权限请求的面板只有两种状态：**等待选择**与**挂起**。当前活动会话的权限面板必须显示并等待明确选择，不允许通过关闭、Escape、点击外部或其他隐藏操作绕过请求。
  - 用户确认并且真实权限回复成功后，结束该请求并关闭面板；回复失败不能当成确认成功或清除未决请求。
  - 请求发生时所属会话已经在后台，或者请求出现后用户切换到其他会话，都进入挂起状态；焦点回到所属会话时，恢复显示同一未决请求并等待选择。挂起只影响呈现，不等于批准、拒绝或丢弃请求。
  - 提供是否启用**超时拒绝**的设置；启用时，请求在 **10 分钟**超时后自动拒绝并关闭面板。后台挂起的请求若按最终计时规则超时，也必须结束请求，不能在返回会话时重新显示已结束的面板。
  - 后续确认超时开关默认值、10 分钟的计时起点以及挂起期间是否继续计时；本条不预设这些未明确规则。
  - 后续实现须覆盖请求在后台产生、展示后切换会话、返回恢复、用户确认与超时竞争、回复失败和多请求排队的回归验证；每个请求只能被实际解决一次。
- **Dev-flow automatic installation and Rózsa setup** — define a signed,
  failure-transparent install/setup contract across Rózsa and dev-flow before
  making the missing-CLI recommendation actionable. The current Settings UI
  must not imply that installation is already available.
- **Dev-flow system-prompt injection** — coordinate the prompt fragment,
  compatibility/version handshake, ownership, session-refresh semantics and
  disable behavior with dev-flow. Until then, `Enable Dev Flow` controls only
  implemented runtime/UI integration and must not claim to modify prompts.
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
