# Report 012: Live Connection Status, Offline Queue & Cache, and Project/Tag Bridge

## 1. Feature #1: Offline Queue, Local Cache & Live Connection Status
- **Status Bar Revamp (`src/ui.rs`, `src/app.rs`)**:
  - Replaced `"Connected to ..."` / `"Offline: ..."` with a dedicated `ConnectionState` indicator:
    - `● Online` (Green)
    - `● Sync...` (Blue)
    - `● Offline` (Red)
  - Displays `[Q:N]` badge when offline mutations are queued.
- **Live Connection Detection (`src/app.rs`)**:
  - `App::tick()` probes `/health` every 2 seconds regardless of whether the app is currently online or offline.
  - Transitioning `Offline -> Online` immediately switches to `● Sync...` (Blue), replays queued offline mutations in order, refreshes tasks/projects/tags/counters, and transitions to `● Online` (Green).
  - Transitioning `Online -> Offline` immediately switches to `● Offline` (Red) while preserving the cached tasks, projects, tags, and counters.
- **Offline Queue & Local Cache (`src/offline.rs`)**:
  - Persists tasks, projects, tags, counters, local-only entities, custom ordering, and `pending_ops` (`CreateTask`, `UpdateTask`, `DeleteTask`, `SetCurrentTask`) to `super-productivity-tui/cache.json`.
  - Automatically falls back to queueing if Super Productivity closes mid-request.

## 2. Feature #11 & Bug #1: Project & Tag Management + Companion Plugin Bridge
- **Root Cause**: Verified against the running Super Productivity Local REST API (`http://127.0.0.1:3876`) that `POST /projects`, `PATCH /projects/:id`, `DELETE /projects/:id`, `POST /tags`, etc. all return `404 {"ok":false,"error":{"code":"NOT_FOUND","message":"Route not found"}}`. Only `GET /projects` and `GET /tags` are implemented in SP's REST handler.
- **Alternative Solution**:
  - **SP Companion Plugin Bridge (`src/bridge.rs`, `plugin/sp-tui-bridge/`)**: Super Productivity's internal Plugin API (`PluginAPI.addProject`, `updateProject`, `deleteProject`, `addTag`, `updateTag`, `deleteTag`) supports full Project & Tag mutations. `spt` now runs a lightweight local HTTP bridge on `127.0.0.1:3877` (configurable via `bridge_port` in `config.toml`), and `plugin/sp-tui-bridge.zip` polls it to execute Project/Tag CRUD inside Super Productivity and return the real SP-assigned IDs.
  - **Full Local CRUD & Reordering (`src/app.rs`, `src/ui.rs`)**: Added color/title editing (`c`), reordering (`u` / `n`), rename (`r`), delete (`d`), and persistent local caching so local Projects & Tags survive syncs and restarts even without the plugin installed.

## Verification
- `cargo fmt --all` — passed
- `cargo clippy --all-targets --all-features -- -D warnings` — passed (0 warnings)
- `cargo test --all-features` — 15 passed, 0 failed
