# Design: Runtime Plugin API

## Goal

Extract framework-specific behavior from the `vtz` runtime into a plugin interface, so the runtime becomes a generic dev server that frameworks plug into. The Vertz framework becomes the first plugin; React becomes the second, validating the abstraction.

## Manifesto Alignment

- **LLM-first design** — A plugin API makes the runtime composable and discoverable. LLMs can reason about "which plugin handles X" better than monolithic code.
- **Simplicity** — The runtime becomes simpler (less code, clearer responsibilities). Each plugin is self-contained.
- **Robustness** — Contract boundaries between runtime and plugin create testable seams. Bugs in one plugin can't corrupt the runtime.

## Non-Goals

- Plugin marketplace or dynamic loading from npm — plugins are compiled into the binary
- Supporting arbitrary plugin combinations (e.g., React + Vertz simultaneously)
- Backwards compatibility with current internal APIs — this is pre-v1, we break freely
- Hot-swappable plugins at runtime — one plugin per `vtz dev` invocation

## Approach: React-Driven Extraction

Instead of designing the plugin API in the abstract, we build the React plugin **concurrently** with the extraction. Each phase extracts one concern, implements it for both Vertz and React, and lets the real second consumer drive the trait design.

---

## Architecture Overview

```
┌──────────────────────────────────────────────────────────┐
│                       vtz runtime                         │
│                                                           │
│  ┌──────────┐  ┌──────────┐  ┌───────────────────────┐   │
│  │ Watcher  │  │ Module   │  │ HTTP Server (axum)    │   │
│  │ + Graph  │  │ Cache    │  │                       │   │
│  └────┬─────┘  └────┬─────┘  │ /__vtz_ai/* (core)   │   │
│       │              │        │ /__vtz_mcp  (core)    │   │
│       └──────────────┤        │ /__vtz_ai/x/* (plugin)│   │
│                      │        └───────────┬───────────┘   │
│              ┌───────▼───────┐            │               │
│              │  Plugin Trait │◄───────────┘               │
│              └───────┬───────┘                            │
│                      │                                    │
└──────────────────────┼────────────────────────────────────┘
                       │
          ┌────────────┼────────────┐
          │                         │
  ┌───────▼───────┐       ┌────────▼───────┐
  │ vertz-plugin  │       │  react-plugin  │
  │               │       │                │
  │ - signals     │       │ - React JSX    │
  │ - vertz JSX   │       │ - React Refresh│
  │ - fast refresh│       │ - RSC (future) │
  │ - SSR shim    │       │                │
  │               │       │                │
  │ MCP tools:    │       │ MCP tools:     │
  │ - api_spec    │       │ - component    │
  │ - (future)    │       │   _tree        │
  └───────────────┘       └────────────────┘
```

## The Plugin Trait

