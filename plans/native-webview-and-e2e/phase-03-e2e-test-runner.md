# Phase 3: E2E Test Runner Integration

**Design Doc:** `plans/native-webview-and-e2e.md`
**Issue:** #64

## Context

VTZ has a built-in test runner (`vtz test`) that discovers test files by glob, executes each in an isolated V8 isolate, and reports results via terminal/JSON/JUnit reporters. After Phase 2, VTZ also has a page API — deno_core ops that let V8 test code control a native webview.

This phase wires everything together into `vtz test --e2e`: a single command that automatically starts the dev server, creates a hidden webview, discovers `.e2e.ts` files, runs them with the page API available, and reports results through the existing reporter infrastructure.

The e2e runner reuses as much of the existing test runner as possible. The differences from unit test mode are:
1. **Shared infrastructure** — a single dev server and webview are started before all tests and stopped after
2. **Sequential execution** — tests run one file at a time (single webview, no parallelism in Phase 1)
3. **State reset** — between test files, the webview clears cookies/localStorage and navigates to `about:blank`
4. **Additional ops** — the e2e ops (from Phase 2) are registered in the V8 isolate, and the `page` global is injected alongside `describe`/`it`/`expect`

Test files use `.e2e.ts` or `.e2e.tsx` extensions and live alongside other tests (typically in `src/` or `tests/`).

## Prerequisites

- Phase 2 complete (page API ops, `vtz:e2e` module, `WebviewBridge`)

## Acceptance Criteria

```typescript
describe("Feature: E2E test discovery", () => {
  describe("Given a project with .e2e.ts files in src/", () => {
    describe("When running vtz test --e2e", () => {
      it("Then discovers all .e2e.ts and .e2e.tsx files", () => {});
      it("Then ignores regular .test.ts files", () => {});
      it("Then respects --filter to match test names", () => {});
    });
  });
});

describe("Feature: E2E infrastructure lifecycle", () => {
  describe("Given vtz test --e2e is invoked", () => {
    describe("When the runner starts", () => {
      it("Then starts the dev server automatically if not already running", () => {});
      it("Then creates a hidden webview pointing at the dev server", () => {});
      it("Then the page global is available in test files", () => {});
    });

    describe("When all tests complete", () => {
      it("Then the webview is closed", () => {});
      it("Then the dev server is stopped", () => {});
      it("Then the process exits with code 0 for all-pass", () => {});
      it("Then the process exits with code 1 for any failure", () => {});
    });
  });
});

describe("Feature: E2E test execution", () => {
  describe("Given multiple .e2e.ts files", () => {
    describe("When running them", () => {
      it("Then runs files sequentially (not in parallel)", () => {});
      it("Then resets webview state between files (cookies, localStorage)", () => {});
      it("Then reports results in the same format as unit tests", () => {});
    });
  });

  describe("Given an e2e test that fails", () => {
    describe("When an assertion fails", () => {
      it("Then reports file path, line number, and assertion message", () => {});
      it("Then continues running remaining test files (unless --bail)", () => {});
    });
  });
});

describe("Feature: E2E CLI flags", () => {
  describe("Given --headed flag", () => {
    it("Then the webview window is visible during tests", () => {});
  });

  describe("Given --devtools flag", () => {
    it("Then devtools are opened (implies --headed)", () => {});
  });

  describe("Given --bail flag", () => {
    it("Then stops on first test file with a failure", () => {});
  });
});
```

- [ ] `vtz test --e2e` discovers `.e2e.ts` / `.e2e.tsx` files
- [ ] Dev server starts automatically for e2e mode
- [ ] Hidden webview created and pointed at dev server
- [ ] `page` global injected into e2e test isolates
- [ ] Test files run sequentially, webview state reset between files
- [ ] Results flow through existing reporters (terminal, JSON, JUnit)
- [ ] `--headed` makes webview visible
- [ ] `--devtools` opens inspector (implies `--headed`)
- [ ] `--bail` stops on first failure
- [ ] `--filter` works for test names
- [ ] Process exits 0 on all-pass, 1 on any failure
- [ ] Clean shutdown: webview closed, server stopped, no zombie processes

## Tasks

### Task 1: Add e2e discovery to test collector

**Files:** (max 5)
- `native/vtz/src/test/collector.rs` (modify)

The existing test collector discovers files matching `**/*.test.{ts,tsx}` (or similar). Extend it to support an e2e mode:

- Add an `E2eCollectionMode` that uses glob `**/*.e2e.{ts,tsx}`
- The `collect_test_files()` function (or equivalent) takes a mode parameter
- When in e2e mode, only `.e2e.ts` / `.e2e.tsx` files are collected
- `--filter` still applies to test names within discovered files
- Include/exclude glob patterns from `TestRunConfig` still apply

Write tests:
- Discovers `.e2e.ts` files when in e2e mode
- Ignores `.test.ts` files when in e2e mode
- Ignores `.e2e.ts` files when in normal mode
- Respects include/exclude patterns

