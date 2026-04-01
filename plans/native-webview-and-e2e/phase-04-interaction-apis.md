# Phase 4: Interaction & Waiting APIs

**Design Doc:** `plans/native-webview-and-e2e.md`
**Issue:** #64

## Context

After Phase 3, VTZ can run e2e tests: `vtz test --e2e` discovers `.e2e.ts` files, starts a dev server and hidden webview, and runs tests with a `page` API that supports navigation, querying, and reading content.

This phase completes the page API with **interaction** primitives (click, fill, type, press, check, select) and **waiting** primitives (waitForSelector, waitForFunction, waitForNavigation). These are the methods that make e2e tests useful for real-world scenarios like form submission, navigation flows, and dynamic content.

All interaction ops follow the same pattern as Phase 2: the V8 test code calls a deno_core op → the op builds a JS snippet → sends it to the webview via `WebviewBridge::eval()` → WebKit executes it in the real DOM → result flows back. The key difference is that interaction ops need to dispatch proper DOM events (not just set properties), because application code listens for events like `input`, `change`, `click`, etc.

## Prerequisites

- Phase 3 complete (e2e test runner working end-to-end with read-only page API)

## Acceptance Criteria

```typescript
describe("Feature: click interaction", () => {
  describe("Given a page with a button", () => {
    describe("When calling page.click('button')", () => {
      it("Then dispatches mousedown, mouseup, and click events", async () => {
        await page.navigate("/click-test");
        await page.click("#btn");
        expect(await page.textContent("#result")).toBe("clicked");
      });
    });
  });

  describe("Given a page with a link", () => {
    describe("When calling page.click('a')", () => {
      it("Then navigates to the link target", async () => {
        await page.navigate("/nav-test");
        await page.click("a[href='/other']");
        expect(await page.url()).toContain("/other");
      });
    });
  });
});

describe("Feature: fill interaction", () => {
  describe("Given a text input", () => {
    describe("When calling page.fill('input', 'hello')", () => {
      it("Then sets the value and dispatches input + change events", async () => {
        await page.navigate("/form");
        await page.fill('input[name="name"]', "Alice");
        const value = await page.evaluate(
          () => (document.querySelector('input[name="name"]') as HTMLInputElement).value
        );
        expect(value).toBe("Alice");
      });
    });
  });

  describe("Given a textarea", () => {
    describe("When calling page.fill('textarea', 'content')", () => {
      it("Then sets the value and dispatches events", async () => {
        await page.navigate("/form");
        await page.fill("textarea", "Multi\nline");
        expect(await page.evaluate(
          () => (document.querySelector("textarea") as HTMLTextAreaElement).value
        )).toBe("Multi\nline");
      });
    });
  });
});

describe("Feature: type interaction", () => {
  describe("Given an input field", () => {
    describe("When calling page.type('input', 'abc')", () => {
      it("Then dispatches keydown/keypress/keyup per character", async () => {
        await page.navigate("/keylog");
        await page.type("#input", "abc");
        const log = await page.textContent("#keylog");
        expect(log).toContain("keydown:a");
        expect(log).toContain("keyup:c");
      });
    });
  });
});

describe("Feature: press interaction", () => {
  describe("Given a focused element", () => {
    describe("When calling page.press('Enter')", () => {
      it("Then dispatches Enter key events", async () => {
        await page.navigate("/form");
        await page.click('input[name="name"]');
        await page.press("Enter");
        expect(await page.textContent("#status")).toBe("submitted");
      });
    });
  });
});

describe("Feature: checkbox and select interactions", () => {
  describe("Given an unchecked checkbox", () => {
    describe("When calling page.check(selector)", () => {
      it("Then checks the checkbox", async () => {
        await page.navigate("/form");
        await page.check("#agree");
        expect(await page.isChecked("#agree")).toBe(true);
      });
    });
  });

  describe("Given a checked checkbox", () => {
    describe("When calling page.uncheck(selector)", () => {
      it("Then unchecks the checkbox", async () => {
        await page.navigate("/form");
        await page.check("#agree");
        await page.uncheck("#agree");
        expect(await page.isChecked("#agree")).toBe(false);
      });
    });
  });

  describe("Given a <select> element", () => {
    describe("When calling page.selectOption(selector, value)", () => {
      it("Then selects the option and dispatches change event", async () => {
        await page.navigate("/form");
        await page.selectOption("#color", "blue");
        expect(await page.evaluate(
          () => (document.querySelector("#color") as HTMLSelectElement).value
        )).toBe("blue");
      });
    });
  });
});

describe("Feature: waitForSelector", () => {
  describe("Given dynamic content that appears after a delay", () => {
    describe("When calling page.waitForSelector('.delayed')", () => {
      it("Then resolves when the element appears", async () => {
        await page.navigate("/async");
        const el = await page.waitForSelector(".delayed", { timeout: 3000 });
        expect(el).toBeTruthy();
      });
    });
  });

  describe("Given an element that never appears", () => {
    describe("When the timeout expires", () => {
      it("Then rejects with a timeout error", async () => {
        await page.navigate("/empty");
        await expect(
          page.waitForSelector(".never", { timeout: 200 })
        ).rejects.toThrow("timeout");
      });
    });
  });
});

describe("Feature: waitForFunction", () => {
  describe("Given a counter that increments on click", () => {
    describe("When waiting for a specific count", () => {
      it("Then resolves when the condition is true", async () => {
        await page.navigate("/counter");
        await page.click("#increment");
        await page.click("#increment");
        await page.waitForFunction(
          () => document.querySelector("#count")?.textContent === "2"
        );
        expect(await page.textContent("#count")).toBe("2");
      });
    });
  });
});

describe("Feature: waitForNavigation", () => {
  describe("Given a link click that triggers navigation", () => {
    describe("When calling page.waitForNavigation()", () => {
      it("Then resolves when the URL changes", async () => {
        await page.navigate("/nav-test");
        const navPromise = page.waitForNavigation("/other");
        await page.click("a[href='/other']");
        await navPromise;
        expect(await page.url()).toContain("/other");
      });
    });
  });
});
```

