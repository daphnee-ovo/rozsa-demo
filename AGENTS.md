# Development Rules

## Overview
> Use `CONTRIBUTING.md` to document development guidelines shared by both human contributors and agents.

@CONTRIBUTING.md

## Project Scope

Rózsa is a Rust 2024 Cargo workspace. The supported interactive frontend is the native Tauri GUI.

| Crate | Responsibility |
|---|---|
| `rozsa-model` | LLM providers, streaming, credentials, and model types |
| `rozsa-core` | Agent loop, tools, hooks, and execution primitives |
| `rozsa-app` | Product runtime, sessions, permissions, settings, and orchestration |
| `rozsa-gui` | Native Tauri GUI |
| `rozsa-cli` | CLI entry point |

Dependency direction is `rozsa-cli` / `rozsa-gui` → `rozsa-app` → `rozsa-core` → `rozsa-model`. Preserve these boundaries.

The active product is implemented entirely in the five Rust crates above. Do not introduce dependencies on retired terminal, TypeScript, or process-bridge implementations.

## Cross-Platform Compatibility

- Multi-system compatibility applies to every crate and module, not only `rozsa-gui` or UI components.
- New and changed behavior must preserve macOS, Linux, and Windows support where the module is applicable.
- Keep operating-system-specific paths, environment discovery, process control, file permissions, shell integration, and native APIs behind explicit platform adapters or `cfg` boundaries; do not assume macOS behavior in shared model, core, app, or CLI code.
- Add platform-specific fixtures or tests when behavior differs by operating system, and keep the common path deterministic and safe.

## Documentation

- Keep code, `Related Docs`, and Markdown backlinks synchronized.
- After structural Rust changes, run `make-tree --write <FILE>` on affected supported files; never hand-edit generated `FrameworkTree` sections.
- Update this file when project structure, conventions, commands, or workflows change.
- Do not edit dev-flow-managed files or generated sections by hand.

## Code Quality

- Use standard `rustfmt` and `clippy` conventions.
- Model variants and invariants with Rust types. Avoid string dispatch, opaque containers, and `dyn Any` when an enum, trait, or concrete type can express the constraint.
- Keep shared dependencies in the workspace dependency table when multiple crates use them.
- Never add `#[test]` or `#[cfg(test)]` blocks under `crates/*/src/`. Put tests in the relevant crate's `tests/` directory or the root `tests/` tree.
- Do not remove intentional behavior without explicit approval.
- Do not preserve backward compatibility unless the user asks for it.
- Use concise technical prose. Do not add emojis to code, commits, issues, or PR comments.

## Commands

- Run the smallest relevant Cargo test target first.
- If a Rust test file changes, run its relevant test target before broader checks.
- Full project verification is `cargo build`, `cargo clippy`, and `cargo test` from the repository root.
- Use `cargo fmt --all -- --check` for formatting verification.
- On macOS, use `./run.sh` to launch the GUI from a staged debug `.app` bundle with the development Dock icon; use `./run.sh --prepare-only` to validate the bundle without launching it.
- Run tests that depend on real macOS FSEvents, filesystem watchers, or watcher-driven SSE updates with host permissions. A sandbox may allow REST and socket traffic while suppressing FSEvents; reproduce a watcher timeout on the host before treating it as a product defect.
- Run `./devtools/sync-codex-model-client-version.sh` to update the models endpoint compatibility version directly from `openai/codex` GitHub tags; use `--check` for verification.
- When verification requires opening the app, close the test app immediately after testing; do not leave a validation instance running in the user's session.
- Never commit unless the user asks.

## Dependencies

- Treat `Cargo.toml` and `Cargo.lock` changes as reviewed code.
- Use workspace dependencies for versions shared across crates.

## Docs
- Keeping documentation up to date and ensuring it accurately reflects the current state of the code is as important as writing the code itself.


## Git

Multiple rozsa sessions may be running in this cwd at the same time, each modifying different files. Git operations that touch unstaged, staged, or untracked files outside your own changes will stomp on other sessions' work. Follow these rules:

Committing:

- Only commit files YOU changed in THIS session.
- Stage explicit paths (`git add <path1> <path2>`); never `git add -A` / `git add .`.
- Before committing, run `git status` and verify you are only staging your files.

Never run (destroys other agents' work or bypasses checks):

- `git reset --hard`, `git checkout .`, `git clean -fd`, `git stash`, `git add -A`, `git add .`, `git commit --no-verify`.

If rebase conflicts occur:

- Resolve conflicts only in files you modified.
- If a conflict is in a file you did not modify, abort and ask the user.
- Never force push.

## GUI

- Treat `docs/gui/UI_USAGE_GUIDELINES.md`, `docs/gui/ARCHITECTURE.md`, `docs/gui/TERMINOLOGY.md`, and `docs/gui/prototype/` as the GUI design sources.
- New GUI elements must preserve the established visual and interaction language.
- If the prototype conflicts with an explicit user requirement, the user requirement wins. Obtain approval before modifying the prototype to restore product/prototype consistency.
- Shared prototype CSS and JavaScript may be extracted when doing so improves scene reuse without changing behavior.
- GUI components that interact with the operating system, such as native file pickers, must support macOS, Linux, and Windows in addition to the cross-platform requirements above.
- Use Gaussian blur to make hierarchical relationships feel more elegant.
