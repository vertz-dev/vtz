# Phase 0: POC — Thread Model & Hidden WebView

**Design Doc:** `plans/native-webview-and-e2e.md`
**Issue:** #64

## Context

VTZ wants to embed a native webview (via the `wry` crate) for two purposes: running the app in a desktop window (`vtz dev --desktop`) and running fast e2e tests without a browser (`vtz test --e2e`).

The fundamental technical risk is the **thread model conflict**: VTZ currently runs tokio on the main thread (for axum, V8, file watching, etc.), but `wry` requires the native event loop on the main thread (macOS AppKit enforces this). The solution is to flip thread ownership — run tokio on a background thread and the native event loop on main.

A second risk is whether **hidden webviews execute JavaScript**. The e2e test runner needs to run WebKit in a non-visible window. On macOS, AppKit/WebKit should process JS regardless of window visibility, but this must be validated.

This phase is a research spike. The output is a working POC and documented findings — not production code. The POC code may be thrown away or refactored in Phase 1.

## Prerequisites

- None

## Acceptance Criteria

- [ ] `wry` and `tao` added as optional dependencies behind a `desktop` cargo feature flag
- [ ] A POC example binary that:
  1. Spawns a tokio runtime on a background thread
  2. Starts an axum HTTP server on that background thread (serves a simple HTML page)
  3. Creates a hidden `wry` webview on the main thread pointing at the axum server
  4. Calls `evaluate_script_with_callback("document.title")` and prints the result
  5. Exits cleanly
- [ ] The hidden webview successfully executes JavaScript and returns the result
- [ ] Measured and documented: startup time, memory overhead of webview, JS execution latency for `evaluate_script_with_callback`
- [ ] POC Results section added to the design doc with findings

## Tasks

### Task 1: Add wry/tao as optional dependencies

**Files:** (max 5)
- `native/vtz/Cargo.toml` (modify)

Add `wry` and `tao` as optional dependencies behind a `desktop` feature flag:

```toml
[features]
default = []
desktop = ["wry", "tao"]

[dependencies]
wry = { version = "0.55", optional = true }
tao = { version = "0.32", optional = true }
```

Verify the project still compiles without the feature flag (`cargo build`) and with it (`cargo build --features desktop`).

### Task 2: Create minimal webview module

**Files:** (max 5)
- `native/vtz/src/webview/mod.rs` (new)
- `native/vtz/src/lib.rs` (modify)

Create a `webview` module (gated behind `#[cfg(feature = "desktop")]`) with:

- `UserEvent` enum: `ServerReady(u16)`, `EvalScript(String, oneshot::Sender<String>)`, `Quit`
- `WebviewApp` struct holding a `tao::event_loop::EventLoop<UserEvent>` and a `tao::event_loop::EventLoopProxy<UserEvent>`
- `WebviewApp::new(hidden: bool, width: u32, height: u32) -> Result<Self>`
  - Creates the event loop and a window (visible or hidden)
  - Creates a `wry::WebView` as a child of the window
- `WebviewApp::proxy(&self) -> EventLoopProxy<UserEvent>` — returns a cloneable, Send proxy
- `WebviewApp::run(self) -> !` — runs the event loop, dispatching `UserEvent` variants:
  - `ServerReady(port)` → `webview.load_url(format!("http://localhost:{port}"))`
  - `EvalScript(js, tx)` → `webview.evaluate_script_with_callback(js, move |result| tx.send(result))`
  - `Quit` → exit the event loop

Register the module in `lib.rs` with `#[cfg(feature = "desktop")] pub mod webview;`.

### Task 3: Create POC example binary

**Files:** (max 5)
- `native/vtz/examples/webview_poc.rs` (new)

Create an example that validates the full thread model:

1. Create `WebviewApp` on main thread (hidden mode)
2. Clone the `EventLoopProxy`
3. Spawn a std::thread that:
   a. Creates a tokio runtime
   b. Starts a minimal axum server on port 0 (OS-assigned) serving `<html><head><title>VTZ POC</title></head><body><h1>Hello</h1></body></html>`
   c. Sends `UserEvent::ServerReady(actual_port)` via the proxy
   d. Sleeps 1s, then sends `UserEvent::EvalScript("document.title", tx)`
   e. Receives the result from `rx`, prints it, asserts it equals `"VTZ POC"`
   f. Sends `UserEvent::Quit`
4. Main thread calls `webview_app.run()`

Run with: `cargo run --example webview_poc --features desktop`

Also measure and print:
- Time from process start to `ServerReady` event
- Time from `EvalScript` send to response received
- Process RSS memory (via `sysinfo` crate or `mach_task_basic_info` on macOS)

### Task 4: Document POC findings

**Files:** (max 5)
- `plans/native-webview-and-e2e.md` (modify)

Add a `## POC Results` section to the design doc with:
- Whether hidden webview executes JS: yes/no
- Startup time measurement
- `evaluate_script` round-trip latency
- Memory overhead
- Any issues discovered (e.g., macOS permission prompts, threading surprises)
- Go/no-go decision for proceeding to Phase 1

## Notes

- The `tao` event loop's `run()` method never returns (it calls `std::process::exit`). Use `run_return()` if available, or accept that the POC exits via the Quit event handler calling `std::process::exit(0)`.
- On macOS, the process may need an `Info.plist` or LSUIElement entry to avoid showing a dock icon. For the POC, a dock icon appearing briefly is acceptable.
- `wry` 0.55 requires `tao` 0.32 — check compatibility before pinning versions.