```rust
/// A framework plugin for the vtz dev server.
///
/// Implementations provide framework-specific compilation, HMR behavior,
/// and client-side scripts. The runtime handles everything else: file watching,
/// module graph, caching, HTTP serving, WebSocket transport.
pub trait FrameworkPlugin: Send + Sync {
    /// Human-readable name (e.g., "vertz", "react").
    fn name(&self) -> &str;

    // ── Compilation ─────────────────────────────────────────

    /// Compile a source file for browser consumption.
    ///
    /// Called on cache miss. The runtime handles caching, file reading,
    /// and import rewriting. The plugin only transforms the source.
    fn compile(&self, source: &str, ctx: &CompileContext) -> CompileOutput;

    /// Post-process compiled code before import rewriting.
    ///
    /// Default: no-op. Override for framework-specific fixups
    /// (e.g., Vertz's effect → domEffect rename).
    fn post_process(&self, code: &str, ctx: &CompileContext) -> String {
        code.to_string()
    }

    // ── Import Rewriting ────────────────────────────────────

    /// Resolve a bare import specifier to a path.
    ///
    /// Called for each bare specifier (e.g., "@vertz/ui", "react").
    /// Return None to use the default /@deps/ resolution.
    fn resolve_import(&self, specifier: &str) -> Option<String> {
        None
    }

    // ── HMR ─────────────────────────────────────────────────

    /// Client-side scripts injected into the HTML shell before the entry module.
    ///
    /// The runtime injects these as inline <script> tags. Typically includes:
    /// - Fast Refresh runtime (framework-specific)
    /// - HMR client (generic — runtime provides a default)
    /// - Error overlay (generic — runtime provides a default)
    fn hmr_client_scripts(&self) -> Vec<ClientScript>;

    /// Decide what HMR action to take for a file change.
    ///
    /// Default: entry file → full reload, CSS → CSS update, else → module update.
    /// Override for framework-specific logic (e.g., React can do component-level refresh).
    fn hmr_strategy(&self, result: &InvalidationResult) -> HmrAction {
        // Default implementation (current behavior)
        if result.is_entry_file {
            HmrAction::FullReload("entry file changed".into())
        } else if result.is_css_only {
            HmrAction::CssUpdate(result.changed_file.clone())
        } else {
            HmrAction::ModuleUpdate(result.invalidated_files.clone())
        }
    }

    // ── HTML Shell ──────────────────────────────────────────

    /// The ID of the root DOM element (default: "app").
    fn root_element_id(&self) -> &str {
        "app"
    }

    /// Additional <head> content for the HTML shell.
    fn head_html(&self) -> Option<String> {
        None
    }

    // ── File Watching ───────────────────────────────────────

    /// File extensions this plugin cares about (default: ts, tsx, css).
    fn watch_extensions(&self) -> Vec<String> {
        vec!["ts".into(), "tsx".into(), "css".into()]
    }

    /// Files that trigger a full server restart when changed.
    fn restart_triggers(&self) -> Vec<String> {
        // Default: package.json, tsconfig.json, .env
        vec![
            "package.json".into(),
            "tsconfig.json".into(),
            ".env".into(),
        ]
    }

    // ── Compilation Metadata ────────────────────────────────

    /// Whether this plugin wraps modules for Fast Refresh / HMR accept.
    fn supports_fast_refresh(&self) -> bool {
        false
    }

    /// Generate a module ID for Fast Refresh registry matching.
    /// Default: URL-relative path.
    fn module_id(&self, file_path: &Path, root_dir: &Path) -> String {
        file_path.strip_prefix(root_dir)
            .map(|p| format!("/{}", p.display()))
            .unwrap_or_else(|_| file_path.display().to_string())
    }

    // ── AI / MCP Extensibility ──────────────────────────────

    /// Additional MCP tool definitions contributed by this plugin.
    ///
    /// The runtime provides core tools (errors, console, navigate, diagnostics,
    /// events_url, render_page). Plugins can register extra tools for
    /// framework-specific introspection — e.g., component tree, state snapshot,
    /// route map, or devtools data.
    ///
    /// Tool names are automatically prefixed with the plugin name to avoid
    /// collisions: a tool named "component_tree" from the "react" plugin
    /// becomes "react_component_tree" in the MCP tool list.
    fn mcp_tool_definitions(&self) -> Vec<PluginMcpTool> {
        vec![]
    }

    /// Execute a plugin-contributed MCP tool by name.
    ///
    /// Called when `tools/call` receives a tool name matching one from
    /// `mcp_tool_definitions()`. The `name` is the unprefixed tool name.
    /// The plugin has access to `PluginContext` for reading server state.
    fn execute_mcp_tool(
        &self,
        name: &str,
        args: &serde_json::Value,
        ctx: &PluginContext,
    ) -> Result<serde_json::Value, String> {
        Err(format!("Unknown plugin tool: {}", name))
    }

    /// Additional HTTP routes contributed by this plugin.
    ///
    /// Mounted under `/__vtz_ai/x/<plugin_name>/`. The runtime calls this
    /// once at startup to get the plugin's routes. These complement the
    /// MCP tools — they're the same data exposed as REST endpoints for
    /// direct HTTP access (e.g., from browser devtools extensions).
    fn ai_routes(&self) -> Vec<PluginRoute> {
        vec![]
    }
}
```

### Supporting Types

