# Design: Native Webview Runtime & E2E Test Runner

**Issue:** [#64](https://github.com/vertz-dev/vtz/issues/64)

## Goal

Add two capabilities to VTZ:

1. **Desktop app mode** — Embed a native webview (via `wry` crate) so `vtz dev --desktop` opens the app in a native window instead of a browser tab. This is the foundation for Vertz desktop apps.
2. **Built-in E2E test runner** — Use the same webview infrastructure to run fast, lightweight end-to-end tests (`vtz test --e2e`) without requiring Playwright, Cypress, or a full browser install.

Both features share the same core: a Rust-controlled native webview that loads the app from VTZ's existing axum dev server.

## Manifesto Alignment

- **Performance is not optional** — Native webview eliminates the ~50-80MB Node.js sidecar that Electron/Tauri apps carry. E2E tests via native webview start in milliseconds vs seconds for browser-based runners.
- **No ceilings** — If Playwright is too slow and heavy for e2e tests, build a faster one. If Electron wastes memory, eliminate the waste.
- **One way to do things** — `vtz test` already runs unit tests. Adding `--e2e` keeps everything in one tool. No separate test runner config, no separate browser install.
- **AI agents are first-class users** — A single `vtz test --e2e` command is trivially discoverable by LLMs. No multi-tool orchestration.
- **If you can't test it, don't build it** — The e2e runner makes full-app testing as cheap as unit testing, removing the excuse to skip it.

## Non-Goals

- Production packaging/distribution (DMG, MSI, code signing) — separate future work
- Native OS APIs (menus, file dialogs, system tray, notifications) — separate future work
- Multi-window support — one window per `vtz dev --desktop` invocation
- Cross-platform CI for e2e tests in this phase — macOS first, Linux (Xvfb) later
- Visual regression / screenshot diffing — may be added later but not in scope
- Replacing Playwright for projects that need cross-browser testing — this tests WebKit only

## Unknowns

1. **Main thread ownership** — `wry` requires the native event loop on the main thread. VTZ currently runs tokio on the main thread. Resolution: POC in Phase 1 validates the thread flip (tokio on background thread, event loop on main).
2. **Offscreen/hidden webview for tests** — wry doesn't have an official headless mode. Resolution: POC tests whether a hidden window (`set_visible(false)`) still executes JS and renders DOM. On macOS, WebKit processes JS even when the window is hidden.
3. **evaluate_script latency** — E2E test speed depends on how fast `evaluate_script_with_callback` round-trips. Resolution: benchmark in POC.

---

## POC Results (Phase 0)

**Date:** 2026-04-01
**Crate versions:** `wry 0.49.0`, `tao 0.32.8`
**Platform:** macOS (Darwin 25.3.0)

### Findings

| Metric | Debug Build | Release Build |
|--------|------------|---------------|
| `evaluate_script_with_callback` round-trip | 41.4ms | **1.9ms** |
| RSS memory (full process) | 86.9 MB | **83.3 MB** |
| WebviewApp creation time | 188ms | 247ms |
| Axum server bind time | 0.3ms | 0.1ms |
| Total startup to first eval | ~1.7s | ~1.75s |

### Validated

1. **Thread model works.** Tokio runtime on background thread, tao event loop on main thread. Communication via `EventLoopProxy<UserEvent>` is reliable.
2. **Hidden webview executes JavaScript.** A window created with `with_visible(false)` still loads URLs and executes `evaluate_script_with_callback` correctly on macOS. WebKit processes everything regardless of window visibility.
3. **Round-trip latency is excellent.** 1.9ms in release mode means e2e test interactions will add negligible overhead per command.
4. **Memory is reasonable.** 83 MB for the entire process (V8 + axum + WebKit) in release mode. The webview itself adds roughly 30-40 MB on top of a no-webview baseline.

### Issues

- The `evaluate_script_with_callback` closure must be `Fn` (not `FnOnce`). Solved with `Mutex<Option<oneshot::Sender>>` pattern to allow single consumption inside a multi-call closure.
- `wry 0.55` is not yet published; latest available is `0.49.0`. API surface is equivalent for our use case.
- WebviewApp creation (~200ms) is a one-time cost at startup, acceptable.

### Decision

**Go ahead.** All three risks from the Unknowns section are resolved favorably. Proceeding to Phase 1.

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────┐
│                      main thread                         │
│                                                          │
│  ┌────────────────┐     ┌─────────────────────────┐     │
│  │  tao EventLoop │────▶│  wry WebView            │     │
│  │                │     │  - loads localhost:PORT  │     │
│  │  receives:     │     │  - IPC via postMessage   │     │
│  │  - UserEvent   │     │  - evaluate_script       │     │
│  └────────┬───────┘     └─────────────────────────┘     │
│           │                                              │
│           │ EventLoopProxy (Send)                        │
└───────────┼──────────────────────────────────────────────┘
            │
            ▼
┌───────────────────────────────────────────────────────────┐
│                    tokio runtime (background)              │
│                                                            │
│  ┌──────────┐  ┌──────────────┐  ┌──────────────────┐    │
│  │ axum dev │  │ Compilation  │  │ HMR WebSocket    │    │
│  │ server   │  │ Pipeline     │  │ broadcast        │    │
│  └──────────┘  └──────────────┘  └──────────────────┘    │
│                                                            │
│  ┌──────────────────┐  ┌──────────────────────────────┐   │
│  │ V8 Persistent    │  │ E2E Test Orchestrator        │   │
│  │ Isolate (API)    │  │ (sends commands via proxy)   │   │
│  └──────────────────┘  └──────────────────────────────┘   │
└────────────────────────────────────────────────────────────┘
```

### Thread Model

The key architectural change: **flip the thread ownership**.

**Current:**
```
main thread → tokio::main → axum server, V8, etc.
```

**With webview:**
```
main thread → tao event loop + wry webview
background  → tokio runtime → axum server, V8, etc.
```

This is necessary because both macOS (AppKit) and wry require the event loop on the main thread. The tokio runtime moves to a background thread spawned before the event loop starts. Communication between the two uses `tao::event_loop::EventLoopProxy<UserEvent>`, which is `Send`.

When running without `--desktop` (normal browser mode), the thread flip doesn't happen — tokio stays on main as today. The webview code path is only activated by the `--desktop` flag or `--e2e` test mode.

---

## Part 1: Desktop App Mode (`vtz dev --desktop`)

### User Experience

```bash
# Opens native window instead of browser tab
vtz dev --desktop

# With specific window size
vtz dev --desktop --width 1200 --height 800

# Normal browser mode (unchanged)
vtz dev
```

The window shows the same app, served by the same dev server, with the same HMR. The only difference is the rendering target.

### Implementation

#### New crate: `vtz-webview` (internal)

A thin wrapper around `wry` + `tao` that provides:

```rust
pub struct WebviewApp {
    event_loop: EventLoop<UserEvent>,
    proxy: EventLoopProxy<UserEvent>,
}

pub enum UserEvent {
    /// Navigate the webview to a URL
    Navigate(String),
    /// Execute JavaScript in the webview
    EvalScript(String, Option<oneshot::Sender<String>>),
    /// Close the window and exit
    Quit,
    /// Server is ready — load the URL
    ServerReady(u16), // port
}

impl WebviewApp {
    /// Create the event loop and webview. Must be called on main thread.
    pub fn new(opts: WebviewOptions) -> Result<Self>;

    /// Get a Send-able proxy for cross-thread communication.
    pub fn proxy(&self) -> EventLoopProxy<UserEvent>;

    /// Run the event loop (blocks forever). Call on main thread.
    pub fn run(self) -> !;
}
```

This is NOT a plugin — it's a runtime feature of the `vtz` crate, gated behind a cargo feature flag (`desktop`).

#### IPC Bridge

The webview gets an initialization script that sets up a bidirectional bridge:

```javascript
// Injected via with_initialization_script()
window.__vtz = {
  postMessage(msg) {
    window.ipc.postMessage(JSON.stringify(msg));
  },
  _handlers: new Map(),
  on(event, handler) {
    this._handlers.set(event, handler);
  },
};
```

Rust side receives messages via `with_ipc_handler` and can respond via `evaluate_script`. This enables future native OS API calls (menus, dialogs, etc.) without changing the transport.

#### Feature Flag

```toml
# native/vtz/Cargo.toml
[features]
default = []
desktop = ["wry", "tao"]

[dependencies]
wry = { version = "0.55", optional = true }
tao = { version = "0.32", optional = true }
```

Desktop mode is opt-in at compile time. The default `vtz` binary doesn't include webview dependencies. A `vtz-desktop` binary (or feature-flagged build) includes them.

---

## Part 2: E2E Test Runner (`vtz test --e2e`)

### User Experience

```bash
# Run all e2e tests
vtz test --e2e

# Run specific e2e test file
vtz test --e2e src/tests/login.e2e.ts

# Run with visible window (for debugging)
vtz test --e2e --headed

# Run with devtools open
vtz test --e2e --headed --devtools
```

E2E test files use a `.e2e.ts` or `.e2e.tsx` extension and are discovered by the existing test collector with a different glob pattern.

### Test API

Tests use a vitest-compatible structure with a `page` API inspired by Playwright but simplified:

```typescript
import { describe, it, expect } from "vtz:test";
import { page } from "vtz:e2e";

describe("Login flow", () => {
  it("shows the login form", async () => {
    await page.navigate("/login");
    const form = await page.query("form.login");
    expect(form).toBeTruthy();
  });

  it("logs in with valid credentials", async () => {
    await page.navigate("/login");
    await page.fill('input[name="email"]', "user@example.com");
    await page.fill('input[name="password"]', "password123");
    await page.click('button[type="submit"]');
    await page.waitForNavigation("/dashboard");
    const heading = await page.textContent("h1");
    expect(heading).toBe("Welcome back");
  });

  it("shows error for invalid credentials", async () => {
    await page.navigate("/login");
    await page.fill('input[name="email"]', "wrong@example.com");
    await page.fill('input[name="password"]', "wrong");
    await page.click('button[type="submit"]');
    const error = await page.waitForSelector(".error-message");
    expect(await page.textContent(error)).toContain("Invalid credentials");
  });
});
```

### Page API Surface

```typescript
// vtz:e2e module — available in .e2e.ts files
interface Page {
  // Navigation
  navigate(url: string): Promise<void>;
  reload(): Promise<void>;
  waitForNavigation(urlPattern?: string): Promise<void>;
  url(): Promise<string>;

  // Querying
  query(selector: string): Promise<ElementHandle | null>;
  queryAll(selector: string): Promise<ElementHandle[]>;
  waitForSelector(selector: string, opts?: { timeout?: number }): Promise<ElementHandle>;

  // Interaction
  click(selector: string): Promise<void>;
  fill(selector: string, value: string): Promise<void>;
  type(selector: string, text: string): Promise<void>;
  press(key: string): Promise<void>;
  check(selector: string): Promise<void>;
  uncheck(selector: string): Promise<void>;
  selectOption(selector: string, value: string): Promise<void>;

  // Content
  textContent(selectorOrHandle: string | ElementHandle): Promise<string>;
  innerHTML(selector: string): Promise<string>;
  getAttribute(selector: string, name: string): Promise<string | null>;
  isVisible(selector: string): Promise<boolean>;
  isChecked(selector: string): Promise<boolean>;

  // Evaluation
  evaluate<T>(fn: () => T): Promise<T>;
  evaluateHandle(fn: () => unknown): Promise<ElementHandle>;

  // Waiting
  waitForTimeout(ms: number): Promise<void>;
  waitForFunction(fn: () => boolean, opts?: { timeout?: number }): Promise<void>;

  // Screenshots (future)
  // screenshot(opts?: { path?: string }): Promise<Buffer>;
}

interface ElementHandle {
  click(): Promise<void>;
  fill(value: string): Promise<void>;
  textContent(): Promise<string>;
  getAttribute(name: string): Promise<string | null>;
  isVisible(): Promise<boolean>;
}
```

### E2E Test Architecture

```
┌─────────────────────────────────────────────────────┐
│                    main thread                       │
│                                                      │
│  tao EventLoop + wry WebView (hidden by default)    │
│  - Receives EvalScript commands from test runner    │
│  - Executes JS in real WebKit rendering engine      │
│  - Returns results via EventLoopProxy              │
└──────────────┬──────────────────────────────────────┘
               │ EventLoopProxy<UserEvent>
               │
┌──────────────▼──────────────────────────────────────┐
│                  tokio runtime                        │
│                                                       │
│  ┌──────────────┐    ┌─────────────────────────┐     │
│  │ axum dev     │    │ E2E Test Orchestrator    │     │
│  │ server       │    │                          │     │
│  │ (compiles &  │    │ For each .e2e.ts file:  │     │
│  │  serves app) │    │ 1. Create V8 isolate    │     │
│  │              │    │ 2. Load test globals     │     │
│  │              │    │ 3. Load page API ops     │     │
│  │              │    │ 4. Run test file         │     │
│  │              │    │ 5. Collect results       │     │
│  └──────────────┘    └─────────────────────────┘     │
└──────────────────────────────────────────────────────┘
```

**How `page.click(selector)` works end-to-end:**

1. Test JS calls `page.click("button.submit")` in V8 test isolate
2. This invokes a deno_core op: `op_e2e_click`
3. The op sends `UserEvent::EvalScript(js)` via the `EventLoopProxy`
   - The JS payload: `document.querySelector("button.submit").click()`
4. The main thread's event loop receives the event
5. `webview.evaluate_script_with_callback(js, callback)` executes in WebKit
6. The callback sends the result back via a `oneshot::channel`
7. The op resolves the result back to the test V8 isolate
8. `page.click()` promise resolves

This round-trip happens entirely in-process — no CDP, no browser process, no network protocol overhead.

### Why This Is Faster Than Playwright

| Aspect | Playwright | VTZ E2E |
|--------|-----------|---------|
| Browser process | Full Chromium (~150MB) | Native WebKit (shared with OS, ~0MB extra) |
| Startup time | 1-3s (browser launch) | ~50ms (window creation) |
| Communication | WebSocket → CDP JSON protocol | In-process function call via EventLoopProxy |
| Test runner | Node.js process | V8 isolate (already running) |
| Install size | ~250MB browser download | 0 (uses OS WebKit) |
| Per-test overhead | New browser context | `navigate()` + DOM clear |

### E2E Test Isolation

Each test file gets:
- A fresh V8 isolate (same as unit tests — no state pollution)
- A `page.navigate("/")` at the start (webview navigates to app root)
- Tests within a file run sequentially (they share the webview)

Between test **files**, the webview is reset:
- `webview.clear_all_browsing_data()` — clears cookies, localStorage, cache
- Navigate to `about:blank` then back to the app

Test files themselves can run in parallel if multiple webview windows are supported in the future. For Phase 1, e2e tests run sequentially (one webview).

### Headless vs Headed

- **Default (`vtz test --e2e`):** Hidden window (`set_visible(false)`). WebKit still renders and executes JS — the window just isn't shown. This works on macOS because AppKit/WebKit processes everything regardless of visibility.
- **Headed (`--headed`):** Visible window. Useful for debugging. Slower because rendering is visible.
- **CI:** On macOS, hidden window works. On Linux, requires Xvfb (future work).

---

## Implementation Plan

### Phase 0: POC — Thread Model & Hidden WebView (research spike)

**Goal:** Validate that the thread flip works and hidden webviews execute JS.

**Tasks:**
1. Add `wry` and `tao` as optional dependencies behind `desktop` feature flag
2. Create a minimal binary that:
   - Spawns tokio on a background thread
   - Starts axum on the background thread (serves a "Hello World" HTML page)
   - Creates a hidden `wry` webview on the main thread, pointing at localhost
   - Calls `evaluate_script("document.title")` and prints the result
3. Measure: startup time, memory usage, JS execution in hidden mode

**Acceptance Criteria:**
- Hidden webview executes JS and returns results via `evaluate_script_with_callback`
- Axum server runs on background thread without issues
- Memory overhead of webview is <20MB above baseline

**POC Result:** attach findings before proceeding to Phase 1.

---

### Phase 1: Desktop Mode Foundation

**Goal:** `vtz dev --desktop` opens the app in a native window with HMR.

**Acceptance Criteria:**

```rust
// Integration test (manual/local — requires display)
#[test]
#[cfg(feature = "desktop")]
fn desktop_mode_opens_window() {
    // 1. Start dev server on background thread
    // 2. Create webview pointing at server
    // 3. Verify webview loaded the page (evaluate_script returns app title)
    // 4. Modify a source file
    // 5. Verify HMR updated the page (evaluate_script returns new content)
}
```

- [ ] `vtz dev --desktop` opens a native macOS window rendering the Vertz app
- [ ] HMR works in the webview (file change → live update, no manual reload)
- [ ] Window title shows project name
- [ ] Closing the window exits the process
- [ ] `--width` and `--height` flags control initial window size
- [ ] DevTools accessible via right-click → Inspect (debug builds)
- [ ] Without `--desktop`, behavior is unchanged (opens browser)

**Implementation:**
1. Create `src/webview/` module in the vtz crate
2. Implement `WebviewApp` struct wrapping `tao` + `wry`
3. Modify `main.rs`: when `--desktop`, run event loop on main, tokio on background
4. Wire `ServerReady` event to load the URL in the webview
5. Wire window close → process exit

---

### Phase 2: E2E Infrastructure — Page API & Ops

**Goal:** Build the page API that e2e tests will use, implemented as deno_core ops.

**Acceptance Criteria:**

```typescript
describe("Feature: E2E page API", () => {
  describe("Given a running dev server with a simple page", () => {
    describe("When navigating to the page", () => {
      it("Then page.url() returns the navigated URL", async () => {
        await page.navigate("/");
        expect(await page.url()).toBe("http://localhost:PORT/");
      });
    });

    describe("When querying an element", () => {
      it("Then page.query() returns an element handle for existing elements", async () => {
        await page.navigate("/");
        const el = await page.query("h1");
        expect(el).toBeTruthy();
      });

      it("Then page.query() returns null for non-existing elements", async () => {
        await page.navigate("/");
        const el = await page.query(".does-not-exist");
        expect(el).toBeNull();
      });
    });

    describe("When reading text content", () => {
      it("Then page.textContent() returns the element's text", async () => {
        await page.navigate("/");
        expect(await page.textContent("h1")).toBe("Hello World");
      });
    });
  });
});
```

- [ ] `op_e2e_navigate`, `op_e2e_query`, `op_e2e_click`, `op_e2e_fill`, `op_e2e_text_content` ops implemented
- [ ] Ops communicate with webview via `EventLoopProxy` round-trip
- [ ] All ops have configurable timeout (default 5s)
- [ ] `vtz:e2e` module provides the `page` object backed by these ops
- [ ] Ops only registered when running in e2e mode (not polluting unit test isolates)

---

### Phase 3: E2E Test Runner Integration

**Goal:** `vtz test --e2e` discovers and runs `.e2e.ts` files.

**Acceptance Criteria:**

```typescript
describe("Feature: E2E test runner", () => {
  describe("Given a project with .e2e.ts files", () => {
    describe("When running vtz test --e2e", () => {
      it("Then discovers all .e2e.ts files in src/", async () => {});
      it("Then starts the dev server automatically", async () => {});
      it("Then creates a hidden webview", async () => {});
      it("Then runs each test file sequentially", async () => {});
      it("Then reports results in the same format as unit tests", async () => {});
    });
  });

  describe("Given an e2e test that fails", () => {
    describe("When the test assertion fails", () => {
      it("Then reports the failure with file, line, and assertion message", async () => {});
      it("Then continues running remaining tests (no --bail)", async () => {});
    });
  });

  describe("Given --headed flag", () => {
    describe("When running e2e tests", () => {
      it("Then the webview window is visible", async () => {});
    });
  });
});
```

- [ ] Test collector discovers `.e2e.ts` / `.e2e.tsx` files
- [ ] Dev server auto-starts for e2e tests (reuses existing server if running)
- [ ] Hidden webview created for test execution
- [ ] Tests run sequentially (one file at a time, one webview)
- [ ] Results reported via existing terminal/JSON/JUnit reporters
- [ ] `--headed` flag shows the webview during tests
- [ ] `--devtools` flag opens devtools (implies `--headed`)
- [ ] `--bail` stops on first failure
- [ ] `--filter` works for e2e test names
- [ ] Webview state reset between test files (cookies, localStorage cleared)

---

### Phase 4: Interaction & Waiting APIs

**Goal:** Complete the page API with interaction and waiting primitives.

**Acceptance Criteria:**

```typescript
describe("Feature: Page interactions", () => {
  describe("Given a form page", () => {
    describe("When filling and submitting a form", () => {
      it("Then page.fill() sets input values", async () => {
        await page.navigate("/form");
        await page.fill('input[name="email"]', "test@example.com");
        const value = await page.evaluate(
          () => (document.querySelector('input[name="email"]') as HTMLInputElement).value
        );
        expect(value).toBe("test@example.com");
      });

      it("Then page.click() triggers click events", async () => {
        await page.navigate("/form");
        await page.click("button[type='submit']");
        await page.waitForSelector(".success-message");
      });
    });

    describe("When waiting for dynamic content", () => {
      it("Then page.waitForSelector() resolves when element appears", async () => {
        await page.navigate("/async-page");
        const el = await page.waitForSelector(".loaded", { timeout: 3000 });
        expect(el).toBeTruthy();
      });

      it("Then page.waitForSelector() rejects on timeout", async () => {
        await page.navigate("/empty-page");
        await expect(
          page.waitForSelector(".never-exists", { timeout: 100 })
        ).rejects.toThrow("timeout");
      });

      it("Then page.waitForFunction() polls until truthy", async () => {
        await page.navigate("/counter");
        await page.click("#increment");
        await page.click("#increment");
        await page.click("#increment");
        await page.waitForFunction(
          () => document.querySelector("#count")?.textContent === "3"
        );
      });
    });
  });
});
```

- [ ] `page.fill()` dispatches proper input/change events (not just setting `.value`)
- [ ] `page.click()` dispatches mousedown/mouseup/click sequence
- [ ] `page.type()` dispatches keydown/keypress/keyup per character
- [ ] `page.press()` handles special keys (Enter, Tab, Escape, etc.)
- [ ] `page.check()` / `page.uncheck()` for checkboxes
- [ ] `page.selectOption()` for `<select>` elements
- [ ] `page.waitForSelector()` polls with configurable interval and timeout
- [ ] `page.waitForFunction()` polls arbitrary JS conditions
- [ ] `page.waitForNavigation()` resolves on URL change
- [ ] `page.evaluate()` runs arbitrary JS and returns serialized result

---

## Type Flow Map

```
CLI args (--desktop / --e2e)
  → main.rs: decides thread model
    → WebviewApp::new(opts) → EventLoop<UserEvent> + WebView
      → EventLoopProxy<UserEvent> (Send) → passed to tokio runtime
        → E2E ops use proxy to send EvalScript commands
          → WebView.evaluate_script_with_callback()
            → oneshot::Sender<String> → resolves op future in V8
              → page.click() Promise resolves in test JS
```

No dead generics. `UserEvent` is the only generic (on `EventLoop<T>`) and it flows from creation through proxy to webview dispatch.

## E2E Acceptance Test

```typescript
// The full developer walkthrough — this test must pass for the feature to be done
import { describe, it, expect } from "vtz:test";
import { page } from "vtz:e2e";

describe("E2E: Todo App", () => {
  it("creates, completes, and deletes a todo", async () => {
    await page.navigate("/");

    // Create a todo
    await page.fill('input[placeholder="What needs to be done?"]', "Buy milk");
    await page.press("Enter");
    expect(await page.textContent(".todo-list li:first-child")).toBe("Buy milk");

    // Complete it
    await page.click(".todo-list li:first-child .toggle");
    expect(await page.isChecked(".todo-list li:first-child .toggle")).toBe(true);

    // Delete it
    await page.click(".todo-list li:first-child .destroy");
    expect(await page.query(".todo-list li")).toBeNull();
  });
});
```

Run: `vtz test --e2e src/tests/todo.e2e.ts`

---

## Dependencies

```toml
# New optional dependencies
wry = { version = "0.55", optional = true }
tao = { version = "0.32", optional = true }
```

No new required dependencies. The `desktop` feature flag controls inclusion.

## Risks

1. **macOS-only initially** — WebKit is native on macOS. Linux needs WebKitGTK (extra deps). Windows needs WebView2. Phase 1 targets macOS only.
2. **Hidden webview JS execution** — if macOS throttles JS in hidden windows, e2e tests could be slow. POC Phase 0 validates this.
3. **Single-threaded webview** — all webview ops are sequenced through the main thread event loop. This is fine for e2e tests (sequential anyway) but means `evaluate_script` calls are not concurrent.
4. **WebKit-only testing** — e2e tests only validate WebKit rendering. Apps that need cross-browser testing still need Playwright. This is a feature (fast WebKit tests) not a limitation — it complements Playwright, doesn't replace it.
