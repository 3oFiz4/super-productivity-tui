# Report 018: Inline Task Editing (`e` / `i`) with Short-Syntax Completion

## Changes
- **Inline Editing Keybind (`src/app.rs`, `src/keybinds.rs`, `README.md`)**:
  - Bound `e` (all modes in Main task list) alongside `i` (NVIM mode) and `r` (in Properties) to `start_rename_selected_task()`.
  - Updated `:h` keybinds and `README.md` to document `e / i` inline editing with short syntax.
- **Floating Completion & Shared Short-Syntax Engine (`src/app.rs`, `src/ui.rs`)**:
  - Extracted `compute_completion_for_text()` and `completion_prefix_char()` on `App` and `render_completion_popup()` in `src/ui.rs`, shared between Add Task (`a`) and Inline Task Editing (`e` / `i`).
  - Typing `+` (projects), `#` (tags), `t` (time spent/estimate), `@` (due date), or `!` (deadline) during inline task editing opens the floating completion popup directly below (or above) the edited task row (`Up`/`Down` navigate, `Tab` accepts, `Esc` dismisses).
- **Short-Syntax Patch on Commit (`src/app.rs`)**:
  - `commit_rename()` for `RenameTarget::Task` now runs `parse_short_syntax()` on `rename_buffer` and updates `title` (preserving existing title if only tokens were entered), `projectId`, `tagIds`, `dueDay`, `deadlineDay`, `timeSpent`, and `timeEstimate` via `client.update_task()` (with offline queue fallback).
- **Tests (`src/app.rs`)**:
  - Added `inline_edit_with_e_and_short_syntax_updates_title_estimate_due_and_tags`.

## Verification
- `cargo fmt --all` — passed
- `cargo clippy --all-targets --all-features -- -D warnings` — passed (0 warnings)
- `cargo test --all-features` — 24 passed, 0 failed
