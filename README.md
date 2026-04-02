# vtz

Vertz Runtime — a Rust-powered development runtime that runs the full [Vertz](https://github.com/vertz-dev/vertz) stack. Dev server, test runner, compiler, and package manager — all in a single native binary, no Node.js required.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/vertz-dev/vtz/main/install.sh | sh
```

Or via npm (for use within Vertz projects):

```bash
npm install @vertz/runtime
```

## What VTZ Runs

VTZ is purpose-built to run the **entire Vertz framework** — from UI components to server handlers to database queries. Here's what that means in practice:

### Full-Stack Vertz Applications

VTZ runs every layer of a Vertz app out of the box:

| Layer | What VTZ Does |
|-------|---------------|
| **UI Components** | Compiles JSX/TSX with signal-based reactivity transforms, serves components with HMR |
| **Server Handlers** | Runs `src/server.ts` in a persistent V8 isolate, handles `/api/*` routes |
| **SSR** | Server-side renders pages in a persistent V8 isolate with AOT compilation and hydration markers |
| **Database Queries** | Compiles Vertz query expressions with auto-thunking and field selection |
| **Signals & Reactivity** | Detects reactive `let` declarations and auto-wraps them in `signal()` at compile time |
| **CSS** | Extracts, transforms, and hot-reloads CSS with PostCSS and Lightning CSS support |
| **Routing** | Extracts route patterns for static discovery, generates prefetch manifests |
| **Testing** | Runs tests in isolated V8 contexts with vitest-compatible globals |

### Running a Vertz App

```bash
vtz dev      # Start dev server — compiles, serves, and hot-reloads your entire app
vtz test     # Run all tests in parallel V8 isolates
vtz build    # Production build
```

A typical Vertz project structure:

```
my-app/
├── src/
│   ├── pages/          # UI components with signal-based reactivity
│   ├── server.ts       # Server handlers (default export)
│   └── app.tsx         # Root component
├── public/             # Static assets
└── package.json
```

VTZ compiles and serves everything — no separate build step for server code, no separate process for the API, no bundler configuration.

## Capabilities

### Dev Server

- **Hot Module Replacement** — instant updates on save via WebSocket (`/__vertz_hmr`)
- **CSS-only HMR** — stylesheet changes apply without page reload
- **Component-level Fast Refresh** — preserves component state during edits
- **On-demand compilation** — TypeScript and JSX compiled on first request, not upfront
- **Error overlay** — compile and runtime errors displayed in-browser via WebSocket (`/__vertz_errors`)
- **Import rewriting** — bare imports (`react`, `@vertz/ui`) rewritten to browser-compatible `/@deps/` URLs
- **Static file serving** — files from `public/` served directly
- **SPA fallback** — unmatched routes fall back to the HTML shell for client-side routing
- **Port auto-increment** — automatically tries the next port if the default is in use
- **TypeScript type checking** — runs `tsc` or `tsgo` concurrently alongside the dev server
- **Environment variables** — `import.meta.env` with configurable public prefixes (default: `VITE_`)
- **Self-signed TLS** — HTTPS in development via auto-generated certificates
- **Proxy support** — reverse proxy with subdomain routing for microservice development

### Server Runtime

VTZ runs server-side JavaScript in a **persistent V8 isolate** — your `src/server.ts` loads once and stays alive across requests, like Cloudflare Workers:

```ts
// src/server.ts
export default async function handler(req: Request): Promise<Response> {
  if (req.url.endsWith("/api/hello")) {
    return new Response(JSON.stringify({ message: "Hello" }), {
      headers: { "Content-Type": "application/json" },
    });
  }
  return new Response("Not found", { status: 404 });
}
```

- All `/api/*` requests route to the persistent isolate
- Modules loaded once and reused across requests (stateful — database connections persist)
- 10 MB max request body
- CORS headers added automatically in dev mode
- Hot-reloads when server source files change

### Server-Side Rendering

VTZ provides built-in SSR via a **persistent V8 isolate** — modules load once and are reused across renders, matching Cloudflare Workers' execution model:

- AOT single-pass rendering
- CSS collection during render
- Hydration data injection for client-side takeover
- Graceful fallback to client-only shell on SSR failure

### Compiler

A pure Rust compiler built on [oxc](https://oxc.rs/) — no Babel, no SWC, no Node.js in the compilation pipeline:

- **TypeScript** — full syntax support with type stripping
- **JSX** — automatic and classic transform modes
- **Signal transforms** — reactive `let` → `signal()` wrapping
- **Computed transforms** — automatic computed derivations
- **Query auto-thunk** — database queries wrapped for lazy evaluation
- **Field selection** — intelligent column selection for relational queries
- **CSS extraction** — CSS from imports and inline styles extracted and served separately
- **Route splitting** — static route pattern discovery
- **Fast Refresh** — per-component HMR metadata injection
- **Hydration markers** — SSR hydration ID generation
- **Import injection** — auto-imports for signal APIs, context, effects
- **Source maps** — generated for all transforms

### Test Runner

A V8-based test runner with vitest-compatible API:

```bash
vtz test                     # Run all tests
vtz test --filter "auth"     # Filter by name
vtz test --watch             # Re-run on changes
vtz test --bail              # Stop on first failure
vtz test --reporter json     # Machine-readable output
vtz test --coverage          # Collect V8 coverage
```

- **Parallel execution** — configurable concurrency (defaults to CPU count)
- **Per-file isolation** — each test file runs in its own V8 isolate
- **Vitest-compatible globals** — `describe`, `it`, `expect`, `beforeEach`, `afterEach`, `beforeAll`, `afterAll`
- **Modifiers** — `.only`, `.skip`, `.todo` for tests and suites
- **Reporters** — terminal (pretty), JSON, JUnit
- **V8 coverage** — line-level coverage with configurable thresholds (default 95%)
- **DOM shim** — `document`, `window`, `HTMLElement`, `querySelector`, event dispatch, `innerHTML` parsing
- **Preload scripts** — global setup/fixtures
- **Compilation caching** — disk-backed cache for faster re-runs
- **Timeout control** — per-file timeout (default 5000ms)

### Package Manager

```bash
vtz install          # Install all dependencies
vtz add <pkg>        # Add to dependencies
vtz add -D <pkg>     # Add to devDependencies
vtz remove <pkg>     # Remove a package
vtz audit            # Vulnerability scanning
vtz outdated         # Check for updates
vtz update           # Update packages
vtz why <pkg>        # Trace why a package is installed
vtz list             # List installed packages
vtz publish          # Publish to npm
vtz patch <pkg>      # Manage dependency patches
vtz cache            # Manage the package cache
vtz run <script>     # Run package.json scripts
vtz exec <cmd>       # Run commands with node_modules/.bin on PATH
```

- npm registry with ETag-based metadata caching
- Scoped package support (`@scope/pkg`)
- Concurrent resolution (16 parallel registry requests)
- Exponential backoff retry on transient failures

### Runtime APIs

VTZ's V8 runtime exposes standard Web and Node.js APIs — no polyfill packages needed:

| API | Examples |
|-----|----------|
| **Fetch** | `fetch()`, `Request`, `Response`, `Headers` |
| **File System** | `readFile()`, `writeFile()`, `stat()`, `mkdir()` |
| **Crypto** | `crypto.subtle`, SHA-256, HMAC |
| **Encoding** | `TextEncoder`, `TextDecoder`, `btoa()`, `atob()` |
| **Timers** | `setTimeout`, `setInterval`, `setImmediate` |
| **Performance** | `performance.now()` |
| **Streams** | `ReadableStream`, `WritableStream` |
| **URL** | `URL`, `URLSearchParams` |
| **Path** | `path.join()`, `path.resolve()`, etc. |
| **Console** | `console.log()`, `console.error()`, etc. |
| **Structured Clone** | `structuredClone()` |
| **Async Context** | `AsyncLocalStorage` |
| **SQLite** | Built-in SQL database operations |

## Framework Compatibility

### Vertz (Native Support)

VTZ is built for Vertz. Every Vertz feature — signals, computed values, effects, queries, SSR, routing, server handlers — works out of the box with zero configuration. The compiler understands Vertz's reactive model and generates optimized output.

### React (Plugin Support)

VTZ includes a built-in React plugin:

- React 17+ with automatic JSX transform
- React Refresh for component-level HMR
- Standard React development workflow

### Other Frameworks

VTZ has a **pluggable framework architecture** — the `FrameworkPlugin` trait allows custom frameworks to define their own compilation, HMR strategy, import resolution, and HTML shell.

**Can I run Hono on VTZ?**
Not directly. VTZ's server runtime expects a single default export handler in `src/server.ts` — it doesn't expose the raw HTTP listener that Hono expects. However, you could adapt a Hono router to work inside VTZ's handler pattern, since both deal with `Request` → `Response`:

```ts
// Theoretically possible — Hono inside VTZ's handler
import { Hono } from "hono";
const app = new Hono();
app.get("/api/hello", (c) => c.json({ message: "Hello" }));
export default app.fetch;
```

This would require Hono to work in a V8 isolate (no Node.js APIs). Hono's core is runtime-agnostic, so the basic routing and middleware would work, but Node.js-specific middleware would not.

**Can I run TanStack Start, Next.js, or Nuxt?**
No. These are full-stack meta-frameworks with their own compilation pipelines, routing conventions, data loading patterns, and server integration. They expect to control the entire build and serve process. VTZ is a runtime for Vertz, not a generic framework host.

**Can I run a plain TypeScript API server?**
Yes — if your server code exports a `Request → Response` handler and doesn't depend on Node.js-specific APIs (like `http.createServer`, `net.Socket`, or Node streams), it will run in VTZ's server isolate.

## Architecture

VTZ is a Cargo workspace with three crates:

- **vtz** (`native/vtz/`) — the full runtime: V8 engine (via [deno_core](https://github.com/denoland/deno_core)), dev server ([axum](https://github.com/tokio-rs/axum)), test runner, package manager, SSR engine
- **vertz-compiler-core** (`native/vertz-compiler-core/`) — pure Rust compilation library: signal transforms, JSX, CSS extraction, query analysis, route splitting
- **vertz-compiler** (`native/vertz-compiler/`) — NAPI bindings so the framework's Bun plugin uses the same compiler

Key architectural decisions:

- **No Node.js dependency** — V8 runs directly via deno_core, not through Node
- **Pure Rust compiler** — all transforms happen in Rust via oxc, not JavaScript-based tools
- **Persistent V8 isolates** — server code and SSR load modules once and reuse them across requests
- **Plugin system** — framework-specific behavior (Vertz, React) is decoupled from the core runtime

## Commands

```bash
vtz dev              # Start the dev server
vtz test             # Run tests
vtz build            # Production build
vtz install          # Install all dependencies
vtz add <pkg>        # Add a dependency
vtz remove <pkg>     # Remove a dependency
vtz audit            # Check for vulnerabilities
vtz outdated         # Check for outdated packages
vtz update           # Update packages
vtz run <script>     # Run a package.json script
vtz exec <cmd>       # Execute with node_modules/.bin on PATH
vtz why <pkg>        # Trace dependency path
vtz list             # List installed packages
vtz publish          # Publish to npm
vtz patch <pkg>      # Manage dependency patches
vtz cache            # Manage package cache
vtz config           # Manage .vertzrc configuration
vtz proxy            # Local development proxy (subdomain routing)
vtz migrate-tests    # Migrate from bun:test to @vertz/test
```

Both `vtz` and `vertz` work as command names — they are aliases.

## Configuration

VTZ is configured via a `.vertzrc` file at the project root (JSON format). See the full reference: **[docs/configuration.md](docs/configuration.md)**

Key options:

| Option | Description |
|--------|-------------|
| `plugin` | Framework plugin: `"vertz"` or `"react"` (auto-detected by default) |
| `autoInstall` | Auto-install missing packages during dev (default: `true`) |
| `proxy` | Dev server reverse proxy for API forwarding |
| `extraWatchPaths` | Additional directories to watch in monorepo setups |
| `trustScripts` | Packages allowed to run postinstall scripts |

## Development

```bash
cd native
cargo build --release                                    # Build
cargo test --all                                         # Run all tests
cargo clippy --all-targets --release -- -D warnings      # Lint
cargo fmt --all -- --check                               # Format check
cargo fmt --all                                          # Auto-format
```

The built binary is at `native/target/release/vtz`.

## License

MIT
