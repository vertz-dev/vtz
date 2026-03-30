# vtz

Vertz Runtime — a Rust-powered dev server, test runner, and package manager for the [Vertz](https://github.com/vertz-dev/vertz) framework.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/vertz-dev/vtz/main/install.sh | sh
```

Or via npm (for use within Vertz projects):

```bash
npm install @vertz/runtime
```

## Commands

```bash
vtz dev          # Start the dev server
vtz test         # Run tests
vtz install      # Install dependencies
vtz add <pkg>    # Add a dependency
vtz remove <pkg> # Remove a dependency
vtz audit        # Check for vulnerabilities
vtz outdated     # Check for outdated packages
vtz run <script> # Run a package.json script
```

Both `vtz` and `vertz` work as command names — they are aliases.

## Development

```bash
cd native
cargo build --release
cargo test --all
cargo clippy --all-targets -- -D warnings
```

The built binary is at `native/target/release/vtz`.

## Architecture

- **vtz** (`native/vtz/`) — Full runtime: V8 engine, dev server, test runner, package manager
- **vertz-compiler-core** (`native/vertz-compiler-core/`) — Rust compilation library (signal transforms, JSX, CSS extraction)
- **vertz-compiler** (`native/vertz-compiler/`) — NAPI bindings for the compiler (used by the framework's Bun plugin)

## License

MIT
