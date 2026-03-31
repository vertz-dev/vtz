# VTZ — Vertz Runtime

**Read the framework's [VISION.md](https://github.com/vertz-dev/vertz/blob/main/VISION.md) and [MANIFESTO.md](https://github.com/vertz-dev/vertz/blob/main/MANIFESTO.md) before making any design decision.** The runtime exists to serve the framework's principles.

## Stack

- Language: Rust (2021 edition)
- Linter: clippy
- Formatter: rustfmt
- Test runner: `cargo test`
- Async runtime: Tokio
- JS engine: V8 via deno_core
- HTTP server: axum
- Cargo workspace under `native/`

## Development

```bash
cd native
cargo test --all           # Run all tests
cargo clippy --all-targets --release -- -D warnings  # Lint
cargo fmt --all -- --check # Format check
cargo fmt --all            # Auto-format
cargo build --release      # Release build
```

## Crate Structure

- **vtz** (`native/vtz/`) — Full runtime: V8 dev server, test runner, package manager
- **vertz-compiler-core** (`native/vertz-compiler-core/`) — Rust compilation library (transforms, JSX, CSS)
- **vertz-compiler** (`native/vertz-compiler/`) — NAPI bindings for the framework's Bun plugin

## Git

- **NEVER commit or push directly to `main`.** Always create a branch and open a PR.
- See `.codex/` for branch naming, commits, and PR requirements.

## Conventions

- Strict TDD: Red → Green → Refactor. Every behavior needs a failing test first.
- Run quality gates (clippy, fmt, test) after every green.
- No `unsafe` without a `// SAFETY:` comment explaining the invariant.
- No `#[allow(clippy::*)]` without a comment explaining why.
- Prefer `thiserror` for error types.
- See `.codex/` for detailed guidelines.

## Quality Gates (must all pass before push)

```bash
cargo test --all && cargo clippy --all-targets --release -- -D warnings && cargo fmt --all -- --check
```
