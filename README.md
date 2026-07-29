# Rózsa
```
██████   ██████  ███████ ███████  █████
██   ██ ██    ██     ██  ██      ██   ██
██████  ██    ██    ██   ███████ ███████
██  ██  ██    ██   ██         ██ ██   ██
██   ██  ██████  ███████ ███████ ██   ██
```
An interactive coding agent with a native GUI frontend, built in Rust.

Named after **Rózsa Péter** (1905-1977), Hungarian mathematician and the founding mother of recursion theory. Her work on recursive functions laid the theoretical foundation for modern computation — a fitting namesake for a recursive, self-improving agent.

## Architecture

Rózsa is a Cargo workspace with five active crates:

| Crate | Role |
|-------|------|
| `rozsa-model` | LLM abstraction layer — provider registry, streaming, types |
| `rozsa-core` | Agent loop engine — tool trait, execution, hooks |
| `rozsa-app` | Application runtime — product logic, sessions, permissions |
| `rozsa-gui` | Native GUI frontend (Tauri) |
| `rozsa-cli` | Binary entry point (clap) |

Dependency direction: `cli` / `gui` → `app` → `core` → `model`

## Status

Rózsa is under active development, migrating from a TypeScript monorepo. Retired terminal frontends are preserved under `legacy/`; the supported interactive entry point is the GUI.

## Build

```bash
# Rust crates
cargo build
```

## Project Structure

```
crates/          Rust workspace crates
packages/        Legacy TypeScript packages (migration source)
docs/            Documentation
devtools/        Build and check scripts
tests/           Integration tests
tmp/             Temporary files (gitignored)
```

## Contributing and Development

See [CONTRIBUTING.md](CONTRIBUTING.md) for contributor prerequisites, development tools, code style, testing, and pull request requirements.

## License

MIT — see [LICENSE](LICENSE).

This project is derived from [pi](https://github.com/earendil-works/pi-mono) by Mario Zechner.