```rust
/// Context passed to compile() — everything the plugin needs to know about the file.
pub struct CompileContext<'a> {
    pub file_path: &'a Path,
    pub root_dir: &'a Path,
    pub src_dir: &'a Path,
    /// "dom" for browser, "ssr" for server rendering
    pub target: &'a str,
}

/// Result of plugin compilation.
pub struct CompileOutput {
    /// Compiled JavaScript code.
    pub code: String,
    /// Extracted CSS, if any.
    pub css: Option<String>,
    /// Source map JSON, if available.
    pub source_map: Option<String>,
    /// Compilation errors (non-fatal — the runtime will create error modules).
    pub errors: Vec<CompileError>,
}

/// What the HMR system should do in response to a file change.
pub enum HmrAction {
    /// Full page reload.
    FullReload(String),
    /// Hot-update specific modules.
    ModuleUpdate(Vec<PathBuf>),
    /// Update CSS without reload.
    CssUpdate(PathBuf),
    /// Plugin handled it — runtime should do nothing.
    Handled,
}

/// A client-side script to inject into the HTML shell.
pub struct ClientScript {
    /// Script content (inline).
    pub content: String,
    /// Whether this is a module script (type="module").
    pub is_module: bool,
}

/// An MCP tool contributed by a plugin.
pub struct PluginMcpTool {
    /// Tool name (will be prefixed with plugin name, e.g., "component_tree"
    /// becomes "react_component_tree").
    pub name: String,
    /// Human-readable description for LLM tool discovery.
    pub description: String,
    /// JSON Schema for the tool's input parameters.
    pub input_schema: serde_json::Value,
}

/// An HTTP route contributed by a plugin.
pub struct PluginRoute {
    /// Route path (relative, e.g., "/component-tree").
    /// Mounted under `/__vtz_ai/x/<plugin_name><path>`.
    pub path: String,
    /// HTTP method.
    pub method: HttpMethod,
    /// Handler function. Receives the request body and plugin context,
    /// returns JSON response.
    pub handler: Box<dyn Fn(&[u8], &PluginContext) -> serde_json::Value + Send + Sync>,
}

/// Context passed to plugin MCP tool execution and HTTP route handlers.
///
/// Gives the plugin read access to runtime state without exposing internals.
pub struct PluginContext<'a> {
    /// Root directory of the project.
    pub root_dir: &'a Path,
    /// Source directory.
    pub src_dir: &'a Path,
    /// Server port.
    pub port: u16,
    /// Access to the error broadcaster (read-only).
    pub errors: &'a ErrorBroadcaster,
    /// Access to console log (read-only).
    pub console_log: &'a ConsoleLog,
    /// Access to the persistent V8 isolate (for querying app state).
    pub api_isolate: &'a Arc<RwLock<Option<Arc<PersistentIsolate>>>>,
}
```

---

## AI / MCP Extensibility

### Current State

The runtime exposes two layers of LLM-facing APIs, all hardcoded:

**REST endpoints (`/__vertz_ai/*`):**
- `GET /__vertz_ai/errors` — structured compilation/runtime errors
- `GET /__vertz_ai/render?url=/path` — SSR "text screenshot"
- `GET /__vertz_ai/console?last=N` — server diagnostic log
- `POST /__vertz_ai/navigate` — browser navigation via HMR WebSocket

**MCP tools (`/__vertz_mcp`):**
- `vertz_get_errors`, `vertz_render_page`, `vertz_get_console`, `vertz_navigate`, `vertz_get_diagnostics`, `vertz_get_events_url`, `vertz_get_api_spec`

**MCP event push (`/__vertz_mcp/events` WebSocket):**
- `error_update`, `file_change`, `hmr_update`, `ssr_refresh`, `typecheck_update`

### The Problem

All of these are hardcoded in `mcp.rs` (`tool_definitions()` + `execute_tool()`) and `http.rs` (route handlers). A React plugin can't expose a component tree inspector, a Vue plugin can't expose its devtools state, and a Svelte plugin can't expose its component hierarchy — because there's no hook to register framework-specific tools or endpoints.

### Design: Core + Plugin Tools

Split the AI/MCP surface into two tiers:

**Tier 1 — Core (runtime-provided, always available):**
These are framework-agnostic and stay in the runtime:
- `get_errors` — compilation errors (generic)
- `get_console` — server logs (generic)
- `navigate` — browser navigation via WebSocket (generic)
- `get_diagnostics` — server health (generic)
- `get_events_url` — real-time event push URL (generic)
- `render_page` — SSR text screenshot (generic, uses V8 isolate)

Naming changes: drop the `vertz_` prefix from core tools. They become `vtz_get_errors`, `vtz_navigate`, etc. The prefix matches the runtime, not the framework.

**Tier 2 — Plugin-contributed (framework-specific):**
Registered via `mcp_tool_definitions()` and `execute_mcp_tool()` on the trait. The runtime auto-prefixes tool names with the plugin name:

