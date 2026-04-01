# Phase 2: E2E Infrastructure — Page API & Ops

**Design Doc:** `plans/native-webview-and-e2e.md`
**Issue:** #64

## Context

VTZ has a built-in test runner (`vtz test`) that executes test files in isolated V8 isolates with vitest-compatible globals (describe, it, expect). It also has a dev server (axum) and, after Phase 1, a native webview integration via `wry`.

This phase builds the **page API** — the bridge that lets test code running in V8 control the native webview. When a test calls `page.navigate("/login")` or `page.click("button")`, the call flows from the V8 test isolate → deno_core op → `EventLoopProxy` → main thread → `webview.evaluate_script()` → WebKit executes the DOM operation → result flows back.

The page API is exposed as a `vtz:e2e` module available in `.e2e.ts` test files. This phase implements only the **read-only** subset: navigation, querying elements, and reading content. Interaction (click, fill, type) is Phase 4.

The ops in this phase are the foundation that all e2e functionality builds on. The critical piece is the `evaluate_script` round-trip — every page API method ultimately calls `evaluate_script` on the webview and waits for the result.

## Prerequisites

- Phase 1 complete (webview module with `EventLoopProxy<UserEvent>`, `EvalScript` variant)

## Acceptance Criteria

```typescript
describe("Feature: E2E page API — navigation", () => {
  describe("Given a running dev server with a page at /", () => {
    describe("When calling page.navigate('/')", () => {
      it("Then the webview loads the page", async () => {
        await page.navigate("/");
        expect(await page.url()).toContain("/");
      });
    });
  });

  describe("Given a page with a known title", () => {
    describe("When calling page.evaluate(() => document.title)", () => {
      it("Then returns the page title", async () => {
        await page.navigate("/");
        const title = await page.evaluate(() => document.title);
        expect(title).toBe("Test App");
      });
    });
  });
});

describe("Feature: E2E page API — querying", () => {
  describe("Given a page with an <h1> element", () => {
    describe("When calling page.query('h1')", () => {
      it("Then returns a truthy element handle", async () => {
        await page.navigate("/");
        const el = await page.query("h1");
        expect(el).toBeTruthy();
      });
    });

    describe("When calling page.textContent('h1')", () => {
      it("Then returns the element's text content", async () => {
        await page.navigate("/");
        expect(await page.textContent("h1")).toBe("Hello World");
      });
    });
  });

  describe("Given a page without a .missing element", () => {
    describe("When calling page.query('.missing')", () => {
      it("Then returns null", async () => {
        await page.navigate("/");
        expect(await page.query(".missing")).toBeNull();
      });
    });
  });

  describe("Given a page with multiple <li> elements", () => {
    describe("When calling page.queryAll('li')", () => {
      it("Then returns all matching elements", async () => {
        await page.navigate("/list");
        const items = await page.queryAll("li");
        expect(items.length).toBe(3);
      });
    });
  });
});

describe("Feature: E2E page API — attributes and visibility", () => {
  describe("Given an element with a data attribute", () => {
    describe("When calling page.getAttribute(selector, name)", () => {
      it("Then returns the attribute value", async () => {
        await page.navigate("/");
        expect(await page.getAttribute("#app", "id")).toBe("app");
      });

      it("Then returns null for non-existing attributes", async () => {
        await page.navigate("/");
        expect(await page.getAttribute("#app", "data-missing")).toBeNull();
      });
    });
  });
});

describe("Feature: E2E page API — timeouts", () => {
  describe("Given an op that would hang", () => {
    describe("When the timeout expires", () => {
      it("Then the op rejects with a timeout error", async () => {
        await page.navigate("/");
        await expect(
          page.waitForSelector(".never-exists", { timeout: 100 })
        ).rejects.toThrow("timeout");
      });
    });
  });
});
```

