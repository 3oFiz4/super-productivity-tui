# Feature & Fix Report: Navigation, Subtasks, Markdown, Search, Dropdown

## Changes
- **Navigation (`src/app.rs`, `src/keybinds.rs`):** Added `gg` (top) and `G` (bottom) for both sidebar and main list; `l` (open/toggle) and `L` (properties) in sidebar and main list.
- **Properties Dropdown (`src/ui.rs`, `src/app.rs`):** Anchored Properties menu directly below the selected task/nav row; fixed `move to top` (`mt`), `move to bottom` (`mb`), and `move to project` (`mp`) underline hints.
- **Subtasks (`src/ui.rs`, `src/app.rs`):** Filtered top-level tasks; added `s` to expand/collapse subtasks rendered below parent as name-only (`• <title>`).
- **Markdown Notes (`src/ui.rs`, `src/app.rs`):** Added Markdown rendering in Note pane and `N` (`Shift+N`) for full-screen Markdown note modal.
- **Theme (`src/theme.rs`, `src/ui.rs`):** Added `sidebar_bg` and `status_bg` so all UI regions are customizable; removed background dimming when `:c` is open for live preview.
- **Create Projects/Tags (`src/api.rs`, `src/app.rs`, `src/ui.rs`):** Pressing `a` in sidebar under Projects or Tags opens Add Project/Tag modal with custom color.
- **Live Search (`/`):** Added `/` fuzzy search across task title, project, and tags.

## Verification
- `cargo clippy --all-targets --all-features -- -D warnings`: passed
- `cargo test --all-features`: 9 passed, 0 failed
