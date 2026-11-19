# Feature Report: Counters, Pomodoro, Reschedule, Color Picker

## Implemented
- **Counters & Simple Habits (`:n` / `C` / Sidebar `Counters`):** Reads `/counters` (with local persistence fallback in `config.toml`), supports `+/-`, reset (`r`), and adding habits (`a`).
- **Pomodoro / Focus Mode (`:p` / `P`):** Countdown timer (`Focus` 25m, `ShortBreak` 5m, `LongBreak` 15m), synced with `/task-control/current`, live status bar badge, session counter, and break reminders.
- **Reschedule Date Picker (`R` direct or in Properties):** Interactive monthly calendar (`hjkl` days, `[/]` months) + optional `remindAt` (`HH:MM`) input, synced via `PATCH /tasks/:id` with undo support.
- **2D Color Picker in `:c` (`src/theme.rs`, `src/app.rs`, `src/ui.rs`):** Side-by-side 2D RGB/HSV map (`24x8`), 3 vertically stacked `R` / `G` / `B` inputs, and `HEX` input with live two-way sync and live UI preview.

## Verification
- `cargo clippy --all-targets --all-features -- -D warnings`: passed
- `cargo test --all-features`: 13 passed, 0 failed
