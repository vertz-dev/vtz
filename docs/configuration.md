# Configuration

VTZ is configured via a `.vertzrc` file at the project root. The file uses JSON format.

```json
{
  "plugin": "react",
  "autoInstall": true,
  "proxy": {
    "/api": {
      "target": "http://localhost:8080",
      "rewrite": { "^/api": "" },
      "changeOrigin": true,
      "headers": { "X-Accel-Buffering": "no" }
    }
  }
}
```

You can also manage config via the CLI:

```bash
vtz config set autoInstall false
vtz config set plugin react
vtz config get autoInstall
```

## Options

### `plugin`

**Type:** `"vertz" | "react"`
**Default:** auto-detected from `package.json`

Which framework plugin to use. When omitted, VTZ checks your `package.json` dependencies:

- If `react` is listed in `dependencies` or `devDependencies` -> `"react"`
- Otherwise -> `"vertz"`

You can also override via CLI: `vtz dev --plugin react`

**Precedence:** CLI flag > `.vertzrc` > auto-detect from `package.json` > `"vertz"`

### `autoInstall`

**Type:** `boolean`
**Default:** `true`

When `true`, VTZ automatically installs missing packages when they are imported during development. Disabled automatically in CI environments.

### `proxy`

**Type:** `object`
**Default:** none

Configures the dev server to forward matching HTTP requests to a backend server. This is useful when your frontend needs to talk to a separate API server during development, avoiding CORS issues.

Each key is a path prefix to match, and the value configures where and how to forward.

```json
{
  "proxy": {
    "/api": {
      "target": "http://localhost:8080",
      "rewrite": { "^/api": "" },
      "changeOrigin": true,
      "headers": { "X-Accel-Buffering": "no" }
    }
  }
}
```

#### Proxy rule options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `target` | `string` | **(required)** | The base URL to forward requests to. |
| `rewrite` | `object` | `{}` | Regex-based path rewrite rules. Keys are regex patterns, values are replacements. Applied in order. |
| `changeOrigin` | `boolean` | `false` | Set the `Host` header to the target's host (useful when the backend checks the Host header). |
| `headers` | `object` | `{}` | Custom headers to inject on every proxied request. |

#### Examples

**Simple proxy** -- forward `/api/*` to a backend on port 8080:

```json
{
  "proxy": {
    "/api": {
      "target": "http://localhost:8080"
    }
  }
}
```

A request to `http://localhost:3000/api/users` is forwarded to `http://localhost:8080/api/users`.

**Strip the prefix** -- forward `/api/users` as `/users` on the backend:

```json
{
  "proxy": {
    "/api": {
      "target": "http://localhost:8080",
      "rewrite": { "^/api": "" }
    }
  }
}
```

A request to `http://localhost:3000/api/users` is forwarded to `http://localhost:8080/users`.

**Multiple proxies** -- route different prefixes to different backends:

```json
{
  "proxy": {
    "/api": {
      "target": "http://localhost:8080",
      "rewrite": { "^/api": "" }
    },
    "/auth": {
      "target": "http://localhost:9000"
    }
  }
}
```

When multiple prefixes match, the longest prefix wins. For example, if both `/api` and `/api/v2` are configured, a request to `/api/v2/users` matches the `/api/v2` rule.

**SSE / streaming endpoints** -- responses are streamed by default, so Server-Sent Events and long-lived connections work without additional configuration. If your backend uses proxy buffering headers:

```json
{
  "proxy": {
    "/api": {
      "target": "http://localhost:8080",
      "headers": { "X-Accel-Buffering": "no" }
    }
  }
}
```

### `extraWatchPaths`

**Type:** `string[]`
**Default:** `[]`

Additional directories to watch for changes during `vtz dev`. Paths are relative to the project root. Useful in monorepo setups where shared libraries are built outside the project directory.

```json
{
  "extraWatchPaths": ["../shared-lib/build", "../common/dist"]
}
```

### `trustScripts`

**Type:** `string[]`
**Default:** `[]`

Package names (or scope patterns) whose postinstall scripts are allowed to run during `vtz install`. By default, postinstall scripts are blocked for security.

- Exact names: `"esbuild"` matches only `esbuild`
- Scope wildcards: `"@vertz/*"` matches any package under `@vertz/`

```json
{
  "trustScripts": ["esbuild", "@prisma/client", "@vertz/*"]
}
```

You can initialize this list from your existing `node_modules`:

```bash
vtz config init trust-scripts
```

## File loading

VTZ loads `.vertzrc` from your project root directory. The file is optional -- all options have sensible defaults. Unknown fields are preserved during read/write cycles, so forward compatibility is maintained.

Changes to `.vertzrc` require a server restart to take effect (the dev server watches for changes to `.vertzrc` and triggers an automatic restart).
