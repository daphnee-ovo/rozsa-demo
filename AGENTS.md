# Development Rules

## Core Principles

### 0. Safety and Execution Priority

Blocks and prohibitions from external safety systems have the highest execution priority, including hooks, permission systems, sandboxes, CI security checks, and policy engines. The agent must not question, bypass, weaken, or re-evaluate them. When blocked, the agent must stop the relevant action and explain the reason based on the safety system's message.

### 1. Requirement and Change Boundary

**Confirm Requirements First** — When requirements are unclear, clarify before starting work. Any uncertainty that affects implementation direction, modification scope, safety risk, execution cost, or acceptance criteria must not be resolved by assumption.

**Confirm Scope Escalation** — If the actual impact scope is found during execution to be significantly larger than initially expected, the agent must pause and explain the situation to the user. The agent must not expand execution scope on its own.

**Principle of Least Surprise** — Program behavior should match user expectations. Unless explicitly requested by the user, do not proactively refactor, change style, change interfaces, modify unrelated files, or expand scope.

### 2. Documentation and Long-Term Maintenance

**Bidirectional Code-Documentation Traceability** — Code and documentation must be traceable in both directions. A code file header should describe the internal structure of the current file using a tree of classes, functions, methods, or core flows. The code file header should link to related Markdown documentation, and the related documentation should link back to the relevant code file or code directory. Every code or documentation change must check whether the other side remains accurate.

### 3. Implementation Complexity Control

**Prefer Simplicity** — If a simple implementation is enough, do not overcomplicate it. Prefer direct, readable, and easy-to-debug solutions. Avoid unnecessary abstractions, frameworks, layers, or algorithmic complexity introduced only for an appearance of architecture.

**Use Software Leverage** — Prefer reusing mature, reliable, well-maintained tools and libraries. Do not reinvent the wheel. Implement from scratch only when existing solutions are too heavy, unstable, or incompatible with core needs.

**Composition over Integration** — When adding a new feature, prefer creating a new module with a clear responsibility and composing it with existing modules through stable interfaces. Do not pile new logic into old programs or functions merely for convenience.

### 4. Interface and Module Boundary

**High Cohesion, Low Coupling** — Modules should be highly cohesive internally, organizing code around a single responsibility. Modules should be loosely coupled externally, collaborating through clear, stable, and predictable interfaces.

**Extensible, but Do Not Assume Finality** — When designing protocols, file formats, configuration structures, and data structures, leave reasonable room for extension (version fields, self-describing fields, stable interfaces). Extensibility must serve foreseeable evolution and must not introduce excessive abstraction for imaginary futures.

**Centralized Configuration** — Mutable parameters, paths, names, thresholds, feature flags, model names, and environment-specific values should be centralized instead of scattered through business logic.

**Prefer Explicit Constraints** — Code should prefer explicit modeling capabilities provided by the language and type system. Avoid implementation approaches that bypass constraints, hide real types, or rely on runtime conventions.

### 5. Error Handling and Failure Strategy

**Transparent Errors** — Programs must report errors clearly and must not fail silently. When a failure occurs, explain what happened, possible causes, impact scope, and the next handling step.

**Fail Fast** — If complete program operation depends on specific configuration, components, services, permissions, or external resources, check them during startup or before the task begins. If a dependency is missing, fail immediately and explain why.

**No Forced Fallbacks** — Do not use default values, empty results, broad exception catches, or implicit degradation to hide real problems. Fallbacks are allowed only when the fallback behavior is an explicit requirement.

### 6. Testing Constraint

**Trust Tests** — Trust existing test code by default unless there is clear evidence that the tests have become invalid due to major business changes, interface changes, or implementation boundary changes. Do not change tests merely because the implementation fails them.

---

## Conversational Style

- Keep answers short and concise
- No emojis in commits, issues, PR comments, or code
- No fluff or cheerful filler text (e.g., "Thanks @user" not "Thanks so much @user!")
- Technical prose only, be direct
- When the user asks a question, answer it first before making edits or running implementation commands.
- When responding to user feedback or an analysis, explicitly say whether you agree or disagree before saying what you changed.

## Code Quality

- Read files in full before wide-ranging changes, before editing files you have not fully inspected, and when asked to investigate or audit. Do not rely on search snippets for broad changes.
- No `any` unless absolutely necessary.
- Inline single-line helpers that have only one call site.
- Check node_modules for external API types; don't guess.
- **No inline imports** (`await import()`, `import("pkg").Type`, dynamic type imports). Top-level imports only.
- Never remove or downgrade code to fix type errors from outdated deps; upgrade the dep instead.
- Use only erasable TypeScript syntax (Node strip-only mode) in code checked by the root config (`packages/*/src`, `packages/*/test`, `packages/coding-agent/examples`): no parameter properties, `enum`, `namespace`/`module`, `import =`, `export =`, or other constructs needing JS emit. Use explicit fields with constructor assignments.
- Always ask before removing functionality or code that appears intentional.
- Do not preserve backward compatibility unless the user asks for it.
- Never hardcode key checks (e.g. `matchesKey(keyData, "ctrl+x")`). Add defaults to `DEFAULT_EDITOR_KEYBINDINGS` or `DEFAULT_APP_KEYBINDINGS` so they stay configurable.
- Never modify `packages/ai/src/models.generated.ts` directly; update `devtools/before/packages/ai/generate-models.ts` instead.

