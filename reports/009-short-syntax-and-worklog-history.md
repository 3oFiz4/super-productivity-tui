# Feature & Research Report: Short Syntax & Worklog/History

## 1. Alternative Ways to Create Projects & Tags in Super Productivity
- Confirmed from `electron/local-rest-api.ts` and `super-productivity-cli`: the Local REST API only exposes `GET /projects` and `GET /tags` (no `POST /projects` or `POST /tags` routes).
- **Alternatives if you want real creation:**
  1. **Plugin API Bridge (recommended):** Write a small Super Productivity internal plugin (`@super-productivity/plugin-api`) that exposes `addProject` / `addTag` over a local port or IPC.
  2. **Sync/Backup File Write:** Modify Super Productivity's sync JSON directly only when the desktop app is closed (not safe while running).
  3. **Current TUI Behavior:** Adds Projects/Tags locally in-session when `Route not found` occurs.

## 2. Short Syntax & Floating Completions (`src/short_syntax.rs`, `src/app.rs`, `src/ui.rs`)
- **`+` Project:** Floating project completion list; prefix/case-insensitive match (spaces stripped, shortest match preferred).
- **`#` Tag:** Floating tag completion list; case-insensitive match.
- **`t` Time:** Compound durations (`1h5m3s`, `2d`, `1.5h`, `30m/1h`) with floating `t` duration presets.
- **`@` Due & `!` Deadline:** Floating presets (`today`, `tomorrow`, `friday`, `next_week`, `2d`, `1h30m`, `4pm`) and date/duration resolution.

## 3. Worklog / History View (`:w` / `H` / Sidebar `History`)
- Summary bar (completed/total, total spent, total estimate).
- 3 tabs (`[1] Completed`, `[2] Projects`, `[3] Tags`) with Project & Tag colors and per-entity completion/time breakdowns.
- Timeframe filter (`f`/`Space`: `All Time`, `Today`, `This Week`) and `j`/`k`/`g` scrolling.

## Verification
- `cargo clippy --all-targets --all-features -- -D warnings`: passed
- `cargo test --all-features`: 12 passed, 0 failed
