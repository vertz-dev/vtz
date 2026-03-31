# Phase 1: Core Proxy + Auto-Registration

- **Author:** Claude Opus 4.6 (implementation agent)
- **Reviewer:** Claude Opus 4.6 (review agent)
- **Commits:** abdb22a
- **Date:** 2026-03-31

## Changes

- `native/vtz/src/proxy/mod.rs` (new) — module declarations
- `native/vtz/src/proxy/naming.rs` (new) — subdomain sanitization + naming
- `native/vtz/src/proxy/routes.rs` (new) — file-based route registration, stale PID cleanup
- `native/vtz/src/proxy/host.rs` (new) — Host header subdomain extraction
- `native/vtz/src/proxy/daemon.rs` (new) — HTTP reverse proxy server + dashboard
- `native/vtz/src/proxy/client.rs` (new) — auto-registration client (used by dev server)
- `native/vtz/src/cli.rs` (modified) — added Proxy subcommand, --name flag on DevArgs
- `native/vtz/src/config.rs` (modified) — added proxy_name field to ServerConfig
- `native/vtz/src/main.rs` (modified) — wired proxy init/start/stop/status commands
- `native/vtz/src/server/http.rs` (modified) — integrated auto-registration + deregistration
- `native/vtz/src/lib.rs` (modified) — added proxy module
- `plans/reverse-proxy.md` (new) — design doc

## CI Status

- [ ] Quality gates passed at `abdb22a` (pending verification)

## Review Checklist

- [x] Delivers what the ticket asks for
- [x] TDD compliance (tests before/alongside implementation)
- [ ] No type gaps or missing edge cases (see findings)
- [ ] No security issues (see findings)
- [x] Public API changes match design doc

## Findings

### BLOCKER-1: `to_bytes(body, usize::MAX)` — unbounded request body read (DoS vector)

**File:** `native/vtz/src/proxy/daemon.rs`, line 111

```rust
let body_bytes = match axum::body::to_bytes(body, usize::MAX).await {
```

The proxy reads the entire request body into memory with no size limit. A malicious or buggy client can send an arbitrarily large body and OOM the proxy daemon, taking down routing for all dev servers.

**Fix:** Set a reasonable limit (e.g., 100MB for dev use):
```rust
const MAX_BODY_SIZE: usize = 100 * 1024 * 1024; // 100 MB
let body_bytes = match axum::body::to_bytes(body, MAX_BODY_SIZE).await {
```

---

### BLOCKER-2: XSS in dashboard HTML — `entry.branch` is not HTML-escaped

**File:** `native/vtz/src/proxy/daemon.rs`, lines 186-193

```rust
rows.push_str(&format!(
    "<tr><td><a href=\"http://{sub}.localhost\">{sub}</a></td>\
     <td>{port}</td><td>{branch}</td><td>{pid}</td></tr>",
    sub = entry.subdomain,
    port = entry.port,
    branch = entry.branch,
    pid = entry.pid,
));
```

The `branch` field (and `subdomain` to a lesser extent) is interpolated directly into HTML without escaping. While subdomains go through `sanitize_label`, the `branch` field in `RouteEntry` stores the raw git branch name (e.g., `feat/auth`). A branch name containing `<script>alert(1)</script>` would execute in the user's browser when viewing the dashboard.

Yes, this is a dev-only tool, but the route files are world-writable JSON on disk. Any local process can write a malicious route file.

**Fix:** HTML-escape all interpolated values before inserting them into the template. Write a simple `fn html_escape(s: &str) -> String` that replaces `<`, `>`, `&`, `"`, `'`.

---

### BLOCKER-3: `sanitize_label` truncation can produce trailing dash

**File:** `native/vtz/src/proxy/naming.rs`, lines 42-47

```rust
// Truncate to DNS label limit
if trimmed.len() <= MAX_LABEL_LEN {
    trimmed.to_string()
} else {
    trimmed[..MAX_LABEL_LEN].to_string()
}
```

If the input is 64+ chars and character 63 is a dash (e.g., `"aaa...aaa-bbb"` where the dash falls at position 63), the truncated result will end with `-`, producing an invalid DNS label.

**Fix:** After truncation, trim trailing dashes again:
```rust
trimmed[..MAX_LABEL_LEN].trim_end_matches('-').to_string()
```

Add a test:
```rust
#[test]
fn sanitize_truncation_does_not_leave_trailing_dash() {
    // 62 a's + dash + more chars → truncates at 63, dash is at position 62
    let input = format!("{}-bbb", "a".repeat(62));
    let result = sanitize_label(&input);
    assert!(!result.ends_with('-'));
}
```

