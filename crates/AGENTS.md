# AGENTS.md

## Code Quality

- Use standard `rustfmt` and `clippy` conventions.
- Model variants and invariants with Rust types. Avoid string dispatch, opaque containers, and `dyn Any` when an enum, trait, or concrete type can express the constraint.
- Keep shared dependencies in the workspace dependency table when multiple crates use them.
- Never add `#[test]` or `#[cfg(test)]` blocks under `crates/*/src/`. Put tests in the relevant crate's `tests/` directory or the root `tests/` tree.

## Command

- If a Rust test file changes, run its relevant test target before broader checks.
- Full project verification is `cargo build`, `cargo clippy`, and `cargo test` from the repository root.
- Use `cargo fmt --all -- --check` for formatting verification.
