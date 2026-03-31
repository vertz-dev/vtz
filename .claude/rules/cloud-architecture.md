# Cloud Context — What VTZ Agents Need to Know

VTZ is a **dev-time tool** — it provides the development runtime (dev server, test runner, compiler, package manager) for the Vertz framework. It is NOT the cloud platform itself.

## Scope of VTZ

VTZ handles **local development** only:
- `vertz dev` — dev server with HMR, SSR, compiler pipeline
- `vertz test` — built-in test runner (V8 isolate per file)
- `vertz build` — production builds
- Native Rust compiler — JSX, signals, CSS transforms
- Package manager (future)

**Cloud infrastructure lives in separate repos.** Don't implement cloud services in vtz.

## Architectural Decisions to Be Aware Of

These decisions are owned by the framework and cloud repos, but agents should know them to avoid conflicting choices:

- **No third-party auth** — Vertz builds its own auth. Never suggest wrapping Better Auth, Auth.js, Clerk, or similar.
- **Asymmetric JWT (RS256)** — no HS256/symmetric secrets anywhere. If vtz handles JWT verification for dev-time features, use public keys only.
- **`rules.*` are declarative** — access rules are plain serializable objects (not functions). The compiler must preserve this property.
