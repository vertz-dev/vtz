---
"@vertz/runtime": patch
---

### New Features
- **Plugin API** — `FrameworkPlugin` trait with React plugin (TSX compilation, HMR, React Refresh)
- **CSS file imports** — `import './styles.css'` injects styles in dev server
- **PostCSS pipeline** — CSS imports processed through PostCSS when configured
- **Asset imports** — `import logo from './logo.png'` resolves to URL strings
- **`import.meta.env`** — `.env` file loading with `VERTZ_` prefix filtering
- **tsconfig path aliases** — `paths` from `tsconfig.json` resolved in import rewriter
- **Reverse proxy** — subdomain routing, WebSocket proxying, TLS/HTTPS with auto-generated certs, `/etc/hosts` sync, loop detection