| Plugin tool name | MCP tool name | Example use |
|-----------------|---------------|-------------|
| `component_tree` | `react_component_tree` | React component hierarchy from React DevTools protocol |
| `api_spec` | `vertz_api_spec` | OpenAPI spec from Vertz's entity system (currently hardcoded) |
| `state_snapshot` | `react_state_snapshot` | Serialized React state tree |
| `route_map` | `vertz_route_map` | Extracted route definitions + params |

**HTTP routes follow the same split:**
- Core: `/__vtz_ai/errors`, `/__vtz_ai/console`, etc.
- Plugin: `/__vtz_ai/x/react/component-tree`, `/__vtz_ai/x/vertz/api-spec`

### How It Works in `mcp.rs`

```rust
// tool_definitions() becomes:
fn tool_definitions(plugin: &dyn FrameworkPlugin) -> serde_json::Value {
    let mut tools = core_tool_definitions(); // errors, console, navigate, etc.

    // Append plugin tools with prefixed names
    for tool in plugin.mcp_tool_definitions() {
        tools.push(serde_json::json!({
            "name": format!("{}_{}", plugin.name(), tool.name),
            "description": tool.description,
            "inputSchema": tool.input_schema,
        }));
    }

    serde_json::json!({ "tools": tools })
}

// execute_tool() becomes:
async fn execute_tool(
    state: &DevServerState,
    name: &str,
    args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    // Try core tools first
    if let Some(result) = execute_core_tool(state, name, args).await {
        return result;
    }

    // Strip plugin prefix and delegate to plugin
    let prefix = format!("{}_", state.plugin.name());
    if let Some(tool_name) = name.strip_prefix(&prefix) {
        let ctx = PluginContext::from_state(state);
        return state.plugin.execute_mcp_tool(tool_name, args, &ctx);
    }

    Err(format!("Unknown tool: {}", name))
}
```

### What This Enables

A React plugin author can expose rich framework introspection:

```rust
impl FrameworkPlugin for ReactPlugin {
    fn mcp_tool_definitions(&self) -> Vec<PluginMcpTool> {
        vec![
            PluginMcpTool {
                name: "component_tree".into(),
                description: "Get the current React component tree with props and state. \
                              Useful for understanding the UI structure and debugging \
                              rendering issues.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "depth": {
                            "type": "integer",
                            "description": "Max depth to traverse (default: unlimited)"
                        }
                    }
                }),
            },
        ]
    }

    fn execute_mcp_tool(
        &self,
        name: &str,
        args: &serde_json::Value,
        ctx: &PluginContext,
    ) -> Result<serde_json::Value, String> {
        match name {
            "component_tree" => {
                // Query the V8 isolate for React DevTools data
                // or inject a client-side script that reports back
                self.get_component_tree(args, ctx)
            }
            _ => Err(format!("Unknown tool: {}", name)),
        }
    }
}
```

An LLM connected via MCP would see:
```json
{
  "tools": [
    { "name": "vtz_get_errors", "description": "..." },
    { "name": "vtz_get_console", "description": "..." },
    { "name": "vtz_navigate", "description": "..." },
    { "name": "vtz_get_diagnostics", "description": "..." },
    { "name": "vtz_get_events_url", "description": "..." },
    { "name": "vtz_render_page", "description": "..." },
    { "name": "react_component_tree", "description": "..." }
  ]
}
```

### MCP Events Extensibility

Plugins can also contribute custom event types to the `/__vtz_mcp/events` WebSocket push:

```rust
/// Custom event types this plugin emits via the MCP event hub.
fn mcp_event_types(&self) -> Vec<String> {
    vec![]
}
```

For example, a React plugin could emit `react_render_profile` events when a component re-renders, giving the LLM real-time performance data without polling.

The runtime's `McpEventHub` already uses a broadcast channel — adding plugin events means letting plugins push to the same channel with a prefixed event type.

---

## Implementation Phases

### Phase 1: Extract the Plugin Trait + Vertz Plugin (3-4 days)

**Goal:** Define the trait, extract Vertz-specific code into a plugin, runtime calls the plugin instead of hardcoded logic. Everything works exactly as before — zero behavior change.

**What moves out of the runtime:**
- `vertz_compiler_core::compile()` call → `VertzPlugin::compile()`
- `post_process_compiled()` + `fix_compiler_api_names()` + `fix_module_id()` → `VertzPlugin::post_process()`
- Fast Refresh runtime/helpers JS assets → `VertzPlugin::hmr_client_scripts()`
- `RestartTriggers::default()` → `VertzPlugin::restart_triggers()`
- `vertz_get_api_spec` MCP tool → `VertzPlugin::mcp_tool_definitions()` + `execute_mcp_tool()`