## Commands

- After code changes (not docs): `npm run check` (full output, no tail). Fix all errors, warnings, and infos before committing. Does not run tests.
- Never run `npm run build` or `npm test` unless requested by the user.
- Never run the full vitest suite directly: it includes e2e tests that activate when endpoint/auth env vars are present. For all non-e2e tests, run `./devtools/before/test.sh` from the repo root. Otherwise run specific tests from the package root: `node ../../node_modules/vitest/dist/cli.js --run test/specific.test.ts`.
- If you create or modify a test file, run it and iterate on test or implementation until it passes.
- For `packages/coding-agent/test/suite/`, use `test/suite/harness.ts` + the faux provider. No real provider APIs, keys, or paid tokens.
- Put issue-specific regressions under `packages/coding-agent/test/suite/regressions/` named `<issue-number>-<short-slug>.test.ts`.
- For ad-hoc scripts, `write` them to a temp file (e.g. `/tmp`), run, edit if needed, remove when done. Don't embed multi-line scripts in `bash` commands.
- Never commit unless the user asks.

## Dependency and Install Security

- Treat npm dep and lockfile changes as reviewed code. Direct external deps stay pinned to exact versions.
- Hydrate/update locally with `npm install --ignore-scripts`; clean/CI-style with `npm ci --ignore-scripts`. Don't run lifecycle scripts unless the user asks.
- If dep metadata changes, refresh `package-lock.json` with `npm install --package-lock-only --ignore-scripts`.
- If `packages/coding-agent/npm-shrinkwrap.json` needs regen, run `node devtools/before/generate-coding-agent-shrinkwrap.mjs` (verify with `--check` or `npm run check`). New deps with lifecycle scripts require review and an explicit allowlist entry in that script; never add one silently.
- Pre-commit blocks lockfile commits unless `ROZSA_ALLOW_LOCKFILE_CHANGE=1`. Don't bypass unless the user wants the lockfile change committed.

## Git

Multiple rozsa sessions may be running in this cwd at the same time, each modifying different files. Git operations that touch unstaged, staged, or untracked files outside your own changes will stomp on other sessions' work. Follow these rules:

Committing:

- Only commit files YOU changed in THIS session.
- Stage explicit paths (`git add <path1> <path2>`); never `git add -A` / `git add .`.
- Before committing, run `git status` and verify you are only staging your files.
- `packages/ai/src/models.generated.ts` may always be included alongside your files.

Never run (destroys other agents' work or bypasses checks):

- `git reset --hard`, `git checkout .`, `git clean -fd`, `git stash`, `git add -A`, `git add .`, `git commit --no-verify`.

If rebase conflicts occur:

- Resolve conflicts only in files you modified.
- If a conflict is in a file you did not modify, abort and ask the user.
- Never force push.

## Issues and PRs

See `CONTRIBUTING.md` for contribution guidelines.

When closing issues via commit:

- Include `fixes #<number>` or `closes #<number>` in the message so merging auto-closes the issue. For multiple issues, repeat the keyword per issue (`closes #1, closes #2`); a shared keyword (`closes #1, #2`) only closes the first.

## Testing Interactive Mode with tmux

Run the TUI in a controlled terminal (from the repo root):

```bash
tmux new-session -d -s rozsa-test -x 80 -y 24
tmux send-keys -t rozsa-test "./devtools/before/pi-test.sh" Enter
sleep 3 && tmux capture-pane -t rozsa-test -p     # capture after startup
tmux send-keys -t rozsa-test "your prompt here" Enter
tmux send-keys -t rozsa-test Escape               # special keys (also C-o for ctrl+o, etc.)
tmux kill-session -t rozsa-test
```

## Changelog

Location: `packages/*/CHANGELOG.md` (one per package).

Sections under `## [Unreleased]`: `### Breaking Changes` (API changes requiring migration), `### Added`, `### Changed`, `### Fixed`, `### Removed`.

Rules:

- All new entries go under `## [Unreleased]`. Read the full section first and append to existing subsections; never duplicate them.
- Released version sections (e.g. `## [0.12.2]`) are immutable; never modify them.

## User Override

If the user's instructions conflict with any rule in this document, ask for explicit confirmation before overriding. Only then execute their instructions.
