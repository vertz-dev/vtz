# Phase 5: Cleanup + Test App + Documentation

## Goal

Remove dead code from the old SSR system, create a canonical test app in the repo, and document the app structure so agents scaffold apps consistently.

## Tasks

### Task 1: Remove old SSR rendering code

**Files** (3):
- `native/vtz/src/runtime/persistent_isolate.rs` (modified — remove old SSR_RENDER_JS, SSR_RESET_JS constants)
- `native/vtz/src/ssr/render.rs` (modified — remove or delete render_to_html if unused)
- `native/vtz/src/ssr/mod.rs` (modified — update exports)

**Changes**:
- Delete `SSR_RENDER_JS` constant (replaced by `SSR_RENDER_FRAMEWORK_JS`)
- Delete `SSR_RESET_JS` if no longer needed (or simplify to only reset location/session)
- Remove `render_to_html()` and its per-request V8 creation logic if fully replaced
- Remove any dead imports/types left behind

**Acceptance Criteria**:
```rust
#[test]
fn no_references_to_old_ssr_render_js() {
    // Given: the codebase
    // When: searching for "SSR_RENDER_JS" (the old constant name)
    // Then: no active code references it (only comments/history)
}
```
- `cargo test --all` passes (no broken references)
- `cargo clippy --all-targets --release -- -D warnings` passes (no dead code warnings)

### Task 2: Create canonical test app fixture

**Files** (4):
- `native/vtz/tests/fixtures/ssr-app/src/app.tsx` (new or modified)
- `native/vtz/tests/fixtures/ssr-app/src/entry-client.ts` (new)
- `native/vtz/tests/fixtures/ssr-app/src/server.ts` (new)
- `native/vtz/tests/fixtures/ssr-app/package.json` (modified)

**Changes**:
- `app.tsx`: Export `App`, `theme`, `styles`, `getInjectedCSS` matching SSRModule interface
- `entry-client.ts`: Import App from `./app`, call `hydrate(App, { target: '#app' })`
- `server.ts`: Minimal API server with `GET /api/tasks` returning test data
- `package.json`: Add `@vertz/ui`, `@vertz/ui-server` as dependencies

**Acceptance Criteria**:
```rust
#[tokio::test]
async fn test_app_serves_ssr_html() {
    // Given: ssr-app fixture with canonical structure
    // When: vtz dev server starts and GET / is requested
    // Then: HTTP 200 with server-rendered HTML containing app content
}

#[tokio::test]
async fn test_app_api_routes_work() {
    // Given: ssr-app fixture with server.ts
    // When: GET /api/tasks is requested
    // Then: returns JSON with task data
}
```

### Task 3: Document canonical app structure

**Files** (1):
- `CLAUDE.md` (modified — add canonical app structure section)

**Changes**:
- Add "Canonical App Structure" section documenting:
  - `src/app.tsx` — SSR module, what it must export
  - `src/entry-client.ts` — client hydration, calls `hydrate()`
  - `src/server.ts` — API server, `createServer()` pattern
- This ensures all agents scaffold apps the same way

**Acceptance Criteria**:
- CLAUDE.md contains a "Canonical App Structure" section
- Section lists all three files with their purpose and required exports
- An agent reading CLAUDE.md can scaffold a correct Vertz app without ambiguity

## Phase Acceptance Criteria

- No dead SSR code remains (old innerHTML scraping path is gone)
- `ssr-app` fixture follows canonical structure and all SSR tests pass against it
- CLAUDE.md documents the canonical app structure
- All quality gates pass

## Dependencies

Phases 2, 3, and 4 must be complete.
