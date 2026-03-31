/**
 * React Refresh Module Setup (vtz dev server)
 *
 * This module script runs after the bootstrap (react-refresh-runtime.js)
 * and before the app entry module. It dynamically imports react-refresh/runtime
 * from /@deps/ and initializes the global registration hooks.
 */
import RefreshRuntime from '/@deps/react-refresh/runtime';

if (typeof globalThis.__vtz_react_refresh_init === 'function') {
  globalThis.__vtz_react_refresh_init(RefreshRuntime);
}
