use crate::proxy::host::extract_subdomain;
use crate::proxy::routes::{self, RouteEntry};
use axum::body::Body;
use axum::extract::State;
use axum::http::{header, Request, StatusCode};
use axum::response::Response;
use axum::Router;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::RwLock;

/// Shared state for the proxy daemon.
#[derive(Debug, Clone)]
pub struct ProxyState {
    /// Map from subdomain to route entry.
    pub route_table: Arc<RwLock<HashMap<String, RouteEntry>>>,
    /// The routes directory to watch.
    pub routes_dir: PathBuf,
    /// HTTP client for forwarding requests.
    pub client: reqwest::Client,
}

impl ProxyState {
    pub fn new(routes_dir: PathBuf) -> Self {
        Self {
            route_table: Arc::new(RwLock::new(HashMap::new())),
            routes_dir,
            client: reqwest::Client::new(),
        }
    }

    /// Reload routes from disk into the in-memory table.
    pub async fn reload_routes(&self) {
        let entries = routes::load_all_routes_in(&self.routes_dir);
        let mut table = self.route_table.write().await;
        table.clear();
        for entry in entries {
            table.insert(entry.subdomain.clone(), entry);
        }
    }

    /// Look up a route by subdomain.
    pub async fn lookup(&self, subdomain: &str) -> Option<RouteEntry> {
        let table = self.route_table.read().await;
        table.get(subdomain).cloned()
    }
}

/// Build the proxy router.
pub fn build_router(state: ProxyState) -> Router {
    Router::new().fallback(proxy_handler).with_state(state)
}

/// Main proxy handler: extract subdomain, look up route, forward request.
async fn proxy_handler(State(state): State<ProxyState>, req: Request<Body>) -> Response<Body> {
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let subdomain = match extract_subdomain(host) {
        Some(s) => s,
        None => return dashboard_response(&state).await,
    };

    // Reload routes for fresh data (cheap for small route counts)
    state.reload_routes().await;

    let route = match state.lookup(&subdomain).await {
        Some(r) => r,
        None => {
            return Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .header(header::CONTENT_TYPE, "text/plain")
                .body(Body::from(format!(
                    "No dev server registered for subdomain: {subdomain}"
                )))
                .unwrap();
        }
    };

    forward_request(req, route.port, &state.client).await
}

/// Forward an HTTP request to the target port on localhost.
async fn forward_request(
    req: Request<Body>,
    target_port: u16,
    client: &reqwest::Client,
) -> Response<Body> {
    let (parts, body) = req.into_parts();
    let uri = parts.uri.clone();
    let path_and_query = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/");
    let target_url = format!("http://127.0.0.1:{target_port}{path_and_query}");

    let method = match parts.method {
        axum::http::Method::GET => reqwest::Method::GET,
        axum::http::Method::POST => reqwest::Method::POST,
        axum::http::Method::PUT => reqwest::Method::PUT,
        axum::http::Method::DELETE => reqwest::Method::DELETE,
        axum::http::Method::PATCH => reqwest::Method::PATCH,
        axum::http::Method::HEAD => reqwest::Method::HEAD,
        axum::http::Method::OPTIONS => reqwest::Method::OPTIONS,
        _ => reqwest::Method::GET,
    };

    // Collect body bytes
    let body_bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(b) => b,
        Err(e) => {
            return Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .body(Body::from(format!("Failed to read request body: {e}")))
                .unwrap();
        }
    };

    // Build forwarded request
    let mut forwarded = client
        .request(method, &target_url)
        .body(body_bytes.to_vec());

    // Forward relevant headers (skip Host — the target gets its own)
    for (name, value) in &parts.headers {
        if name == header::HOST {
            continue;
        }
        if let Ok(v) = value.to_str() {
            forwarded = forwarded.header(name.as_str(), v);
        }
    }

    match forwarded.send().await {
        Ok(resp) => convert_response(resp).await,
        Err(e) => Response::builder()
            .status(StatusCode::BAD_GATEWAY)
            .header(header::CONTENT_TYPE, "text/plain")
            .body(Body::from(format!("Proxy error: {e}")))
            .unwrap(),
    }
}

