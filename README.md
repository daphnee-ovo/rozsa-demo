# Rózsa
```
██████   ██████  ███████ ███████  █████
██   ██ ██    ██     ██  ██      ██   ██
██████  ██    ██    ██   ███████ ███████
██  ██  ██    ██   ██         ██ ██   ██
██   ██  ██████  ███████ ███████ ██   ██
```
An interactive coding agent with a terminal frontend, built in Rust.

Named after **Rózsa Péter** (1905-1977), Hungarian mathematician and the founding mother of recursion theory. Her work on recursive functions laid the theoretical foundation for modern computation — a fitting namesake for a recursive, self-improving agent.

## Architecture

Rózsa is a Cargo workspace with five crates:

| Crate | Role |
|-------|------|
| `rozsa-model` | LLM abstraction layer — provider registry, streaming, types |
| `rozsa-core` | Agent loop engine — tool trait, execution, hooks |
| `rozsa-app` | Application runtime — product logic, sessions, permissions |
| `rozsa-tui` | Terminal frontend (ratatui) |
| `rozsa-cli` | Binary entry point (clap) |

Dependency direction: `cli` / `tui` → `app` → `core` → `model`

## Status

Rózsa is under active development, migrating from a TypeScript monorepo. The legacy TypeScript packages remain under `packages/` and will be gradually replaced.

## Build

```bash
# Rust crates
cargo build

# Legacy TypeScript (still active)
npm install --ignore-scripts
npm run build
npm run check
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

## License

MIT — see [LICENSE](LICENSE).

This project is derived from [pi](https://github.com/earendil-works/pi-mono) by Mario Zechner.
