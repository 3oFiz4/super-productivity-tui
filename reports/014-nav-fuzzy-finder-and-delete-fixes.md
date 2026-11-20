# Report 014: Navigation Fuzzy Finder (`/` + `n`/`N`) & Project/Tag Deletion Fixes

## Changes
1. **Navigation Fuzzy Finder (`src/app.rs`, `src/ui.rs`)**:
   - Pressing `/` while focused on the Navigation sidebar (`FocusSection::Nav`) opens a live fuzzy search bar at the top of the sidebar (`nav_search_active` / `nav_search_query`).
   - Matching navigation groups/projects/tags are highlighted in the sidebar and jumped to live as you type.
   - Pressing `Enter` keeps the highlighted matches active; pressing `n` / `N` (only under `FocusSection::Nav`) cycles forward/backward through highlighted matches. `Esc` clears the search.
2. **Project & Tag Deletion Fixes (`src/app.rs`, `plugin/sp-tui-bridge/`)**:
   - Added `handle_delete_shortcut()`, `delete_project_by_id()`, and `delete_tag_by_id()` so pressing `d` directly in the sidebar deletes the selected Project or Tag (in addition to `O` / `L` / `p` → `d` in Properties).
   - Added `"deleteProject"` permission to `plugin/sp-tui-bridge/manifest.json` (`v1.0.1`) and updated `plugin/sp-tui-bridge.zip`.
   - Updated `plugin.js` to explicitly report an error for `delete_tag` because Super Productivity's `@super-productivity/plugin-api` does not expose `deleteTag`.

## Verification
- `cargo fmt --all` — passed
- `cargo clippy --all-targets --all-features -- -D warnings` — passed (0 warnings)
- `cargo test --all-features` — 17 passed, 0 failed
