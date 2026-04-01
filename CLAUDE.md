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
- See `.claude/rules/workflow.md` for branch naming, commits, and PR requirements.

## Conventions

- Strict TDD: Red → Green → Refactor. Every behavior needs a failing test first.
- Run quality gates (clippy, fmt, test) after every green.
- No `unsafe` without a `// SAFETY:` comment explaining the invariant.
- No `#[allow(clippy::*)]` without a comment explaining why.
- Prefer `thiserror` for error types.
- See `.claude/rules/` for detailed guidelines.

## Quality Gates (must all pass before push)

```bash
cargo test --all && cargo clippy --all-targets --release -- -D warnings && cargo fmt --all -- --check
```

## Canonical App Structure

Every Vertz app follows this structure. Agents must scaffold apps consistently:

```
src/
  app.tsx          # SSR module — exports matching SSRModule interface
  entry-client.ts  # Client hydration — calls hydrate(App, { target: '#app' })
  server.ts        # API server — exports { handler } via createServer()
```

### `src/app.tsx` — SSR Entry

Loaded by the persistent V8 isolate for server-side rendering. Must export:

- `App` — Root component function (returns DOM/VNode)
- `styles?` — Global CSS strings (e.g., resets, body styles)
- `theme?` — Theme object from `@vertz/ui`
- `getInjectedCSS?` — Returns CSS from `@vertz/ui` module instance
- `routes?` — Compiled routes for build-time SSG
- `api?` — Code-generated API client for zero-discovery prefetch

### `src/entry-client.ts` — Client Entry

Browser-only hydration script. Referenced by `<script type="module">` in the HTML shell. Imports `App` from `./app` and calls the framework's `hydrate()`.

### `src/server.ts` — API Server

Exports a request handler for `/api/*` routes. Loaded by the persistent isolate as `globalThis.__vertz_server_module`. During SSR, `/api/*` fetch calls are intercepted and routed to this handler in-memory (no network hop).

### SSR Data Flow

```
HTTP Request → Extract cookies/session → Persistent V8 isolate
  → ssrRenderSinglePass(appModule, url, { ssrAuth, cookies })
  → Framework: discovery → prefetch (via scoped fetch) → render
  → SsrResponse { content, css, ssr_data, head_tags, redirect }
  → Assemble HTML document → HTTP Response
```