**What stays in the runtime (generic):**
- `CompilationPipeline` (cache, CSS store, file I/O)
- `import_rewriter` (generic specifier rewriting — plugin can override specific specifiers)
- `HmrHub` + `HmrMessage` + WebSocket transport
- `FileWatcher` + `SmartDebouncer` + `ModuleGraph`
- `ErrorBroadcaster` + error overlay
- HTML shell generation (calls plugin for scripts + root element)
- Core MCP tools: `vtz_get_errors`, `vtz_get_console`, `vtz_navigate`, `vtz_get_diagnostics`, `vtz_get_events_url`, `vtz_render_page`
- MCP tool dispatch: core tools first, then delegate to `plugin.execute_mcp_tool()`
- HTTP route mounting: core routes + `/__vtz_ai/x/<plugin>/` for plugin routes

**Changes to `DevServerState`:**
```rust
pub struct DevServerState {
    pub plugin: Arc<dyn FrameworkPlugin>,  // NEW — replaces hardcoded vertz logic
    pub pipeline: CompilationPipeline,
    // ... rest unchanged
}
```

**Changes to `CompilationPipeline`:**
```rust
// Before:
let compile_result = vertz_compiler_core::compile(&source, ...);
let deduped = post_process_compiled(&compile_result.code);

// After:
let output = state.plugin.compile(&source, &ctx);
let processed = state.plugin.post_process(&output.code, &ctx);
```

**Changes to `build_router()`:**
```rust
// Before:
pub fn build_router(config: &ServerConfig) -> (Router, Arc<DevServerState>)

// After:
pub fn build_router(config: &ServerConfig, plugin: Arc<dyn FrameworkPlugin>) -> (Router, Arc<DevServerState>)
```

**Changes to HTML shell:**
```rust
// Before:
html.push_str(FAST_REFRESH_RUNTIME_JS);  // hardcoded
html.push_str(HMR_CLIENT_JS);             // hardcoded

// After:
for script in plugin.hmr_client_scripts() {
    let tag = if script.is_module { "module" } else { "text/javascript" };
    html.push_str(&format!("<script type=\"{}\">", tag));
    html.push_str(&script.content);
    html.push_str("</script>\n");
}
// HMR client and error overlay remain runtime-provided (generic)
```

**Acceptance Criteria:**
- [ ] `FrameworkPlugin` trait defined in `native/vtz/src/plugin/mod.rs`
- [ ] `VertzPlugin` implements `FrameworkPlugin` in `native/vtz/src/plugin/vertz.rs`
- [ ] `CompilationPipeline::compile_for_browser()` delegates to plugin
- [ ] HTML shell uses `plugin.hmr_client_scripts()` instead of hardcoded assets
- [ ] File watcher uses `plugin.restart_triggers()`
- [ ] HMR uses `plugin.hmr_strategy()` instead of hardcoded `invalidation_to_message()`
- [ ] `mcp.rs` split into core tools + plugin tool dispatch via `plugin.execute_mcp_tool()`
- [ ] `vertz_get_api_spec` moved to `VertzPlugin::mcp_tool_definitions()`
- [ ] Core MCP tools renamed from `vertz_*` to `vtz_*` (runtime prefix, not framework prefix)
- [ ] Plugin HTTP routes mounted under `/__vtz_ai/x/<plugin_name>/`
- [ ] All existing tests pass (zero behavior change, except tool name prefix)
- [ ] `cargo clippy --all-targets --release -- -D warnings` clean

---

### Phase 2: React Plugin — Compilation (3-4 days)

**Goal:** Create a React plugin that can compile React TSX files and serve them in the browser. No HMR yet — just compilation + static serving.