/// Convert a reqwest::Response into an axum Response.
async fn convert_response(resp: reqwest::Response) -> Response<Body> {
    let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut builder = Response::builder().status(status);

    for (name, value) in resp.headers() {
        builder = builder.header(name.as_str(), value.as_bytes());
    }

    match resp.bytes().await {
        Ok(bytes) => builder.body(Body::from(bytes)).unwrap(),
        Err(e) => Response::builder()
            .status(StatusCode::BAD_GATEWAY)
            .body(Body::from(format!("Failed to read response body: {e}")))
            .unwrap(),
    }
}

/// Dashboard page listing all registered dev servers.
async fn dashboard_response(state: &ProxyState) -> Response<Body> {
    state.reload_routes().await;
    let table = state.route_table.read().await;

    if table.is_empty() {
        let html = r#"<!DOCTYPE html>
<html><head><title>Vertz Proxy</title></head>
<body>
<h1>&#9650; Vertz Proxy</h1>
<p>No dev servers registered.</p>
<p>Start a dev server with <code>vtz dev</code> to see it here.</p>
</body></html>"#;
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(Body::from(html))
            .unwrap();
    }

    let mut rows = String::new();
    for entry in table.values() {
        rows.push_str(&format!(
            "<tr><td><a href=\"http://{sub}.localhost\">{sub}</a></td>\
             <td>{port}</td><td>{branch}</td><td>{pid}</td></tr>",
            sub = entry.subdomain,
            port = entry.port,
            branch = entry.branch,
            pid = entry.pid,
        ));
    }

    let html = format!(
        r#"<!DOCTYPE html>
<html><head><title>Vertz Proxy</title></head>
<body>
<h1>&#9650; Vertz Proxy</h1>
<table>
<tr><th>Subdomain</th><th>Port</th><th>Branch</th><th>PID</th></tr>
{rows}
</table>
</body></html>"#
    );

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(Body::from(html))
        .unwrap()
}

/// Start the proxy daemon, returning the actual bound port.
pub async fn start_proxy(
    port: u16,
    routes_dir: PathBuf,
) -> std::io::Result<(u16, tokio::task::JoinHandle<()>)> {
    let state = ProxyState::new(routes_dir);
    state.reload_routes().await;
    let router = build_router(state);

    let addr = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&addr).await?;
    let actual_port = listener.local_addr()?.port();

    let handle = tokio::spawn(async move {
        axum::serve(listener, router).await.ok();
    });

    Ok((actual_port, handle))
}

/// Write the proxy PID file.
pub fn write_pid_file(proxy_dir: &Path, pid: u32) -> std::io::Result<()> {
    std::fs::create_dir_all(proxy_dir)?;
    std::fs::write(proxy_dir.join("proxy.pid"), pid.to_string())
}

