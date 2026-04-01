# Design: Align Dev Server SSR with @vertz/ui-server

## Goal

Replace the vtz dev server's custom SSR system (DOM shim + innerHTML scraping) with the framework's own `ssrRenderSinglePass()` from `@vertz/ui-server`. Dev must mirror how production works on Cloudflare Workers — same rendering path, same fetch interception, same data pre-fetching. Two-pass SSR has been removed from the framework — single-pass discovery + prefetch + render is the only SSR model going forward.

## Manifesto Alignment

- **Dev === Production** — SSR in dev uses the exact same `ssrRenderSinglePass()` that Workers use. No divergent code paths that mask production bugs.
- **Simplicity** — Remove the ad-hoc DOM shim SSR rendering. Let the framework own its rendering contract. vtz just wires the plumbing.
- **LLM-first** — A canonical app structure (`app.tsx` = SSR module, `entry-client.ts` = hydration, `server.ts` = API) is easy for agents to scaffold consistently.
- **Robustness** — Single-pass rendering with query pre-fetching works in dev, not just production. No surprises at deploy time.

## Non-Goals

- Removing the DOM shim entirely — it's still needed for the V8 environment (compiled JSX calls `document.createElement`)
- Supporting non-Vertz frameworks for SSR (React plugin has its own SSR path)
- Server-side rendering without `@vertz/ui-server` installed (it's a required dependency)
- Changing the `@vertz/ui-server` API — we consume it as-is
- Production build pipeline changes — this is dev server only

## Unknowns

1. **AsyncLocalStorage in deno_core** — `@vertz/ui-server`'s `runWithScopedFetch()` uses `AsyncLocalStorage`. Does deno_core's V8 support this? If not, we need a shim or fall back to the global fetch interception pattern.
   - Resolution: POC in Phase 1
2. **Single-pass rendering performance** — `ssrRenderSinglePass()` does discovery → prefetch → render in one pipeline. Should be fast, but needs measurement in dev.
   - Resolution: Measure in Phase 2, optimize if >100ms

---

## Current Architecture (what we're replacing)

```
Browser → GET / → axum handler
  → persistent isolate SSR:
    1. SSR_RESET_JS: wipe #app, clear CSS
    2. set_ssr_location(url)
    3. SSR_RENDER_JS: read #app.innerHTML → empty → "client-only"
  → per-request SSR fallback:
    1. Fresh V8 + DOM shim
    2. Load entry-client.ts → mount(App) → populates #app
    3. Read #app.innerHTML
```

**Why it fails**: Persistent isolate loads `entry-client.ts` once at init. `mount()` populates `#app` once. `SSR_RESET_JS` wipes it per request. Nothing re-renders.

## Target Architecture

```
Browser → GET / → axum handler
  → persistent isolate SSR:
    1. Reset DOM state (same)
    2. Set location + session (same)
    3. Run: ssrRenderSinglePass(appModule, url, { ssrTimeout, ssrAuth })
       → Discovery: run App() → capture query registrations
       → Prefetch: await all queries (with per-query timeout)
       → Render: fresh context + pre-populated cache → render to HTML
    4. Return { html, css, ssrData, headTags }
  → axum assembles full HTML document with SSR content + CSS + hydration data
```

Mirrors the Cloudflare Workers model:
```
Worker → fetch(request) → createHandler({
  app: () => createServer({ entities, db }),
  ssr: { module: appModule }
})
  → API routes: app.handler(request)
  → Page routes: ssrRenderSinglePass(module, url) + runWithScopedFetch()
```

Note: AOT rendering (`ssrRenderAot()`) is a production-time optimization that generates
pre-compiled string-builder functions at build time. In dev, we always use `ssrRenderSinglePass()`
which is the single-pass discovery pipeline. AOT falls back to single-pass for non-AOT routes.

---

## API Surface

### App Module Contract (src/app.tsx)

```typescript
// SSR module exports — consumed by ssrRenderSinglePass()
// Matches SSRModule interface from @vertz/ui-server
export function App(): Element { /* root component */ }
export const theme: Theme;                            // optional
export const styles: string[];                         // optional, global CSS
export { getInjectedCSS } from '@vertz/ui';           // component CSS bridge
export const routes: CompiledRoute[];                  // optional, for AOT
export const api: Record<string, Record<string, Function>>;  // optional, for zero-discovery
```

### Entry Client (src/entry-client.ts) — browser only, NOT loaded for SSR

```typescript
import { hydrate } from '@vertz/ui';
import { App, styles, theme } from './app';
hydrate(App, { target: '#app', theme, styles });
```

### Canonical App Structure

| File | Purpose | Used by |
|------|---------|---------|
| `src/app.tsx` | SSR module — exports `App`, `theme`, `styles`, `getInjectedCSS` | SSR + client |
| `src/entry-client.ts` | Client hydration — calls `hydrate(App)` | Browser only |
| `src/server.ts` | API server — `export default createServer(...)` | V8 isolate + Workers |

---

## Type Flow Map

```
app.tsx exports { App, theme, styles, getInjectedCSS }
  → V8 module loader compiles with target="ssr"
  → Module cached as globalThis.__vertz_app_module
  → ssrRenderSinglePass(module, url) consumes { App/default, theme, styles, getInjectedCSS }
    → Discovery: App() triggers query() registrations in AsyncLocalStorage context
    → Prefetch: queries resolved with per-query timeout (default 300ms)
    → Render: fresh context + cache → single DOM traversal → HTML string
  → Returns SSRRenderResult { html, css, ssrData, headTags, redirect }
  → Rust deserializes JSON → SsrResponse
  → axum assembles HTML document → Response
```

No dead generics. Every field flows from app module to HTTP response.

**CSS collection**: Three sources (deduplicated):
1. Theme CSS — compiled from `module.theme` via `compileTheme()`
2. Global styles — `module.styles` array
3. Component CSS — from `module.getInjectedCSS()` (needed because SSR bundle inlines a separate @vertz/ui instance)

---

## E2E Acceptance Tests

### SSR returns server-rendered HTML

```
Input:  GET / with Accept: text/html
        src/app.tsx exports App() returning <div data-testid="app-root"><h1>Hello SSR</h1></div>

Output: HTTP 200, Content-Type: text/html
        Body contains: <div id="app"><div data-testid="app-root"><h1>Hello SSR</h1></div></div>
        Body contains: <script>window.__VERTZ_SSR_DATA__=
        Log shows: [SSR] / rendered in Xms (ssr-persistent)
```

### Query pre-fetching works

```
Input:  GET /tasks with Accept: text/html
        app.tsx has: const tasks = query('/api/tasks')
        server.ts handles: GET /api/tasks → [{ id: 1, title: "Test" }]

Output: HTTP 200
        Body contains rendered task list HTML
        Body contains: __VERTZ_SSR_DATA__ with pre-fetched tasks data
        No network request to /api/tasks (handled in-memory)
```

### Redirect on auth failure

```
Input:  GET /dashboard (ProtectedRoute) with no session cookie

Output: HTTP 302 Location: /login
        OR: HTML with client-side redirect meta tag
```

---

## Risks

| Risk | Impact | Mitigation |
|------|--------|------------|
| AsyncLocalStorage not available in deno_core | Blocks scoped fetch | Phase 1 POC. Fallback: use global fetch interception scoped with a request ID |
| `@vertz/ui-server` has hidden Node.js deps | SSR fails in V8 | Phase 1 POC. The package analysis shows no Node.js deps in core SSR path |
| Single-pass rendering latency in dev | Slower page loads if queries timeout | Phase 2 measurement. Per-query timeout defaults to 300ms, hard stream timeout 30s |
| Module resolution differences (deno_core vs Bun) | Import errors in V8 | Module loader already handles node_modules resolution. Test with real `@vertz/ui-server` in Phase 1 |

## Phase Overview

```
Phase 1 (POC + entry split) → Phase 2 (framework SSR) → Phase 3 (scoped fetch)
                                                        → Phase 4 (auth)
                                                           ↓
                                                        Phase 5 (cleanup)
```

Phase 3 and Phase 4 are independent of each other but both depend on Phase 2.
