# import.meta.env Support

**Issue:** #39
**Status:** Implementation

## API Surface

```tsx
// Built-in variables
import.meta.env.DEV   // true during `vtz dev`
import.meta.env.PROD  // false during `vtz dev`
import.meta.env.MODE  // "development" during `vtz dev`

// .env file variables (public prefix required)
import.meta.env.VITE_API_URL  // from .env, .env.local, .env.development

// Whole object access
const env = import.meta.env  // Object.freeze({DEV: true, PROD: false, ...})
```

## Manifesto Alignment

- **Vite compatibility:** React projects using `import.meta.env.VITE_*` should work out of the box
- **Security by default:** Only variables with public prefixes are exposed to client code
- **Compile-time replacement:** Matches Vite behavior, enables tree-shaking

## Non-Goals

- Variable interpolation in .env files (`${VAR}` syntax)
- `import.meta.env.SSR` (server-only flag) — separate concern
- Build-time env (only dev mode for now)
- `.env.production`, `.env.staging` etc. (only `.env`, `.env.local`, `.env.development`)

## Approach: Compile-Time Replacement

Replace `import.meta.env.KEY` with literal values during compilation (same as Vite):
- `import.meta.env.VITE_API_URL` → `"https://api.example.com"`
- `import.meta.env.DEV` → `true`
- `import.meta.env` (whole object) → `Object.freeze({"DEV":true,...})`

This happens in the compilation pipeline after plugin compile + post-process, before import rewriting.

## .env File Loading Order

1. `.env` — base env vars (committed to git)
2. `.env.local` — local overrides (gitignored)
3. `.env.development` — mode-specific (committed)
4. `.env.development.local` — mode + local override (gitignored)

Later files override earlier ones. Only variables starting with a public prefix are included.

## Public Prefixes

Configured per plugin:
- **React plugin:** `VITE_` (Vite compat)
- **Vertz plugin:** `VERTZ_`

## Implementation Phases

### Phase 1: Env file parser + loader

New module: `native/vtz/src/env.rs`

- `parse_env_file(content: &str) -> HashMap<String, String>` — parse KEY=VALUE format
- `load_env_files(root_dir: &Path, mode: &str) -> HashMap<String, String>` — load + merge .env files
- `filter_public_env(env: &HashMap<String, String>, prefixes: &[&str]) -> HashMap<String, String>` — filter by prefix, add built-ins

Acceptance criteria:
- Parses KEY=VALUE, handles quotes, comments, empty lines, export prefix
- Loads files in correct precedence order
- Filters by public prefix
- Adds MODE, DEV, PROD built-in variables

### Phase 2: Compile-time env replacer

New module: `native/vtz/src/compiler/env_replacer.rs`

- `replace_import_meta_env(code: &str, env: &HashMap<String, String>) -> String`
- Replaces `import.meta.env.KEY` → literal value
- Replaces `import.meta.env` (no key) → Object.freeze({...})

Acceptance criteria:
- Replaces known keys with quoted string values
- Boolean built-ins (DEV, PROD) are unquoted true/false
- Whole object access returns frozen object literal
- Does not replace inside string literals or comments

### Phase 3: Integration with pipeline + server

- Add `env_public_prefixes()` method to `FrameworkPlugin` trait
- Add env map to `CompilationPipeline`
- Wire up in `build_router`: load env → create pipeline with env → compile-time replacement
- Add `env_replacer` step to compilation pipeline

Acceptance criteria:
- `import.meta.env.VITE_API_URL` in source → literal string in compiled output
- `import.meta.env.DEV` → `true` in compiled output
- React plugin advertises `VITE_` prefix
- Vertz plugin advertises `VERTZ_` prefix
