# Report 015: Daily Summary & End-of-Day Review (`:r` / `E`)

## Changes
- **Daily Summary Modal (`src/app.rs`, `src/ui.rs`)**:
  - Added `ActiveModal::DailySummary` and `DailySummaryState`, accessible via `:r` or `Shift+E` (`E`).
  - Displays today's completed task count, total focus time (`time_spent`), Pomodoro completed sessions, and remaining unfinished tasks.
  - Lists completed tasks for today and provides an interactive **Carry-Over Review** list for unfinished today tasks:
    - `j` / `k` (or Up/Down): select unfinished task
    - `t` / `Enter`: carry over to Tomorrow
    - `w`: carry over to Next Week
    - `u`: unschedule
    - `Space`: mark done
    - `A`: carry over all remaining tasks to Tomorrow
    - `Esc` / `q`: close modal
- **Tests (`src/app.rs`, `src/ui.rs`)**:
  - Added `ActiveModal::DailySummary` to `renders_main_and_modals_without_panic`.
  - Added `daily_summary_shows_totals_and_carries_over_unfinished`.

## Verification
- `cargo fmt --all` — passed
- `cargo clippy --all-targets --all-features -- -D warnings` — passed (0 warnings)
- `cargo test --all-features` — 18 passed, 0 failed