- [ ] `page.click()` dispatches mousedown → mouseup → click event sequence
- [ ] `page.fill()` clears the field, sets `.value`, dispatches `input` and `change` events
- [ ] `page.type()` dispatches keydown/keypress/keyup per character and updates `.value` incrementally
- [ ] `page.press()` handles special keys (Enter, Tab, Escape, ArrowUp, etc.)
- [ ] `page.check()` / `page.uncheck()` toggle checkboxes with proper events
- [ ] `page.selectOption()` selects `<option>` and dispatches `change`
- [ ] `page.isChecked()` returns checkbox checked state
- [ ] `page.waitForSelector()` polls at 100ms intervals with configurable timeout
- [ ] `page.waitForFunction()` polls arbitrary JS conditions
- [ ] `page.waitForNavigation()` resolves on URL change (optional URL pattern match)
- [ ] All interactions target the correct element (scroll into view, focus before typing)
- [ ] ElementHandle gets `click()` and `fill()` methods

## Tasks

### Task 1: Implement click, fill, type ops

**Files:** (max 5)
- `native/vtz/src/runtime/ops/e2e.rs` (modify)
- `native/vtz/src/runtime/ops/e2e_bootstrap.js` (modify)

**Ops:**

`op_e2e_click(selector: String, timeout_ms: u64)` — generates JS that:
1. Finds the element with `querySelector`
2. Scrolls it into view (`el.scrollIntoViewIfNeeded()`)
3. Dispatches: `new MouseEvent('mousedown', {bubbles:true})`, `new MouseEvent('mouseup', {bubbles:true})`, `new MouseEvent('click', {bubbles:true})`

`op_e2e_fill(selector: String, value: String, timeout_ms: u64)` — generates JS that:
1. Finds the element
2. Focuses it
3. Sets `el.value = ''` then `el.value = value`
4. Dispatches `new Event('input', {bubbles:true})` then `new Event('change', {bubbles:true})`
5. Works for `<input>`, `<textarea>`, and `contenteditable`

`op_e2e_type(selector: String, text: String, timeout_ms: u64)` — generates JS that:
1. Finds and focuses the element
2. For each character in `text`:
   a. Dispatches `keydown`, `keypress` events with the correct `key`/`code`
   b. Appends character to `el.value`
   c. Dispatches `input` event
   d. Dispatches `keyup` event

**JS bootstrap:** Add `page.click()`, `page.fill()`, `page.type()` wrappers, and `ElementHandle.click()`, `ElementHandle.fill()`.

### Task 2: Implement press, check, uncheck, selectOption ops

**Files:** (max 5)
- `native/vtz/src/runtime/ops/e2e.rs` (modify)
- `native/vtz/src/runtime/ops/e2e_bootstrap.js` (modify)

**Ops:**

`op_e2e_press(key: String, timeout_ms: u64)` — generates JS that dispatches key events on the focused element. Handle special key names: `Enter`, `Tab`, `Escape`, `Backspace`, `ArrowUp`, `ArrowDown`, `ArrowLeft`, `ArrowRight`, `Space`. Map key name to `key`/`code`/`keyCode` values.

`op_e2e_check(selector: String, timeout_ms: u64)` — if not already checked, clicks the checkbox.

`op_e2e_uncheck(selector: String, timeout_ms: u64)` — if already checked, clicks the checkbox.

`op_e2e_is_checked(selector: String, timeout_ms: u64) -> bool` — returns `el.checked`.

