# Developer Documentation (`README_DEV.md`)

Technical architecture, module map, data flows, and contributor notes for **Super Productivity TUI**.

---

## Architecture Overview

Super Productivity TUI follows an asynchronous **Elm / TEA-inspired (Model–View–Update)** architecture driven by `tokio::select!`:

- **State & Update**: [`src/app.rs`](src/app.rs) owns the single source of truth (`App`) and dispatches input events (`handle_key`, `handle_mouse`), timer ticks (`tick`), and async API results (`handle_api_event`).
- **View**: [`src/ui.rs`](src/ui.rs) performs pure immediate-mode rendering against `&App` via `ratatui::Frame`.
- **Side Effects**: [`src/api.rs`](src/api.rs) wraps `reqwest::Client` calls on spawned Tokio tasks and returns results through an unbounded `mpsc` channel (`ApiEvent`).

```mermaid
flowchart TD
    T["Terminal (crossterm EventStream)"] -->|"Event::Key / Event::Mouse"| L["main.rs tokio::select! Loop"]
    TK["50ms Interval Ticker"] -->|"tick()"| L
    CH["mpsc::UnboundedReceiver (ApiEvent)"] -->|"Loaded / Mutated / Error"| L

    L -->|"handle_key / handle_mouse / tick / handle_api_event"| APP["App State (src/app.rs)"]
    APP -->|"tokio::spawn + SuperProductivityClient"| API["Super Productivity Local REST API (127.0.0.1:3876)"]
    API -->|"ApiEvent via UnboundedSender"| CH
    APP -->|"ui::render(frame, &mut app)"| UI["Ratatui Frame Renderer (src/ui.rs)"]
    UI --> T
```

---

## Module Structure

| File | Responsibility |
| :--- | :--- |
| [`src/main.rs`](src/main.rs) | Raw terminal setup/teardown, `tokio::select!` event loop (50ms tick, `EventStream`, `api_rx`). |
| [`src/app.rs`](src/app.rs) | `App` state, modal routing, key/mouse handlers, 100-entry undo/redo stack, 30s auto-sync timer, `PomodoroState` (`Pomodoro`, `Flowtime`, `Countdown`), Worklog & Reschedule state. |
| [`src/ui.rs`](src/ui.rs) | Layout splits (sidebar, task list, inspector/details, status bar) and overlays (Add Task, Properties dropdown, Reschedule calendar, Focus Timer, History, Counters, Theme Editor + 2D Color Picker, `:h` Help). |
| [`src/api.rs`](src/api.rs) | `SuperProductivityClient` HTTP methods (`health`, `list_projects`, `list_tags`, `list_tasks`, `create_task`, `update_task`, `delete_task`, `toggle_task_done`, `start_tracking`, `stop_tracking`, counters/entity fallbacks). |
| [`src/models.rs`](src/models.rs) | Serde models (`Task`, `Project`, `Tag`, `SimpleCounter`, `CreateTaskPayload`, `ApiResponse`, hex color helpers). |
| [`src/short_syntax.rs`](src/short_syntax.rs) | Token parser for `+project`, `#tag`, `t<duration>[/<estimate>]`, `@due`, `!deadline`, compound duration parser (`1h5m3s`, `2d`), and floating autocomplete extraction. |
| [`src/keybinds.rs`](src/keybinds.rs) | Sectioned `KeybindEntry` registry for `:h`, `KeybindSequenceBuffer` (`jk` to Esc, `gg` sequence), and mode filtering (`Both`, `NVIM`, `Normal`). |
| [`src/theme.rs`](src/theme.rs) | `ThemeConfig`, 21 built-in presets (including `AMOLED`), HSV/RGB 2D map math (`map_coords_to_rgb`, `rgb_to_map_coords`, `parse_rgb`, `format_hex`). |
| [`src/config.rs`](src/config.rs) | TOML persistence (`AppConfig::load`, `AppConfig::save`) under the OS config directory (`super-productivity-tui/config.toml`). |

---

## Key Workflows

### 1. Sync & Mutation Flow

Data stays synchronized without blocking the UI thread:

```mermaid
sequenceDiagram
    participant U as User / Ticker
    participant A as App (src/app.rs)
    participant W as Tokio Task + API Client
    participant S as Super Productivity (:3876)

    alt Manual Sync (S) or Startup or 30s Idle Timer
        U->>A: refresh_all()
        A->>W: spawn async fetch
        W->>S: GET /health, /projects, /tags, /tasks
        S-->>W: JSON ApiResponse
        W-->>A: ApiEvent::Loaded(...)
        A->>A: Update state & reset last_sync_at
    end

    alt Task Mutation (Done, Schedule, Delete, Undo/Redo)
        U->>A: Apply local mutation + push UndoEntry (cap 100)
        A->>W: spawn async PATCH/POST/DELETE
        W->>S: /tasks/...
        S-->>W: Result
        W-->>A: ApiEvent::Mutated
        A->>A: Trigger refresh_all() for reconciliation
    end
```

### 2. Focus Timer Modes (`FocusMode`)

`PomodoroState` accumulates 50ms ticks via fractional `tick_accum_ms` until 1000ms (1 second) passes:

```mermaid
flowchart LR
    T["tick() (20Hz)"] -->|"accum >= 1000ms"| SEC["1-second Timer Step"]
    SEC --> M{"FocusMode"}
    M -->|"Pomodoro"| P["Decrement remaining_secs (25m/5m/15m) -> auto phase alert"]
    M -->|"Flowtime (Focus)"| F["Increment elapsed_secs -> n switches to earned Break (elapsed/5, min 60s)"]
    M -->|"Countdown"| C["Decrement remaining_secs (5m-180m) -> stop & alert at 00:00"]
```

### 3. Input Routing & Key Sequences

1. **Windows Key Release Filtering**: `App::handle_key` ignores `KeyEventKind::Release` and `KeyEventKind::Repeat` events (except navigation repeats if needed) so Windows terminals do not double-trigger actions.
2. **Escape Sequence (`jk`)**: Inside text/dropdown/modal contexts configured for `jk`, `KeybindSequenceBuffer` holds `j` briefly so `j` + `k` maps to `Esc`, while any other key flushes `j` cleanly.
3. **Command Bar (`:`)**: Commands are parsed in `execute_command` (`:c`, `:h`, `:s`, `:p`, `:hist`, `:cnt`, `:q`).

---

## Known API Constraints

- **Project & Tag Creation (`Route not found`)**: Super Productivity's Local REST API exposes `GET /projects` and `GET /tags` along with full `GET/POST/PATCH/DELETE /tasks`, but does not currently expose `POST /projects` or `POST /tags`. When `POST` fails with a route error, `App` creates the Project/Tag in the local session so tasks can still be tagged and grouped immediately.
- **Worklog / Counters Fallbacks**: Time-tracking totals (`timeSpent`) are aggregated directly from synced tasks (`Project` and `Tag` attribution), while `SimpleCounter` defaults and updates persist cleanly into `config.toml` when backend counter endpoints are unavailable.

---

## Development & Verification

Before committing any change, run the full check pipeline:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

### Commit & Report Conventions

- Keep commits atomic using [Conventional Commits](https://www.conventionalcommits.org/) (`feat:`, `fix:`, `docs:`, `refactor:`, `test:`).
- Record notable implementation/verification summaries as concise Markdown reports under `reports/`.
- Keep `chat_log/` local-only (gitignored).
