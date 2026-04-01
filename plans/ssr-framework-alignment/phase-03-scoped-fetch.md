# Phase 3: Scoped Fetch Integration

## Goal

`/api/*` calls during SSR are intercepted and handled in-memory via `server.ts` handler using `runWithScopedFetch()`. No network round-trip for API data during SSR.

## Tasks

### Task 1: Install fetch proxy at isolate init

**Files** (1):
- `native/vtz/src/runtime/persistent_isolate.rs` (modified)

**Changes**:
- Add `INSTALL_FETCH_SCOPE_JS` constant that imports and calls `installFetchProxy()` from `@vertz/ui-server/fetch-scope`
- Execute it during isolate initialization, after DOM shim and before module loading
- Save original `globalThis.fetch` as `globalThis.__vertz_original_fetch` for passthrough

**Acceptance Criteria**:
```rust
#[tokio::test]
async fn isolate_installs_fetch_proxy_at_init() {
    // Given: persistent isolate with @vertz/ui-server available
    // When: isolate finishes initialization
    // Then: globalThis.__vertz_original_fetch is defined
    // And: fetch proxy is installed (globalThis.fetch !== __vertz_original_fetch)
}
```

### Task 2: Wrap SSR rendering in runWithScopedFetch

**Files** (1):
- `native/vtz/src/runtime/persistent_isolate.rs` (modified)

**Changes**:
- Replace `SSR_RENDER_FRAMEWORK_JS` with `SSR_WITH_SCOPED_FETCH_JS` that wraps `ssrRenderSinglePass()` in `runWithScopedFetch(interceptor, ...)`
- Interceptor routes `/api/*` to `globalThis.__vertz_api_handler` (the server.ts handler)
- External URLs pass through to `globalThis.__vertz_original_fetch`

**Acceptance Criteria**:
```rust
#[tokio::test]
async fn ssr_intercepts_api_calls_in_memory() {
    // Given: app.tsx with query('/api/tasks')
    //        server.ts handler returning [{ id: 1, title: "Test" }]
    // When: SSR request for "/" is processed
    // Then: rendered HTML contains "Test" (data was pre-fetched)
    // And: no external HTTP request was made to /api/tasks
}
```

### Task 3: External fetch passthrough

**Files** (2):
- `native/vtz/src/runtime/persistent_isolate.rs` (modified — if interceptor logic needs refinement)
- `native/vtz/tests/ssr_scoped_fetch.rs` (new)

**Changes**:
- Verify that non-`/api/*` URLs are NOT intercepted (pass through to real fetch)
- Verify that absolute URLs (e.g., `https://api.example.com`) are NOT intercepted
- Add integration test file for scoped fetch behavior

**Acceptance Criteria**:
```rust
#[tokio::test]
async fn ssr_external_fetch_passes_through() {
    // Given: app.tsx that fetches from https://api.example.com
    // When: SSR request is processed
    // Then: external fetch is NOT intercepted
    // And: reaches real network (or times out gracefully)
}

#[tokio::test]
async fn ssr_non_api_local_fetch_passes_through() {
    // Given: app.tsx that fetches from /some-other-path
    // When: SSR request is processed
    // Then: fetch is NOT intercepted by API handler
}
```

## Phase Acceptance Criteria

- SSR with `query('/api/tasks')` returns pre-rendered HTML containing task data
- API calls during SSR are handled in-memory (no network hop)
- External URLs and non-API local URLs pass through to real fetch
- All quality gates pass

## Dependencies

Phase 2 must be complete (framework SSR rendering working).