/// Read the proxy PID file.
pub fn read_pid_file(proxy_dir: &Path) -> Option<u32> {
    std::fs::read_to_string(proxy_dir.join("proxy.pid"))
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

/// Remove the proxy PID file.
pub fn remove_pid_file(proxy_dir: &Path) -> std::io::Result<()> {
    let path = proxy_dir.join("proxy.pid");
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Method;

    fn test_routes_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let routes = dir.path().join("routes");
        std::fs::create_dir_all(&routes).unwrap();
        dir
    }

    fn test_route_entry(subdomain: &str, port: u16) -> RouteEntry {
        RouteEntry {
            subdomain: subdomain.to_string(),
            port,
            branch: "feat/test".to_string(),
            project: "test-app".to_string(),
            pid: std::process::id(),
            root_dir: PathBuf::from("/tmp/test"),
        }
    }

    #[tokio::test]
    async fn proxy_state_reload_routes_populates_table() {
        let dir = test_routes_dir();
        let routes = dir.path().join("routes");
        let entry = test_route_entry("feat-test.test-app", 3000);
        routes::register_in(&routes, &entry).unwrap();

        let state = ProxyState::new(routes);
        state.reload_routes().await;

        let result = state.lookup("feat-test.test-app").await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().port, 3000);
    }

    #[tokio::test]
    async fn proxy_state_lookup_returns_none_for_unknown() {
        let dir = test_routes_dir();
        let routes = dir.path().join("routes");
        let state = ProxyState::new(routes);
        state.reload_routes().await;

        assert!(state.lookup("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn dashboard_shows_no_servers_when_empty() {
        let dir = test_routes_dir();
        let routes = dir.path().join("routes");
        let state = ProxyState::new(routes);

        let resp = dashboard_response(&state).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let html = String::from_utf8(body.to_vec()).unwrap();
        assert!(html.contains("No dev servers registered"));
    }

    #[tokio::test]
    async fn dashboard_lists_registered_servers() {
        let dir = test_routes_dir();
        let routes = dir.path().join("routes");
        let entry = test_route_entry("feat-test.test-app", 3000);
        routes::register_in(&routes, &entry).unwrap();

        let state = ProxyState::new(routes);
        let resp = dashboard_response(&state).await;
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let html = String::from_utf8(body.to_vec()).unwrap();
        assert!(html.contains("feat-test.test-app"));
        assert!(html.contains("3000"));
    }

    #[tokio::test]
    async fn proxy_handler_returns_bad_gateway_for_unknown_subdomain() {
        let dir = test_routes_dir();
        let routes = dir.path().join("routes");
        let state = ProxyState::new(routes);

        let req = Request::builder()
            .method(Method::GET)
            .uri("/")
            .header(header::HOST, "unknown.localhost:4000")
            .body(Body::empty())
            .unwrap();

        let resp = proxy_handler(State(state), req).await;
        assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
    }

    #[tokio::test]
    async fn proxy_handler_returns_dashboard_for_root() {
        let dir = test_routes_dir();
        let routes = dir.path().join("routes");
        let state = ProxyState::new(routes);

        let req = Request::builder()
            .method(Method::GET)
            .uri("/")
            .header(header::HOST, "localhost:4000")
            .body(Body::empty())
            .unwrap();

        let resp = proxy_handler(State(state), req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let html = String::from_utf8(body.to_vec()).unwrap();
        assert!(html.contains("Vertz Proxy"));
    }

    // --- PID file tests ---

    #[test]
    fn write_and_read_pid_file() {
        let dir = tempfile::tempdir().unwrap();
        write_pid_file(dir.path(), 12345).unwrap();
        assert_eq!(read_pid_file(dir.path()), Some(12345));
    }

    #[test]
    fn read_pid_file_returns_none_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_pid_file(dir.path()), None);
    }

    #[test]
    fn remove_pid_file_works() {
        let dir = tempfile::tempdir().unwrap();
        write_pid_file(dir.path(), 12345).unwrap();
        remove_pid_file(dir.path()).unwrap();
        assert_eq!(read_pid_file(dir.path()), None);
    }

    #[test]
    fn remove_pid_file_nonexistent_is_ok() {
        let dir = tempfile::tempdir().unwrap();
        assert!(remove_pid_file(dir.path()).is_ok());
    }

    // --- Integration test: proxy forwards to a real backend ---

    #[tokio::test]
    async fn proxy_forwards_request_to_backend() {
        // Start a tiny backend server
        let backend = axum::Router::new().route(
            "/hello",
            axum::routing::get(|| async { "Hello from backend!" }),
        );
        let backend_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let backend_port = backend_listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            axum::serve(backend_listener, backend).await.ok();
        });

        // Register the backend as a route
        let dir = test_routes_dir();
        let routes = dir.path().join("routes");
        let entry = test_route_entry("test-app", backend_port);
        routes::register_in(&routes, &entry).unwrap();

        // Start the proxy
        let (proxy_port, _handle) = start_proxy(0, routes).await.unwrap();

        // Send request through proxy
        let client = reqwest::Client::new();
        let resp = client
            .get(format!("http://127.0.0.1:{proxy_port}/hello"))
            .header("Host", "test-app.localhost")
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), 200);
        assert_eq!(resp.text().await.unwrap(), "Hello from backend!");
    }
}
