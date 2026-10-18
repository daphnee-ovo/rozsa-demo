> AGENTS.md 按作用域拆分，根目录只留最通用规则。
> AGENTS.md , CONTRIBUTING.md 存放内容原则：只放无法从代码、文档中直接推出来的原则。

<!--这里是临时计划存放点，由用户写入，AGENT提醒。请在合适实际提醒用户处理下列内容。
- 按照存放内容原则清理、切分 `AGENTS.md`,`CONTRIBUTING.md`
-->

# Development Rules
> JSON-RPC 禁止手动实现：Go 一律使用 `go.lsp.dev/jsonrpc2` 的原生 Conn/Stream；Electron main 使用 `vscode-jsonrpc`。配置需求只使用库公开支持的配置；仍不支持时送回主会话重新考量，不用自定义 Stream、framing 或额外协议解析器补齐。
**IMPORTANT: 修改路径下存放的`AGENTS.md`是必读项。**同一路径只需阅读一次。 例如：编辑`./backend/*`，必先阅读`./backend/AGENTS.md`
- Linux/Windows 实机验证暂时不在当前开发计划内
- 单个代码文件代码部分长度不得超过1000行

<!--待细化-->
- 不用做看起来有用但实际上作用有限而且又浪费时间的行为，如哈希固定
- 严格限制文档、业务代码、测试代码、注释的膨胀
<!--待细化-->

<!--迁移完成后删除-->
针对 `refactor/electron-go-migration` 的临时规定：
- 禁止像时尚单品一样滥用 `migration`、`eletron`
- migration 是过程，如果一个片段、一个文件最终会持久存在，那么不应该带上 `migration` 字眼；如果是临时存在，只服务于 `migration` 这一短期过程的，那么文件应该放入 `tmp/`
- 类似的， 组件、服务等应该直接以其作用命名，不用 `eletron` 做语焉不详的命名前缀
- 旧版、迁移后的前版相互独立。后续旧版会被移除，不应该出现两个版本交叉依赖的情况。
<!--迁移完成后删除-->


## Overview
> Use `CONTRIBUTING.md` to document development guidelines shared by both human contributors and agents.

@CONTRIBUTING.md

## Project Scope

| Crate | Responsibility |
|---|---|
| `rozsa-model` | LLM providers, streaming, credentials, and model types |
| `rozsa-core` | Agent loop, tools, hooks, and execution primitives |
| `rozsa-app` | Product runtime, sessions, permissions, settings, and orchestration |
| `rozsa-gui` | Native Tauri GUI |
| `rozsa-cli` | CLI entry point |

The active product is implemented entirely in the five Rust crates above. Do not introduce dependencies on retired terminal, TypeScript, or process-bridge implementations.

On the migration branch, Rust/Tauri owns `crates/rozsa-gui/frontend/`, its Rust
integration tests and `crates/rozsa-gui/js-tests/`. Electron+Go owns `desktop/renderer/`
and `desktop/tests/`. Only the new frontend and `prototype/` share one source.
Both projects must build, run and test without the other project's sources,
resources or generated output. Tauri must not require Node/Vite or desktop assets;
Electron must not read `crates/rozsa-gui`. Keep this boundary until T061's approved final cutover.

The `refactor/electron-go-migration` branch also contains an incremental candidate:
`backend/` is a Go module, and `desktop/` contains TypeScript Electron main,
sandbox preload, and a single renderer. Its current implemented slice is the
read-only session catalog, Go JSONL tree/context reader, and Electron session
switching/history display with per-session scroll/expansion state. It uses current Rust sources as the behavioral
reference; it does not reuse retired implementations or replace the default
Tauri entry. Follow [migration tasks](docs/migration/TASK_ROADMAP.md) and the
active dow task scope. Candidate commands and verification limits are in
[CONTRIBUTING.md](CONTRIBUTING.md).

## Cross-Platform Compatibility

- Multi-system compatibility applies to every crate and module, not only `rozsa-gui` or UI components.
- New and changed behavior must preserve macOS, Linux, and Windows support where the module is applicable.
- Keep operating-system-specific paths, environment discovery, process control, file permissions, shell integration, and native APIs behind explicit platform adapters or `cfg` boundaries; do not assume macOS behavior in shared model, core, app, or CLI code.
- Add platform-specific fixtures or tests when behavior differs by operating system, and keep the common path deterministic and safe.

