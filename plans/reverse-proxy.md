# Reverse Proxy with HTTPS and Subdomain Routing

**Issue:** [#44](https://github.com/vertz-dev/vtz/issues/44)
**Status:** In Progress

## API Surface

### CLI Commands

```bash
# One-time setup: generate CA, install in trust store, start daemon
vtz proxy init

# Daemon lifecycle
vtz proxy start
vtz proxy stop
vtz proxy status

# Trust store (re-install CA)
vtz proxy trust
```

### Auto-Registration (from `vtz dev`)

```rust
// On startup — writes route file to ~/.vtz/proxy/routes/<name>.json
proxy::routes::register(RouteEntry {
    subdomain: "feat-auth.my-app".to_string(),
    port: 3000,
    branch: "feat/auth".to_string(),
    project: "my-app".to_string(),
    pid: std::process::id(),
    root_dir: PathBuf::from("/path/to/worktree"),
});

// On shutdown — removes route file
proxy::routes::deregister("feat-auth.my-app");
```

### Subdomain Naming

```rust
// Pure function: branch + project → subdomain
proxy::naming::to_subdomain("feat/auth-system", "my-app")
// → "feat-auth-system.my-app"

proxy::naming::to_subdomain("main", "my-app")
// → "my-app" (no prefix for default branch)

// Sanitization rules:
// - Replace `/` with `-`
// - Replace non-alphanumeric (except `-`) with `-`
// - Collapse consecutive `-`
// - Lowercase everything
// - Truncate to 63 chars (DNS label limit)
proxy::naming::sanitize_label("feat/Auth_System!!!")
// → "feat-auth-system"
```

### Dev Server Integration

```rust
// DevArgs gets --name flag for custom subdomain override
vtz dev --name dashboard
// → https://dashboard.localhost

// Banner shows HTTPS URL when proxy is available
// ▲ Vertz Dev Server
//   Local:   http://localhost:3000
//   Proxy:   https://feat-auth.my-app.localhost
```

## Manifesto Alignment

- **Developer experience first:** No port juggling when running multiple worktrees. Human-readable URLs instead of port numbers.
- **LLM-first design:** AI agents running parallel worktrees get stable, identifiable URLs automatically — no configuration needed.
- **Zero-config by default:** `vtz dev` auto-registers; proxy detects branch + project name automatically. Override available via `--name`.
- **Graceful degradation:** If proxy isn't running, `vtz dev` works normally on its port.

## Non-Goals

- Windows support (deferred)
- Production/deployment proxy
- Load balancing or multi-instance routing
- Custom domain names pointing to non-localhost IPs
- HTTPS in Phase 1 (TLS is Phase 2)

## Unknowns

- **Safari `.localhost` resolution:** Safari doesn't resolve `*.localhost` natively. Phase 3 addresses this with `/etc/hosts` sync or DNS stub. Not a blocker for Phase 1.
- **HTTP/2 without TLS:** Most browsers require TLS for HTTP/2 (h2). Phase 1 uses HTTP/1.1 for the proxy; HTTP/2 comes with TLS in Phase 2.

## Implementation Plan

### Phase 1: Core proxy + auto-registration (HTTP only)

Proxy daemon on a configurable port (default: 4000) that routes by subdomain. No TLS yet.

**Acceptance Criteria:**
- `vtz proxy start` starts an HTTP proxy daemon
- `vtz proxy stop` stops the daemon
- `vtz proxy status` lists registered dev servers
- `vtz dev` auto-registers with proxy, shows proxy URL in banner
- Requests to `http://<subdomain>.localhost:<proxy-port>` are forwarded to the correct dev server port
- WebSocket upgrade (HMR) works through the proxy
- Stale PIDs are cleaned up automatically
- `vtz dev` works normally when proxy is not running (graceful degradation)

### Phase 2: TLS + trust store

Certificate generation via `rcgen`, trust store installation, HTTPS serving.

### Phase 3: DNS helpers + Safari support

`/etc/hosts` sync, DNS stub resolver, custom TLD support.

### Phase 4: Polish + loop detection

Loop detection, proxy logs, status in dev server banner polish.

## File Layout

```
native/vtz/src/proxy/
  mod.rs          — module declarations
  naming.rs       — subdomain sanitization and naming
  routes.rs       — route file management (read/write/stale detection)
  daemon.rs       — proxy HTTP server (axum)
  client.rs       — registration client (used by dev server)

~/.vtz/proxy/
  proxy.pid       — daemon PID file
  proxy.log       — daemon log
  routes/
    <subdomain>.json  — route registration files
```
