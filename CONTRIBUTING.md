# Contributing to Rózsa

## Before You Start

Read `AGENTS.md` for project-specific development rules. If you use an agent, run it from the repository root so it picks up these rules automatically.

## Requirements

- Rust (stable toolchain)
- Node.js >= 22.19.0 (for legacy TypeScript packages)
- npm

## Development Workflow

```bash
# Rust
cargo build
cargo test

# TypeScript (legacy, still active during migration)
npm install --ignore-scripts
npm run check          # lint + format + type check
./devtools/before/test.sh   # run tests
```

Both `cargo test` and `npm run check` must pass before submitting changes.

## Code Style

- Rust: standard `rustfmt` and `clippy`
- TypeScript: Biome (lint + format), erasable syntax only

## Pull Requests

- Keep changes focused and minimal
- Include tests for new functionality
- Do not edit `CHANGELOG.md` — maintainers handle changelog entries
- Run the full check suite before submitting

## Project Name

Rózsa is named after **Rózsa Péter** (1905-1977), a Hungarian mathematician recognized as the founding mother of recursion theory. Her seminal work *Recursive Functions* (1951) systematized the theory that underpins all of modern computation. The name reflects both the recursive nature of an agent that reasons about and modifies code, and a tribute to a pioneer whose contributions were long underrecognized.

## Questions

Open an issue or reach out to maintainers directly.
