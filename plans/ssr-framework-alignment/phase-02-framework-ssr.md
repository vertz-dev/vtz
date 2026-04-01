# Phase 2: Framework SSR Rendering

## Goal

Replace `SSR_RENDER_JS` (innerHTML scraping) with `ssrRenderSinglePass()` so the persistent isolate returns real server-rendered HTML, CSS, and hydration data.

## Tasks

### Task 1: Replace SSR_RENDER_JS with framework rendering

**Files** (1):
- `native/vtz/src/runtime/persistent_isolate.rs` (modified)

**Changes**:
- Replace `SSR_RENDER_JS` constant with `SSR_RENDER_FRAMEWORK_JS` that calls `ssrRenderSinglePass(globalThis.__vertz_app_module, url)`
- Update `SSR_RESET_JS` to only reset what the framework needs (location, session) — no more `#app.innerHTML = ""`
- Parse the JSON result from the JS call into `SsrResponse`

**Acceptance Criteria**:
```rust
#[tokio::test]
async fn ssr_render_calls_framework_engine() {
    // Given: persistent isolate with app.tsx loaded as __vertz_app_module
    // When: SSR request for "/" is processed
    // Then: SSR_RENDER_FRAMEWORK_JS is executed (not old innerHTML scraping)
    // And: result is parsed from ssrRenderSinglePass output
}
```

### Task 2: Extend SsrResponse with hydration fields

**Files** (2):
- `native/vtz/src/runtime/persistent_isolate.rs` (modified)
- `native/vtz/src/server/http.rs` (modified)

**Changes**:
- Add `ssr_data: Option<String>`, `head_tags: Option<String>`, `redirect: Option<String>` to `SsrResponse`
- Update JSON deserialization in the SSR message handler to extract new fields
- Update `http.rs` SSR handler to pass new fields to HTML assembly

**Acceptance Criteria**:
```rust
#[tokio::test]
async fn ssr_response_includes_hydration_data() {
    // Given: app.tsx with a query() call
    // When: SSR request is processed
    // Then: response.ssr_data is Some(json_string)
}

#[tokio::test]
async fn ssr_response_includes_head_tags() {
    // Given: app.tsx with head tags (title, meta)
    // When: SSR request is processed
    // Then: response.head_tags is Some(html_string)
}
```

### Task 3: Inject SSR data and head tags into HTML document

**Files** (2):
- `native/vtz/src/ssr/html_document.rs` (modified)
- `native/vtz/tests/ssr_html_document.rs` (modified — or the test module inside html_document.rs)

**Changes**:
- Add `ssr_data: Option<&'a str>` and `head_tags: Option<&'a str>` to `SsrHtmlOptions`
- In `assemble_ssr_document()`: inject `<script>window.__VERTZ_SSR_DATA__={data}</script>` before closing `</body>`
- Inject head tags into `<head>` section

**Acceptance Criteria**:
```rust
#[test]
fn html_document_includes_ssr_data_script() {
    // Given: SsrHtmlOptions with ssr_data = Some(r#"[["tasks",[{"id":1}]]]"#)
    // When: assemble_ssr_document() is called
    // Then: output contains <script>window.__VERTZ_SSR_DATA__=[["tasks",[{"id":1}]]]</script>
}

#[test]
fn html_document_includes_head_tags() {
    // Given: SsrHtmlOptions with head_tags = Some("<link rel='preload' ...>")
    // When: assemble_ssr_document() is called
    // Then: output contains the preload link in <head>
}

#[test]
fn html_document_omits_ssr_data_when_none() {
    // Given: SsrHtmlOptions with ssr_data = None
    // When: assemble_ssr_document() is called
    // Then: output does NOT contain __VERTZ_SSR_DATA__
}
```

### Task 4: Remove per-request SSR fallback

**Files** (3):
- `native/vtz/src/ssr/render.rs` (modified — remove or gate render_to_html)
- `native/vtz/src/server/http.rs` (modified — remove legacy fallback path)
- `native/vtz/src/ssr/mod.rs` (modified — update module exports if needed)

**Changes**:
- Remove the per-request SSR fallback in `http.rs` (lines 616-668 that spawn fresh V8 per request)
- Either delete `render_to_html()` in `ssr/render.rs` or mark it `#[cfg(test)]`
- SSR always goes through the persistent isolate — if it's not initialized, serve client-only HTML shell

**Acceptance Criteria**:
```rust
#[tokio::test]
async fn ssr_only_uses_persistent_isolate() {
    // Given: persistent isolate is initialized
    // When: GET / with Accept: text/html
    // Then: SSR is handled by persistent isolate (not per-request V8)
}

#[tokio::test]
async fn ssr_falls_back_to_client_shell_when_isolate_not_ready() {
    // Given: persistent isolate is NOT initialized
    // When: GET / with Accept: text/html
    // Then: returns client-only HTML shell (not 404, not 503)
}
```

## Phase Acceptance Criteria

- `GET /` returns HTML with real component content inside `<div id="app">` (not empty)
- SSR log shows `ssr-persistent` (not `client-only`)
- HTML contains `__VERTZ_SSR_DATA__` script tag when queries are present
- Per-request SSR fallback is removed
- All quality gates pass

## Dependencies

Phase 1 must be complete (SSR entry split + POC verified).
