# Phase 1: Desktop Mode Foundation

**Design Doc:** `plans/native-webview-and-e2e.md`
**Issue:** #64

## Context

VTZ is a Rust-based dev server that compiles and serves Vertz apps via axum on localhost, with HMR over WebSocket. Currently, `vtz dev` prints a URL and the developer opens it in a browser.

This phase adds `vtz dev --desktop`, which opens the app in a native macOS window using WebKit (via the `wry` crate) instead of a browser tab. The app is still served by the same axum dev server — the webview simply points at localhost. HMR continues to work because the webview loads the same JS client that connects to the `/__vertz_hmr` WebSocket.

The thread model is: native event loop on the main thread (required by macOS AppKit), tokio runtime on a background thread (runs axum, V8, file watcher, etc.). Communication between them uses `tao::event_loop::EventLoopProxy<UserEvent>`, which is `Send`.

When `--desktop` is NOT passed, VTZ behaves exactly as before — tokio on main, no webview.

Phase 0 (POC) must be complete and validated before starting this phase.

## Prerequisites

- Phase 0 complete with go-ahead (hidden webview executes JS, thread model works)

## Acceptance Criteria

- [ ] `vtz dev --desktop` opens a native macOS window showing the Vertz app
- [ ] HMR works in the webview — editing a source file triggers a live update without manual reload
- [ ] Window title shows the project name (from `package.json` name field or directory name)
- [ ] Closing the window exits the VTZ process cleanly (server shuts down, no zombie processes)
- [ ] `--width <px>` and `--height <px>` flags control initial window size (default: 1024x768)
- [ ] DevTools are accessible in debug builds (right-click → Inspect or keyboard shortcut)
- [ ] Without `--desktop`, behavior is completely unchanged (opens browser as before)
- [ ] `cargo build` without `desktop` feature compiles without webview dependencies
- [ ] `cargo clippy --all-targets --release --features desktop -- -D warnings` passes

## Tasks

### Task 1: Promote webview module from POC to production

**Files:** (max 5)
- `native/vtz/src/webview/mod.rs` (modify)
- `native/vtz/src/webview/events.rs` (new)
- `native/vtz/src/webview/ipc.rs` (new)

Refactor the POC `webview` module into production-quality code:

**events.rs:**
- Define `UserEvent` enum: `ServerReady { port: u16 }`, `Navigate(String)`, `EvalScript { js: String, tx: oneshot::Sender<String> }`, `Quit`
- Implement `std::fmt::Debug` for logging

**ipc.rs:**
- Define the initialization script injected into the webview via `with_initialization_script()`
- Sets up `window.__vtz.postMessage(msg)` which calls `window.ipc.postMessage(JSON.stringify(msg))`
- Sets up `window.__vtz.on(event, handler)` for Rust→JS messages
- Define the `with_ipc_handler` callback that parses incoming JSON messages

**mod.rs:**
- `WebviewApp::new(opts: WebviewOptions) -> Result<Self>` — takes title, width, height, hidden, devtools
- `WebviewApp::proxy(&self) -> EventLoopProxy<UserEvent>`
- `WebviewApp::run(self) -> !` — runs the event loop
- On `ServerReady` → load URL, set window title
- On `EvalScript` → evaluate and respond
- On window close → clean shutdown (send signal to tokio runtime, then exit)
- Error handling with `thiserror`

### Task 2: Add --desktop CLI flag and wire thread model

**Files:** (max 5)
- `native/vtz/src/cli.rs` (modify)
- `native/vtz/src/main.rs` (modify)

**cli.rs:**
- Add `--desktop` flag to the `dev` subcommand (gated behind `#[cfg(feature = "desktop")]`)
- Add `--width` and `--height` optional args (u32, defaults 1024/768)

**main.rs:**
- When `--desktop` is set:
  1. Create `WebviewApp` on main thread
  2. Clone the `EventLoopProxy`
  3. Spawn `std::thread::spawn` that creates tokio runtime and runs the dev server
  4. The background thread sends `UserEvent::ServerReady(port)` when axum is listening
  5. Main thread calls `webview_app.run()`
- When `--desktop` is NOT set:
  - Existing behavior unchanged — `#[tokio::main]` on main thread

The key is that the dev server startup logic (in `server/http.rs`) needs to be callable as an async function from either context. It should already be structured this way — verify and adjust if needed.

### Task 3: Wire clean shutdown on window close

**Files:** (max 5)
- `native/vtz/src/webview/mod.rs` (modify)
- `native/vtz/src/server/http.rs` (modify)

When the user closes the webview window:
1. The `tao` event loop receives `WindowEvent::CloseRequested`
2. Send a shutdown signal to the tokio runtime (via a `tokio::sync::watch` channel or `CancellationToken`)
3. The axum server gracefully shuts down (using axum's `with_graceful_shutdown`)
4. The process exits with code 0

The server already may have graceful shutdown logic — check and extend if needed. The important thing is no zombie tokio tasks or leaked file watchers.

### Task 4: Integration test for desktop mode

**Files:** (max 5)
- `native/vtz/tests/desktop_local.rs` (new)

Create a test (marked `#[ignore]` for CI, run manually on macOS with a display):
1. Start `vtz dev --desktop` as a child process with a test fixture project
2. Wait for the process to print the "listening on" message
3. Send a signal to quit (or use a timeout)
4. Verify the process exits cleanly (exit code 0)

This is a smoke test, not a full functional test. Full functional testing happens in Phase 3 with the e2e runner.

Also add a unit test (NOT ignored) that verifies `--desktop` flag parsing in the CLI.

## Notes

- The `#[tokio::main]` macro on `main()` won't work for desktop mode because main must run the native event loop. Use conditional compilation or restructure main to call tokio manually.
- `tao::event_loop::EventLoop::new()` must be called on the main thread. On macOS, calling it from a spawned thread will panic.
- Window title can be set at creation time and updated later via `window.set_title()`.
- For debug builds, enable devtools with `WebViewBuilder::with_devtools(true)`.
