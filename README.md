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

Rózsa is under active development. The native Tauri GUI is the supported interactive frontend; the CLI runs one-shot prompts or launches that GUI.

## Build

```bash
# Rust crates
cargo build
```

## Run the GUI on macOS

```bash
./run.sh
```

The script builds the debug executable, stages `target/debug/Rózsa.app`, and
launches that app bundle so the Dock uses the development application icon. Use
`./run.sh --prepare-only` to stage and validate the bundle without launching it.

## Project Structure

```
crates/          Rust workspace crates
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
