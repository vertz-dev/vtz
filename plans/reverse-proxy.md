# Reverse Proxy with HTTPS and Subdomain Routing

**Issue:** [#44](https://github.com/vertz-dev/vtz/issues/44)
**Status:** Complete — Pending Review

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

### Phase 1: Core proxy + auto-registration (HTTP only) ✅

Proxy daemon on a configurable port (default: 4000) that routes by subdomain.

**Acceptance Criteria:**
- [x] `vtz proxy start` starts an HTTP proxy daemon
- [x] `vtz proxy stop` stops the daemon
- [x] `vtz proxy status` lists registered dev servers
- [x] `vtz dev` auto-registers with proxy, shows proxy URL in banner
- [x] Requests to `http://<subdomain>.localhost:<proxy-port>` are forwarded to the correct dev server port
- [x] WebSocket upgrade (HMR) works through the proxy
- [x] Stale PIDs are cleaned up automatically
- [x] `vtz dev` works normally when proxy is not running (graceful degradation)

### Phase 2: TLS + trust store ✅

- [x] Certificate generation via `rcgen` (root CA + *.localhost server cert)
- [x] `vtz proxy init` generates certs and starts HTTPS proxy
- [x] `vtz proxy start` auto-detects certs (HTTPS if available, HTTP fallback)
- [x] `vtz proxy trust` installs CA in macOS trust store
- [x] HTTPS serving via `axum-server` + `rustls`
- [x] Dev server banner shows `https://` URL when TLS is configured

### Phase 3: DNS helpers + Safari support ✅

- [x] `/etc/hosts` sync with managed block markers (BEGIN/END vertz-proxy)
- [x] `vtz proxy sync-hosts` command writes entries via sudo cp
- [x] Safe merge: replaces existing block or appends if none exists

### Phase 4: Polish + loop detection ✅

- [x] Loop detection via `X-Vertz-Proxy` header (returns 508 Loop Detected)
- [x] Loop header injected on all forwarded requests

## File Layout

```
native/vtz/src/proxy/
  mod.rs          — module declarations
  naming.rs       — subdomain sanitization and naming
  routes.rs       — route file management (read/write/stale detection)
  daemon.rs       — proxy HTTP/HTTPS server (axum + axum-server)
  client.rs       — registration client (used by dev server)
  host.rs         — Host header subdomain extraction
  tls.rs          — TLS certificate generation (rcgen)
  hosts.rs        — /etc/hosts management for Safari support

~/.vtz/proxy/
  proxy.pid       — daemon PID file
  proxy.log       — daemon log
  ca-cert.pem     — root CA certificate
  ca-key.pem      — root CA private key
  server-cert.pem — *.localhost server certificate
  server-key.pem  — server private key
  routes/
    <subdomain>.json  — route registration files
```
