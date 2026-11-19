# Feature & Fix Report: Scroll, Subtask Checkboxes, Markdown, Hints, Borders

## Changes
- **Subtasks (`src/app.rs`, `src/ui.rs`):** Added `visible_task_rows()` so expanded subtasks have their own checkboxes and are navigable/toggleable with `j`/`k`/`o`/`l`/`Enter`/`G`/`gg`.
- **Task List Scrolling (`src/app.rs`, `src/ui.rs`):** Added `task_scroll` tracking against `list_area.height` so `G` and `j`/`k` scroll cleanly without out-of-bounds selection.
- **Markdown Notes (`src/ui.rs`):** Added `parse_inline_md` supporting `**bold**`, `*italic*`, `` `code` ``, `#`/`##`/`###` headings, and bullets.
- **Configurable Hint Color & Borders (`src/theme.rs`, `src/app.rs`, `src/ui.rs`):** Added `hint_fg` to `ThemeConfig` and `:c` theme editor (height 21 so `Border` and `Hint Fg` fit), and colored underlined hint characters with `hint_fg_color()`.
- **Project/Tag Creation Fallback (`src/app.rs`):** Handled `Route not found` from Super Productivity Local REST API by adding the Project/Tag locally with a status notice.

## Verification
- `cargo clippy --all-targets --all-features -- -D warnings`: passed
- `cargo test --all-features`: 9 passed, 0 failed