`op_e2e_select_option(selector: String, value: String, timeout_ms: u64)` — generates JS that:
1. Finds the `<select>` element
2. Sets `el.value = value`
3. Dispatches `new Event('change', {bubbles:true})`

**JS bootstrap:** Add `page.press()`, `page.check()`, `page.uncheck()`, `page.isChecked()`, `page.selectOption()` wrappers.

### Task 3: Implement waitForFunction and waitForNavigation

**Files:** (max 5)
- `native/vtz/src/runtime/ops/e2e_bootstrap.js` (modify)
- `native/vtz/src/runtime/ops/e2e.rs` (modify)

`waitForSelector` was already implemented in Phase 2's bootstrap JS (polling with setTimeout). Now add:

**`page.waitForFunction(fn, opts)`** — implemented in JS (no new op needed):
1. Serialize the function to a string
2. Poll via `evaluate` at 100ms intervals
3. Resolve when the function returns truthy
4. Reject on timeout

**`page.waitForNavigation(urlPattern?, opts?)`** — implemented in JS:
1. Record current URL
2. Poll `page.url()` at 100ms intervals
3. Resolve when URL changes (and matches pattern if provided)
4. Reject on timeout

**`op_e2e_wait_for_load(timeout_ms: u64)`** — new op that polls `document.readyState === 'complete'`. Used internally by `navigate` and optionally by `waitForNavigation`.

### Task 4: Integration tests with test fixture app

**Files:** (max 5)
- `native/vtz/tests/e2e_interactions_local.rs` (new)
- `native/vtz/tests/fixtures/e2e-test-app/index.html` (new)
- `native/vtz/tests/fixtures/e2e-test-app/src/app.tsx` (new)

Create a minimal Vertz test fixture app with pages that exercise all interactions:
- `/` — has an `<h1>`, links, a button that updates text on click
- `/form` — has inputs, textarea, checkbox, select, submit button
- `/async` — has content that appears after a setTimeout
- `/counter` — has a count display and increment button
- `/keylog` — has an input that logs key events to a div
- `/nav-test` — has links to other pages

Write integration tests (marked `#[ignore]`, run with `--features desktop` on macOS):
- Click button → text updates
- Fill form → submit → success message
- Check/uncheck checkbox → state changes
- Select option → value changes
- Wait for async content → element appears
- Navigate via link click → URL changes
- Type text → key events dispatched

These tests use the real webview, real dev server, and real Vertz app. They validate the full round-trip from test code → ops → bridge → webview → DOM → back.

### Task 5: Complete ElementHandle with interaction methods

**Files:** (max 5)
- `native/vtz/src/runtime/ops/e2e_bootstrap.js` (modify)

Extend the `ElementHandle` class with methods that delegate to the ops using the element's ID:

```javascript
class ElementHandle {
  constructor(id) { this._id = id; }

  async click(opts = {}) {
    return Deno.core.ops.op_e2e_click(`__id:${this._id}`, opts.timeout ?? 5000);
  }
  async fill(value, opts = {}) {
    return Deno.core.ops.op_e2e_fill(`__id:${this._id}`, value, opts.timeout ?? 5000);
  }
  async type(text, opts = {}) {
    return Deno.core.ops.op_e2e_type(`__id:${this._id}`, text, opts.timeout ?? 5000);
  }
  async textContent() { /* ... existing ... */ }
  async getAttribute(name) { /* ... existing ... */ }
  async isVisible() { /* ... existing ... */ }
  async isChecked() {
    return Deno.core.ops.op_e2e_is_checked(`__id:${this._id}`, 5000);
  }
}
```

The ops need to handle the `__id:` prefix: when the selector starts with `__id:`, look up the element from `window.__vtz_elements` instead of calling `querySelector`. This was partially designed in Phase 2 — complete and test it here.

## Notes

- **Event dispatch order matters.** React and other frameworks attach synthetic event listeners. The events must bubble (`bubbles: true`) and be composed (`composed: true`) to cross Shadow DOM boundaries if needed.
- **`page.fill()` clears first.** Unlike `page.type()` which appends, `fill()` replaces the entire value. This matches Playwright behavior.
- **`page.type()` is slow for long strings** because it dispatches events per character. Use `page.fill()` for setting values; use `page.type()` only when the app needs to react to individual keystrokes.
- **Special key mapping** for `page.press()`: maintain a lookup table of key names → `{ key, code, keyCode }` values. Reference: https://developer.mozilla.org/en-US/docs/Web/API/KeyboardEvent/code/code_values
- **Scroll into view** before click/fill/type — WebKit may not dispatch events to off-screen elements in some cases. Using `scrollIntoViewIfNeeded()` is a WebKit-specific API that's available since we only target WebKit.