## Documentation

- Keeping documentation up to date and ensuring it accurately reflects the current state of the code is as important as writing the code itself.
- Keep code, `Related Docs`, and Markdown backlinks synchronized.
- Update this file when project structure, conventions, commands, or workflows change.

## Code Quality

- 若旧式代码说明、风格与用户要求冲突，简单提醒，但仍以用户要求为准。
- Do not remove intentional behavior without explicit approval.
- Do not preserve backward compatibility unless the user asks for it.
- Use concise technical prose. Do not add emojis to code, commits, issues, or PR comments.

## Commands

- Run the smallest relevant Cargo test target first.
- The Go minimum is declared in `backend/go.mod`; use `go -C backend ...` for native module toolchain selection and keep software cache defaults. Tests should protect behavior, meaningful contracts, or regressions—not fixed tool versions, source text, or restatements of configuration.
- Desktop tool versions come from the relevant manifests and lockfiles. Keep product build commands, output paths, and focused verification instructions in [CONTRIBUTING.md](CONTRIBUTING.md); keep the approved product architecture and behavior decisions in the SPEC.
- For small features, use the implementation language's test runner directly. JavaScript tests belong in the module's `js-tests/` directory; all Electron tests live in `desktop/tests/`. Never wrap Node tests in Rust/Cargo subprocesses.
- Go tests live in dedicated `*_test.go` files beside their module;
  Electron main/preload/bridge/renderer/native host tests all live in `desktop/tests/`.
- Prefer shell scripts under `devtools/` for whole-module integration and end-to-end tests across languages. See `CONTRIBUTING.md` for the GUI test commands and verification boundaries.
- On macOS, use `./run.sh` to launch the GUI from a staged debug `.app` bundle with the development Dock icon; use `./run.sh --prepare-only` to validate the bundle without launching it.
- Run tests that depend on real macOS FSEvents, filesystem watchers, or watcher-driven SSE updates with host permissions. A sandbox may allow REST and socket traffic while suppressing FSEvents; reproduce a watcher timeout on the host before treating it as a product defect.
- Run `./devtools/sync-codex-model-client-version.sh` to update the models endpoint compatibility version directly from `openai/codex` GitHub tags; use `--check` for verification.
- When verification requires opening the app, close the test app immediately after testing; do not leave a validation instance running in the user's session.

## Git
> 用户已经授权，并且是本项目的固定规则：完成一个task 就commit一次。
> Dev-flow 的task,issue文件直接随commit一并提交，***禁止*通过git diff等方式检查或查看**。使用 `git add .dev-doc/<branch>/<name>/{tasks,issue}/` 快速staged。
> codex: 写入 git 前需要先提权

Multiple rozsa sessions may be running in this cwd at the same time, each modifying different files. Git operations that touch unstaged, staged, or untracked files outside your own changes will stomp on other sessions' work. Follow these rules:

Committing:

- Migration task completion and commit sequence are documented in [CONTRIBUTING.md](CONTRIBUTING.md).
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

- Treat `docs/gui/UI_USAGE_GUIDELINES.md`, `docs/gui/ARCHITECTURE.md`, `docs/gui/TERMINOLOGY.md`, and root `prototype/` as the GUI design sources.
- New GUI elements must preserve the established visual and interaction language.
- If the prototype conflicts with an explicit user requirement, the user requirement wins. Obtain approval before modifying the prototype to restore product/prototype consistency.
- Shared prototype CSS and JavaScript may be extracted when doing so improves scene reuse without changing behavior.
- GUI components that interact with the operating system, such as native file pickers, must support macOS, Linux, and Windows in addition to the cross-platform requirements above.
- Use Gaussian blur to make hierarchical relationships feel more elegant.

## Test
- For codex: `dow task done <ids>` 会自动运行相关测试，因此需要提权运行。

## Test Writing
- 有效的测试是在验证代码的行为，而不是做硬编码的断言。

## ABOUT TODO

新增待办必须标注 `进入日期：YYYY-MM-DD`，按 Asia/Singapore 日期记录。后续修改条目时保留原进入日期，方便判断是否过时；历史条目仅在有可靠记录时补记，不猜填日期。
