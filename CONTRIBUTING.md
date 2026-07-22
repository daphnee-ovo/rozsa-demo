# Contributing to Rózsa

## Before You Start

Read `AGENTS.md` for project-specific development rules. If you use an agent, run it from the repository root so it picks up these rules automatically.

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

## Pull Requests

- Keep changes focused and minimal
- Include tests for new functionality
- Do not edit `CHANGELOG.md` — maintainers handle changelog entries
- Run the full check suite before submitting

## Project Name

Rózsa is named after **Rózsa Péter** (1905-1977), a Hungarian mathematician recognized as the founding mother of recursion theory. Her seminal work *Recursive Functions* (1951) systematized the theory that underpins all of modern computation. The name reflects both the recursive nature of an agent that reasons about and modifies code, and a tribute to a pioneer whose contributions were long underrecognized.

## Questions

Open an issue or reach out to maintainers directly.