---

### SHOULD-FIX-1: Double filesystem reads in `register_dev_server`

**File:** `native/vtz/src/proxy/client.rs`, lines 81-94

```rust
let subdomain = if let Some(name) = name_override {
    naming::sanitize_label(name)
} else {
    let branch = detect_git_branch(root_dir).unwrap_or_else(|| "main".to_string());
    let project = detect_project_name(root_dir);
    naming::to_subdomain(&branch, &project)
};

// ...

let branch = detect_git_branch(root_dir).unwrap_or_else(|| "unknown".to_string());
let project = detect_project_name(root_dir);
```

`detect_git_branch` and `detect_project_name` are called twice — once to compute the subdomain, and again to populate the `RouteEntry`. This reads `.git/HEAD` and `package.json` from disk twice. More importantly, the fallback values differ between calls: the first uses `"main"` as default, the second uses `"unknown"`. This means if git detection fails, you get subdomain `"my-app"` (as if on main) but the `RouteEntry.branch` says `"unknown"` — an inconsistency.

**Fix:** Compute branch/project once, reuse for both subdomain computation and entry construction:
```rust
let branch = detect_git_branch(root_dir).unwrap_or_else(|| "main".to_string());
let project = detect_project_name(root_dir);

let subdomain = if let Some(name) = name_override {
    naming::sanitize_label(name)
} else {
    naming::to_subdomain(&branch, &project)
};
```

---

### SHOULD-FIX-2: `reload_routes()` called on every single request

**File:** `native/vtz/src/proxy/daemon.rs`, lines 69-70

```rust
// Reload routes for fresh data (cheap for small route counts)
state.reload_routes().await;
```

Every proxied HTTP request reads the entire `~/.vtz/proxy/routes/` directory from disk, parses all JSON files, and replaces the in-memory HashMap. While the comment says "cheap for small route counts," this is disk I/O on every request including static assets, HMR WebSocket pings, etc. Under load (e.g., a page with 50 asset requests), this means 50 directory reads in rapid succession.

**Fix:** Use a time-based cache (e.g., reload at most once per 2 seconds), or use file-system watching (notify crate, already a dependency), or at minimum reload only on cache miss (unknown subdomain) rather than every request.

---

### SHOULD-FIX-3: WebSocket upgrade (HMR) is not proxied

**File:** `native/vtz/src/proxy/daemon.rs`

The design doc's Phase 1 acceptance criteria explicitly states: "WebSocket upgrade (HMR) works through the proxy." However, the proxy handler uses `reqwest::Client` which does not support WebSocket upgrade. When the dev server's HMR WebSocket connection hits the proxy at `/__vertz_hmr`, it will be forwarded as a regular HTTP GET request rather than upgraded to a WebSocket connection. The HMR handshake will fail.

This is a significant functional gap: developers using `http://<subdomain>.localhost` will not get hot module replacement.

**Fix:** Either implement WebSocket upgrade proxying (using `tokio-tungstenite` which is already a dev-dependency), or explicitly demote this from Phase 1 acceptance criteria and document it as a known limitation.

---

### SHOULD-FIX-4: `Init` and `Start` are nearly identical commands

**File:** `native/vtz/src/main.rs`, lines 978-1046

`ProxyCommand::Init` and `ProxyCommand::Start` have almost identical implementations — both clean stale routes, start the proxy daemon, write the PID file, and block until exit. The only difference is that `Init` prints a more verbose banner and `Start` checks for an already-running instance.

The design doc says `init` is for "first-time setup" (generate CA, install trust store) but Phase 1 has no CA/TLS, making `init` indistinguishable from `start`. This will confuse users: "When do I use `init` vs `start`?"

**Fix:** Either merge them into a single `start` command for Phase 1 (re-introduce `init` when TLS arrives in Phase 2), or make `init` a no-op that prints "proxy initialized" and only `start` runs the daemon. Having two commands that do the same thing is worse than having one.

---

### SHOULD-FIX-5: `vtz proxy stop` sends SIGTERM but doesn't verify the process died

**File:** `native/vtz/src/main.rs`, lines 1048-1065

```rust
unsafe {
    libc::kill(pid as libc::pid_t, libc::SIGTERM);
}
eprintln!("Stopped proxy (PID {})", pid);
```

After sending SIGTERM, the code immediately prints "Stopped" and removes the PID file, but never checks that the process actually terminated. If the proxy is stuck (e.g., blocked on I/O), SIGTERM may not kill it. The user will think it stopped, but the port is still bound. Next `start` will fail with "address in use."