- [ ] `op_e2e_navigate`, `op_e2e_url`, `op_e2e_query`, `op_e2e_query_all`, `op_e2e_text_content`, `op_e2e_inner_html`, `op_e2e_get_attribute`, `op_e2e_is_visible`, `op_e2e_evaluate` ops implemented
- [ ] All ops communicate with webview via `EventLoopProxy` → `EvalScript` round-trip
- [ ] All ops have configurable timeout (default 5000ms), reject with clear error on timeout
- [ ] `vtz:e2e` JS module provides the `page` object backed by these ops
- [ ] Ops are only registered when running in e2e mode (not present in unit test isolates)
- [ ] Element handles are tracked by ID (assigned in WebKit, referenced by number in V8)

## Tasks

### Task 1: Implement the eval bridge (core round-trip mechanism)

**Files:** (max 5)
- `native/vtz/src/webview/bridge.rs` (new)
- `native/vtz/src/webview/mod.rs` (modify)

Create the bridge that all ops use to execute JS in the webview and get results:

```rust
/// Sends JavaScript to the webview for evaluation and returns the result.
/// This is the core mechanism all e2e ops use.
pub struct WebviewBridge {
    proxy: EventLoopProxy<UserEvent>,
}

impl WebviewBridge {
    pub fn new(proxy: EventLoopProxy<UserEvent>) -> Self;

    /// Evaluate JS in the webview and return the JSON-serialized result.
    /// Times out after `timeout_ms` milliseconds.
    pub async fn eval(&self, js: &str, timeout_ms: u64) -> Result<String, BridgeError>;
}

pub enum BridgeError {
    Timeout { js_snippet: String, timeout_ms: u64 },
    EventLoopClosed,
    JsError(String),
}
```

The `eval` method:
1. Creates a `oneshot::channel()`
2. Sends `UserEvent::EvalScript { js, tx }` via the proxy
3. Wraps the receiver in `tokio::time::timeout(Duration::from_millis(timeout_ms))`
4. Returns the result or a timeout error

Make `WebviewBridge` cloneable and `Send + Sync` (the proxy is `Send`).

### Task 2: Implement core e2e ops (navigate, url, query, textContent, evaluate)

**Files:** (max 5)
- `native/vtz/src/runtime/ops/e2e.rs` (new)
- `native/vtz/src/runtime/ops/mod.rs` (modify)

Create deno_core ops that use `WebviewBridge`:

- `op_e2e_navigate(url: String, timeout_ms: u64)` — evaluates `window.location.href = url` then polls until `document.readyState === 'complete'`
- `op_e2e_url() -> String` — evaluates `window.location.href`
- `op_e2e_query(selector: String) -> Option<u32>` — evaluates `document.querySelector(selector)`, assigns an ID to the element (stored in a `window.__vtz_elements` Map), returns the ID or null
- `op_e2e_query_all(selector: String) -> Vec<u32>` — same but `querySelectorAll`, returns array of IDs
- `op_e2e_text_content(selector_or_id: String, timeout_ms: u64) -> Option<String>` — evaluates `el.textContent`
- `op_e2e_inner_html(selector: String, timeout_ms: u64) -> String`
- `op_e2e_get_attribute(selector: String, name: String, timeout_ms: u64) -> Option<String>`
- `op_e2e_is_visible(selector: String, timeout_ms: u64) -> bool`
- `op_e2e_evaluate(js: String, timeout_ms: u64) -> String` — evaluates arbitrary JS, returns JSON-serialized result

The `WebviewBridge` is stored in the deno_core `OpState` so ops can access it. It's injected when creating the e2e test runtime.

Element handle tracking: the initialization script (from Phase 1 ipc.rs) includes a `window.__vtz_elements = new Map()` and a counter. `op_e2e_query` assigns IDs so later ops can reference specific elements.

### Task 3: Create vtz:e2e JS module (page object)

**Files:** (max 5)
- `native/vtz/src/runtime/ops/e2e_bootstrap.js` (new)
- `native/vtz/src/runtime/js_runtime.rs` (modify)

Create the JavaScript module that wraps the ops into a developer-friendly `page` API:

