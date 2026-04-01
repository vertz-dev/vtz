# Phase 4: Auth/Session + Redirects

## Goal

SSR receives session data from cookies and passes it to `ssrRenderSinglePass()`. `ProtectedRoute` redirects work in dev — unauthenticated requests get HTTP 302.

## Tasks

### Task 1: Pass session to ssrRenderSinglePass

**Files** (2):
- `native/vtz/src/runtime/persistent_isolate.rs` (modified)
- `native/vtz/src/ssr/session.rs` (modified — if session format needs changes)

**Changes**:
- In the SSR JS dispatch, pass `ssrAuth` option from `globalThis.__vertz_session` to `ssrRenderSinglePass()`
- Set `globalThis.__vertz_session` from `SsrRequest.session_json` before each render
- Ensure session is available to scoped fetch interceptor (API calls during SSR carry auth)

**Acceptance Criteria**:
```rust
#[tokio::test]
async fn ssr_passes_session_to_render() {
    // Given: SSR request with session_json = Some(r#"{"userId":"123"}"#)
    // When: persistent isolate processes the SSR request
    // Then: ssrRenderSinglePass receives { ssrAuth: { userId: "123" } }
    // And: app can read session data during rendering
}

#[tokio::test]
async fn ssr_session_available_to_scoped_fetch() {
    // Given: SSR request with session cookie
    //        app.tsx with query('/api/me') that requires auth
    //        server.ts handler that reads session from request
    // When: SSR render triggers /api/me fetch
    // Then: scoped fetch interceptor forwards session to API handler
}
```

### Task 2: Handle redirect responses

**Files** (2):
- `native/vtz/src/server/http.rs` (modified)
- `native/vtz/tests/ssr_auth.rs` (new)

**Changes**:
- In the SSR response handler: if `ssr_response.redirect` is `Some(url)`, return HTTP 302 with `Location` header instead of HTML
- Add integration test for redirect behavior

**Acceptance Criteria**:
```rust
#[tokio::test]
async fn ssr_redirect_on_auth_failure() {
    // Given: no session cookie
    //        app.tsx with ProtectedRoute wrapping /dashboard
    // When: GET /dashboard with Accept: text/html
    // Then: HTTP 302, Location: /login
}

#[tokio::test]
async fn ssr_no_redirect_with_valid_session() {
    // Given: valid session cookie
    //        app.tsx with ProtectedRoute wrapping /dashboard
    // When: GET /dashboard with Accept: text/html
    // Then: HTTP 200 with rendered dashboard HTML
}
```

## Phase Acceptance Criteria

- Authenticated SSR renders protected content
- Unauthenticated SSR returns 302 redirect to login
- Session data flows from cookie → Rust → V8 → ssrRenderSinglePass → scoped fetch
- All quality gates pass

## Dependencies

Phase 2 must be complete. Phase 3 (scoped fetch) should be complete for the API call auth forwarding, but basic redirect handling can be tested without it.