**Fix:** After sending SIGTERM, briefly poll `is_pid_alive` (e.g., 5 attempts with 200ms sleep) to confirm termination. If the process survives, warn the user: "Proxy (PID {pid}) did not stop; you may need to `kill -9 {pid}`."

---

### SHOULD-FIX-6: Missing test for `--name` flag end-to-end

**File:** `native/vtz/src/proxy/client.rs`

The `register_dev_server` function accepts `name_override`, and the `--name` flag is wired in CLI. But there's no test in `client.rs` verifying that when `name_override = Some("dashboard")`, the resulting subdomain is `"dashboard"` and the route file is named `dashboard.json`. The existing tests only cover git branch detection and project name detection.

**Fix:** Add a test:
```rust
#[test]
fn register_dev_server_with_name_override_uses_sanitized_name() {
    // Test that name_override produces the expected subdomain
}
```

---

### SHOULD-FIX-7: `home_dir()` fallback to `/tmp` is problematic

**File:** `native/vtz/src/proxy/routes.rs`, lines 31-35

```rust
fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}
```

If `$HOME` is unset (rare but possible in sandboxed/containerized environments), route files end up in `/tmp/.vtz/proxy/routes/`. This directory is world-writable and cleared on reboot. More importantly, different users on the same machine would share the same route directory, leading to cross-user interference.

**Fix:** At minimum, use `dirs::home_dir()` (from the `dirs` crate) or fall back to a user-specific tmp directory. Alternatively, error out clearly: "Cannot determine home directory; set $HOME."

---

### NIT-1: `host.rs` module name is too generic

The module `host.rs` contains a single function `extract_subdomain`. The name `host` doesn't convey this. Consider renaming to `subdomain.rs` or keeping it but adding a module-level doc comment explaining its purpose.

---

### NIT-2: `ProxyState` fields should be private

**File:** `native/vtz/src/proxy/daemon.rs`, lines 16-23

All fields of `ProxyState` are `pub`. Only `route_table` and `client` are accessed outside the struct (by tests). Making fields private with accessor methods would be cleaner.

---

### NIT-3: Inconsistent error types — mixing `String`, `io::Error`, and silent `ok()` swallowing

Throughout the proxy modules:
- `load_route` returns `Result<RouteEntry, String>`
- `register_in` returns `std::io::Result<()>`
- `deregister_dev_server` swallows errors with `.ok()`
- `routes::register(&entry).ok()?` in `client.rs` silently drops the error

For a pre-v1 project, this is not blocking, but it would be cleaner to define a `ProxyError` enum using `thiserror` (per project conventions) and propagate errors consistently.

---

### NIT-4: `pid as libc::pid_t` cast could overflow on exotic platforms

**File:** `native/vtz/src/proxy/routes.rs`, line 99

`pid` is `u32` but `libc::pid_t` is `i32` on most platforms. PIDs above `i32::MAX` (2,147,483,647) are impossible on Linux/macOS, so this is academic, but `pid.try_into().unwrap_or(0)` would be more explicit.

---

### NIT-5: Proxy banner URL should include the port when not 80

**File:** `native/vtz/src/server/http.rs`, line 855

```rust
format!("http://{sub}.localhost").cyan().underline()
```

The proxy runs on port 4000 by default, but the banner shows `http://{sub}.localhost` without the port. Users clicking this URL will hit port 80, not the proxy. Should be `http://{sub}.localhost:4000` (using the actual proxy port).

However, this is tricky because the dev server doesn't know the proxy's port. The route file doesn't store it, and the proxy PID file doesn't either.

**Fix:** Store the proxy port in the PID file (or a separate config file in `~/.vtz/proxy/`), and have the client read it during registration. Then the banner can show the correct URL.

---

## Summary

| Severity | Count | Key Issues |
|----------|-------|------------|
| Blocker | 3 | Unbounded body read (DoS), XSS in dashboard, truncation trailing dash |
| Should-fix | 7 | Double FS reads, per-request reload, no WebSocket proxying, init/start duplication, stop without verify, missing --name test, /tmp fallback |
| Nit | 5 | Module naming, pub fields, error types, PID cast, banner URL |

### Verdict: **Changes Requested**

The three blockers must be addressed before merge. BLOCKER-1 and BLOCKER-3 are straightforward fixes (5-10 minutes each). BLOCKER-2 requires a small HTML-escape helper. Among the should-fixes, SHOULD-FIX-3 (no WebSocket proxying) is the most significant functional gap against the design doc's acceptance criteria.

## Resolution

_Pending — awaiting fixes._