```javascript
// e2e_bootstrap.js — stitched into the runtime when e2e mode is active
const page = {
  async navigate(url, opts = {}) {
    await Deno.core.ops.op_e2e_navigate(url, opts.timeout ?? 5000);
  },
  async url() {
    return Deno.core.ops.op_e2e_url();
  },
  async query(selector) {
    const id = await Deno.core.ops.op_e2e_query(selector);
    return id != null ? new ElementHandle(id) : null;
  },
  async queryAll(selector) {
    const ids = await Deno.core.ops.op_e2e_query_all(selector);
    return ids.map(id => new ElementHandle(id));
  },
  async textContent(selectorOrHandle, opts = {}) {
    const key = typeof selectorOrHandle === 'string'
      ? selectorOrHandle
      : `__id:${selectorOrHandle._id}`;
    return Deno.core.ops.op_e2e_text_content(key, opts.timeout ?? 5000);
  },
  async innerHTML(selector, opts = {}) {
    return Deno.core.ops.op_e2e_inner_html(selector, opts.timeout ?? 5000);
  },
  async getAttribute(selector, name, opts = {}) {
    return Deno.core.ops.op_e2e_get_attribute(selector, name, opts.timeout ?? 5000);
  },
  async isVisible(selector, opts = {}) {
    return Deno.core.ops.op_e2e_is_visible(selector, opts.timeout ?? 5000);
  },
  async evaluate(fn, opts = {}) {
    const js = `(${fn.toString()})()`;
    const result = await Deno.core.ops.op_e2e_evaluate(js, opts.timeout ?? 5000);
    return JSON.parse(result);
  },
  async waitForSelector(selector, opts = {}) {
    // Polling implementation — retry query until found or timeout
    const timeout = opts.timeout ?? 5000;
    const interval = 100;
    const start = Date.now();
    while (Date.now() - start < timeout) {
      const id = await Deno.core.ops.op_e2e_query(selector);
      if (id != null) return new ElementHandle(id);
      await new Promise(r => setTimeout(r, interval));
    }
    throw new Error(`timeout: waitForSelector("${selector}") exceeded ${timeout}ms`);
  },
};

class ElementHandle {
  constructor(id) { this._id = id; }
  async textContent() { return page.textContent(this); }
  async getAttribute(name) { return page.getAttribute(`__id:${this._id}`, name); }
  async isVisible() { return page.isVisible(`__id:${this._id}`); }
}

globalThis.__vtz_e2e_page = page;
```

In `js_runtime.rs`, when constructing extensions for e2e mode, include `e2e_bootstrap.js` in the bootstrap sources. Add `op_e2e_*` to the extension ops list. The `page` global is exposed via `globalThis.__vtz_e2e_page` and then injected as a `page` variable in the test globals (same pattern as `describe`, `it`, `expect`).

### Task 4: Unit tests for bridge and ops

**Files:** (max 5)
- `native/vtz/src/webview/bridge.rs` (modify — add tests module)
- `native/vtz/src/runtime/ops/e2e.rs` (modify — add tests module)

Write unit tests for:

**Bridge tests:**
- `eval` returns the result from the oneshot channel
- `eval` returns `BridgeError::Timeout` when the channel doesn't respond in time
- `eval` returns `BridgeError::EventLoopClosed` when the proxy is dropped

**Op tests (mocked bridge):**
- `op_e2e_navigate` sends the correct JS to the bridge
- `op_e2e_query` returns `None` when the bridge returns `"null"`
- `op_e2e_query` returns `Some(id)` when the bridge returns a valid ID
- `op_e2e_text_content` returns the text from the bridge response
- `op_e2e_evaluate` returns the parsed JSON result

For op tests, mock the `WebviewBridge` by creating a test-only impl that uses an mpsc channel instead of a real `EventLoopProxy`. This avoids needing a real webview in unit tests.

## Notes

- Element handle IDs are scoped to the page — navigating clears `window.__vtz_elements`. Tests should re-query after navigation.
- The `op_e2e_navigate` op needs to wait for page load, not just set `location.href`. Use a polling approach: set href, then poll `document.readyState === 'complete'` with the bridge.
- `op_e2e_evaluate` serializes the return value with `JSON.stringify`. Functions, undefined, and circular references will be lost. This matches Playwright's behavior.
- All ops accept `timeout_ms` to avoid hanging tests. The JS `page` API defaults to 5000ms, which matches Playwright's default.
