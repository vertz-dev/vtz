# Phase 1: AsyncLocalStorage POC + SSR Entry Split

## Goal

Verify `ssrRenderSinglePass()` from `@vertz/ui-server` works inside deno_core's V8, and split entry detection so the persistent isolate loads `app.tsx` (SSR module) instead of `entry-client.ts` (browser-only hydration).

## Tasks

### Task 1: Add `ssr_entry` to ServerConfig

**Files** (2):
- `native/vtz/src/config.rs` (modified)
- `native/vtz/tests/config_tests.rs` (modified — or new if no config tests exist)

**Changes**:
- Add `ssr_entry: PathBuf` field to `ServerConfig`
- Add `detect_ssr_entry(src_dir: &Path) -> PathBuf` that looks for `app.tsx`, `app.ts`, `app.jsx`, `app.js` in order
- Existing `entry_file` stays — it's the client `<script>` tag target
- Wire `ssr_entry` into `ServerConfig::from_root()` or equivalent constructor

**Acceptance Criteria**:
```rust
#[test]
fn detect_ssr_entry_finds_app_tsx() {
    // Given: a src/ directory with app.tsx
    // When: detect_ssr_entry(src_dir) is called
    // Then: returns src/app.tsx
}

#[test]
fn detect_ssr_entry_falls_back_to_app_tsx() {
    // Given: a src/ directory with NO app.* files
    // When: detect_ssr_entry(src_dir) is called
    // Then: returns src/app.tsx (default)
}

#[test]
fn config_has_separate_ssr_and_client_entries() {
    // Given: a project with src/app.tsx and src/entry-client.ts
    // When: ServerConfig is created
    // Then: ssr_entry == src/app.tsx
    // And: entry_file == src/entry-client.ts
}
```

### Task 2: Load SSR entry in persistent isolate

**Files** (2):
- `native/vtz/src/runtime/persistent_isolate.rs` (modified)
- `native/vtz/src/server/http.rs` (modified)

**Changes**:
- `PersistentIsolateOptions`: rename `entry_file` to `ssr_entry` (or add `ssr_entry` alongside)
- `isolate_event_loop`: load `ssr_entry` (app.tsx) and store module namespace as `globalThis.__vertz_app_module`
- `http.rs`: pass `config.ssr_entry` to `PersistentIsolateOptions` instead of `config.entry_file`

**Acceptance Criteria**:
```rust
#[tokio::test]
async fn isolate_loads_app_module_as_ssr_entry() {
    // Given: test fixture with src/app.tsx exporting App function
    // When: persistent isolate initializes
    // Then: globalThis.__vertz_app_module is defined
    // And: globalThis.__vertz_app_module.App is a function
}
```

### Task 3: POC — ssrRenderSinglePass in deno_core V8

**Files** (3):
- `native/vtz/tests/fixtures/ssr-app/src/app.js` (modified — update to export SSRModule shape)
- `native/vtz/tests/ssr_render.rs` (modified — add POC test)
- `native/vtz/tests/fixtures/ssr-app/package.json` (modified — add @vertz/ui-server dep if needed)

**Changes**:
- Update `ssr-app` fixture to export `{ App, theme?, styles?, getInjectedCSS? }` matching SSRModule interface
- Write integration test that creates a V8 runtime, loads `@vertz/ui-server/ssr`, calls `ssrRenderSinglePass(module, "/")`, and asserts HTML output
- Verify `AsyncLocalStorage` is available: `import { AsyncLocalStorage } from 'node:async_hooks'`

**Acceptance Criteria**:
```rust
#[tokio::test]
async fn poc_ssr_render_single_pass_in_v8() {
    // Given: V8 runtime with @vertz/ui-server available
    //        app module exporting App() → <h1>Hello SSR</h1>
    // When: ssrRenderSinglePass(module, "/") is called
    // Then: result.html contains "<h1>Hello SSR</h1>"
}

#[tokio::test]
async fn poc_async_local_storage_available() {
    // Given: V8 runtime with node:async_hooks
    // When: AsyncLocalStorage is imported and .run() is called
    // Then: no error — ALS is functional
}
```

## Phase Acceptance Criteria

- `ServerConfig` has separate `ssr_entry` and `entry_file` fields
- Persistent isolate loads `app.tsx` as SSR module (not `entry-client.ts`)
- `ssrRenderSinglePass()` executes successfully inside deno_core V8
- `AsyncLocalStorage` is confirmed working (or a fallback plan is documented)
- All quality gates pass: `cargo test --all && cargo clippy --all-targets --release -- -D warnings && cargo fmt --all -- --check`

## Dependencies

None — this is the first phase.