### Task 2: Add --e2e, --headed, --devtools CLI flags

**Files:** (max 5)
- `native/vtz/src/cli.rs` (modify)
- `native/vtz/src/test/runner.rs` (modify)

**cli.rs:**
- Add `--e2e` flag to the `test` subcommand (gated behind `#[cfg(feature = "desktop")]`)
- Add `--headed` flag (only valid with `--e2e`)
- Add `--devtools` flag (only valid with `--e2e`, implies `--headed`)
- Validation: `--headed` and `--devtools` without `--e2e` is an error

**runner.rs:**
- Extend `TestRunConfig` with `e2e: bool`, `headed: bool`, `devtools: bool`
- Pass these through to the test execution logic

Write tests for flag parsing and validation.

### Task 3: Implement E2E test orchestrator

**Files:** (max 5)
- `native/vtz/src/test/e2e_runner.rs` (new)
- `native/vtz/src/test/runner.rs` (modify)
- `native/vtz/src/test/mod.rs` (modify)

The E2E orchestrator manages the lifecycle:

```rust
pub struct E2eTestOrchestrator {
    config: TestRunConfig,
    server_port: u16,
    bridge: WebviewBridge,
}

impl E2eTestOrchestrator {
    /// Start dev server + webview, return orchestrator ready for tests
    pub async fn start(config: TestRunConfig, proxy: EventLoopProxy<UserEvent>) -> Result<Self>;

    /// Run all e2e test files sequentially
    pub async fn run_all(&self, files: Vec<PathBuf>) -> Vec<TestFileResult>;

    /// Reset webview state between test files
    async fn reset_webview(&self) -> Result<()>;

    /// Stop server and signal webview to close
    pub async fn stop(self) -> Result<()>;
}
```

**Lifecycle:**
1. `start()` — boots the dev server (reuses existing server startup from `server/http.rs`), waits for it to bind, creates `WebviewBridge` from the proxy
2. `run_all()` — for each file: reset webview → create V8 isolate with e2e ops + page global → load and execute test file → collect results
3. `reset_webview()` — evaluates `localStorage.clear(); sessionStorage.clear();` then navigates to `about:blank` then back to app root
4. `stop()` — shuts down server, sends `UserEvent::Quit`

In `runner.rs`, the main `run_tests()` function checks `config.e2e` and delegates to `E2eTestOrchestrator` instead of the parallel unit test executor.

### Task 4: Inject page global into e2e test isolates

**Files:** (max 5)
- `native/vtz/src/test/executor.rs` (modify)
- `native/vtz/src/test/globals.rs` (modify)

When creating V8 isolates for e2e test files:
1. Include the e2e ops extension (from Phase 2) in the runtime
2. Put the `WebviewBridge` into `OpState` so ops can access it
3. After loading the standard test globals (describe, it, expect), also inject: `const page = globalThis.__vtz_e2e_page;`
4. The `page` object is now available as a top-level variable in test code

The existing `execute_test_file_with_options()` function in `executor.rs` needs a conditional path: if `e2e_bridge` is `Some(bridge)`, register the e2e extension and inject the page global. Otherwise, standard unit test mode.

Write tests:
- e2e isolate has `page` global
- unit test isolate does NOT have `page` global
- `page.navigate` calls the bridge (mock bridge)

### Task 5: Thread model integration for e2e tests

**Files:** (max 5)
- `native/vtz/src/main.rs` (modify)

Wire the `vtz test --e2e` command to the thread-flipped model (same as `vtz dev --desktop`):

1. When `--e2e` flag is set:
   a. Create `WebviewApp` on main thread (hidden unless `--headed`)
   b. Clone `EventLoopProxy`
   c. Spawn background thread with tokio runtime
   d. In background: run `E2eTestOrchestrator` → collect results → send `UserEvent::Quit` when done
   e. Capture the exit code and pass it via an `Arc<AtomicI32>` or channel
   f. Main thread runs `webview_app.run()`
   g. On `Quit` event, exit with the captured exit code

2. Without `--e2e`, test command runs as today (no webview, no thread flip)

The tricky part is getting the exit code from the background thread to the main thread before the process exits. Use a shared `Arc<Mutex<Option<i32>>>` that the background thread writes to before sending `Quit`.

## Notes

- E2e tests are intentionally sequential in this phase. Parallel e2e (multiple webview windows) is future work.
- The dev server started for e2e tests uses port 0 (OS-assigned) to avoid conflicts with a dev server the user might already be running.
- `--bail` stops after the first file with a failure, not the first individual test. This matches the unit test runner behavior.
- Reporter output format is identical for e2e and unit tests. The only difference is a `[e2e]` tag in the file path display so users can distinguish them.
- If the user already has `vtz dev` running, the e2e runner should detect the running server and reuse it (port detection). This is a nice-to-have — for Phase 3, always starting a fresh server is acceptable.
