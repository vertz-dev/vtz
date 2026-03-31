# Policies

## Breaking Changes (Pre-v1)

- All crates pre-v1 — no external users
- Breaking changes encouraged — adopt better designs
- No backward-compat shims, no migration guides, no deprecated aliases
- Consolidate aggressively (merge modules, move functions)
- Only pause if it affects active PR / in-progress work

## Versioning

- All crates share the same version (stamped via `scripts/bump-version.sh`)
- Version lives in `version.txt` at repo root
- npm packages (`npm/runtime*`) version in lockstep with Rust crates

## Linting & Formatting

- **Linter:** `cargo clippy --all-targets --release -- -D warnings`
- **Formatter:** `cargo fmt --all`
- All clippy warnings are errors in CI (`-D warnings`)
- No `#[allow(clippy::*)]` without a comment explaining why
- No `unsafe` without a `// SAFETY:` comment explaining the invariant
- Prefer `thiserror` for error types, `anyhow` for application-level errors