**React compilation approach:**
- TypeScript stripping: reuse `typescript_strip` from vertz-compiler-core (it's generic)
- JSX transform: use `oxc_transformer` (already available via oxc) to transform React JSX to `React.createElement` or the new JSX runtime (`jsx-runtime`)
- No signals, no mutations, no computed — none of the Vertz reactivity transforms

**New file:** `native/vtz/src/plugin/react.rs`

```rust
pub struct ReactPlugin {
    /// Whether to use the automatic JSX runtime (React 17+).
    pub jsx_runtime: JsxRuntime,
}

enum JsxRuntime {
    /// import { jsx } from 'react/jsx-runtime'
    Automatic,
    /// React.createElement
    Classic,
}
```

**What `ReactPlugin::compile()` does:**
1. Parse with oxc
2. Strip TypeScript
3. Transform JSX (React dialect: `className`, `htmlFor`, `onClick`, etc.)
4. Generate code with source map

**What it does NOT do:**
- Signal transforms (Vertz-specific)
- Mutation analysis (Vertz-specific)
- Props destructuring (Vertz-specific)
- Route splitting, field selection, hydration markers (Vertz-specific)

**Acceptance Criteria:**
- [ ] `ReactPlugin` compiles `.tsx` files with React JSX
- [ ] `ReactPlugin` strips TypeScript syntax
- [ ] `ReactPlugin` resolves `react` and `react-dom` as bare specifiers
- [ ] A simple React app (`createRoot(document.getElementById('app')).render(<App />)`) renders in the browser via `vtz dev --plugin react`
- [ ] Test: React JSX output matches expected `jsx()` calls
- [ ] Test: TypeScript syntax is stripped correctly

---

### Phase 3: React Plugin — HMR with React Refresh (2-3 days)

**Goal:** React Fast Refresh working in the browser. Edit a React component, see it update without full page reload.

**React Refresh integration:**
- React Refresh runtime (`react-refresh/runtime`) — injected via `hmr_client_scripts()`
- Babel-style React Refresh transform — wraps components with `$RefreshReg$` / `$RefreshSig$`
- We can use `oxc_transformer`'s React Refresh support (it has this built in)

**What `ReactPlugin` implements:**
```rust
fn hmr_client_scripts(&self) -> Vec<ClientScript> {
    vec![
        ClientScript {
            // React Refresh runtime bootstrap
            content: include_str!("../assets/react-refresh-runtime.js").to_string(),
            is_module: false,
        },
    ]
}

fn supports_fast_refresh(&self) -> bool {
    true
}

fn hmr_strategy(&self, result: &InvalidationResult) -> HmrAction {
    // React: same as default — module update triggers React Refresh
    // via the injected $RefreshReg$ / $RefreshSig$ wrappers
    HmrAction::default_strategy(result)
}
```

**Acceptance Criteria:**
- [ ] React Fast Refresh runtime injected into HTML shell
- [ ] Compiled React modules include `$RefreshReg$` / `$RefreshSig$` wrappers
- [ ] Editing a React component triggers HMR module update (not full reload)
- [ ] Component state is preserved across edits (React Refresh guarantee)
- [ ] CSS changes trigger CSS-only update (no React involvement)

---

### Phase 4: Plugin Selection + Config (1-2 days)

**Goal:** Users can select their framework plugin via CLI flag or config file.

**CLI:**
```bash
vtz dev                     # default: vertz plugin (auto-detected)
vtz dev --plugin react      # explicit: react plugin
```

**Config (`.vertzrc`):**
```json
{
  "plugin": "react"
}
```

**Auto-detection (default when no flag/config):**
1. Check `package.json` dependencies for `react` → ReactPlugin
2. Check for `@vertz/ui` → VertzPlugin
3. Fallback → VertzPlugin (backwards compatible)

**Changes to `cli.rs`:**
```rust
#[arg(long, value_enum)]
plugin: Option<PluginChoice>,
```

**Changes to `config.rs`:**
```rust
pub struct ServerConfig {
    pub plugin: PluginChoice,  // NEW
    // ... rest unchanged
}
```

**Acceptance Criteria:**
- [ ] `--plugin react` selects ReactPlugin
- [ ] `--plugin vertz` selects VertzPlugin (explicit)
- [ ] No flag + `.vertzrc` plugin field works
- [ ] Auto-detection from `package.json` works
- [ ] Default (no flag, no config) is VertzPlugin (backwards compatible)

---

### Phase 5: Cleanup + Hardening (2 days)

**Goal:** Remove dead code, ensure test coverage, document the plugin API.

**Tasks:**
- Remove the old hardcoded Vertz paths from `pipeline.rs` (now in VertzPlugin)
- Remove the old hardcoded Fast Refresh assets from `html_shell.rs` (now in plugin)
- Remove `post_process_compiled()` and `fix_compiler_api_names()` from pipeline (now in VertzPlugin::post_process)
- Add integration tests: start server with each plugin, verify compilation and HMR
- Document `FrameworkPlugin` trait with rustdoc examples
- Ensure both plugins pass all quality gates independently

**Acceptance Criteria:**
- [ ] No Vertz-specific code remains in the runtime core (only in `plugin/vertz.rs`)
- [ ] `cargo test --all` passes
- [ ] `cargo clippy --all-targets --release -- -D warnings` clean
- [ ] Both `--plugin vertz` and `--plugin react` produce working dev servers
- [ ] 95%+ coverage on new `plugin/` module files

---

## Unknowns

| Unknown | Resolution |
|---------|------------|
| Can oxc_transformer handle React JSX + React Refresh? | Needs POC — oxc has React JSX support, but React Refresh transform may need custom impl |
| Should `import_rewriter` be partially pluggable? | Phase 1 will reveal — if React needs different rewrite rules, add `resolve_import()` to trait |
| How does SSR work per-plugin? | Deferred — SSR is behind a V8 isolate and mostly generic. React SSR (renderToString) can be a future phase |
| Error overlay — generic or plugin-specific? | Start generic (current overlay works for any framework), add plugin hooks later if needed |
| How do plugin MCP tools query client-side state (e.g., React component tree)? | Two approaches: (1) inject a client-side script that reports to a server endpoint, or (2) query the V8 isolate if using SSR. Phase 3 will reveal which approach React needs |
| Should plugin MCP events be typed or generic JSON? | Start generic (plugin pushes `serde_json::Value` with a string event type). Add typed events later if warranted |
| How to handle `execute_mcp_tool` needing async? | The trait method can return a future or we use `async_trait`. Phase 1 will pick the approach based on what Vertz's `api_spec` tool needs (it already queries the V8 isolate asynchronously) |

## Risk Assessment

| Risk | Mitigation |
|------|-----------|
| Trait too narrow — React needs something Vertz doesn't | React plugin is built concurrently, so we discover this immediately and widen the trait |
| Trait too wide — unnecessary abstraction points | Start with the minimum trait surface. Every method has a default impl. Only add methods when a real plugin needs them |
| Performance regression from dynamic dispatch | `dyn FrameworkPlugin` is called once per compilation (not per-line). Overhead is negligible vs. I/O and parsing |
| Breaking existing Vertz behavior during extraction | Phase 1 is pure refactor — every existing test must pass. No behavior change allowed |

## File Structure After Implementation

```
native/vtz/src/
├── plugin/
│   ├── mod.rs          # FrameworkPlugin trait + supporting types (PluginMcpTool, PluginContext, etc.)
│   ├── vertz.rs        # VertzPlugin: compilation, HMR, MCP tools (api_spec, route_map)
│   └── react.rs        # ReactPlugin: compilation, HMR, MCP tools (component_tree)
├── hmr/
│   ├── mod.rs          # Calls plugin.hmr_strategy() instead of hardcoded logic
│   ├── protocol.rs     # HmrMessage (unchanged — generic)
│   ├── websocket.rs    # HmrHub (unchanged — generic transport)
│   └── recovery.rs     # IsolateHealth (unchanged)
├── compiler/
│   ├── pipeline.rs     # Delegates to plugin.compile() + plugin.post_process()
│   ├── cache.rs        # Unchanged
│   └── import_rewriter.rs  # Calls plugin.resolve_import() for overrides
├── server/
│   ├── http.rs         # build_router takes plugin param, mounts plugin AI routes
│   ├── module_server.rs  # DevServerState holds Arc<dyn FrameworkPlugin>
│   ├── html_shell.rs   # Uses plugin.hmr_client_scripts() + plugin.root_element_id()
│   └── mcp.rs          # Core tools (vtz_*) + plugin tool dispatch via plugin.execute_mcp_tool()
└── ...
```

## Timeline

| Phase | Effort | Cumulative |
|-------|--------|-----------|
| Phase 1: Trait + Vertz extraction | 3-4 days | 3-4 days |
| Phase 2: React compilation | 3-4 days | 6-8 days |
| Phase 3: React HMR | 2-3 days | 8-11 days |
| Phase 4: Plugin selection + config | 1-2 days | 9-13 days |
| Phase 5: Cleanup + hardening | 2 days | 11-15 days |

**Total estimate: ~2-3 weeks**

Phase 1 is the critical path — it defines the abstraction boundary. If Phase 1 goes cleanly (existing tests pass, no surprises), the rest is straightforward. If Phase 1 reveals that the coupling is deeper than expected, we adjust the trait before proceeding.
