# Report 021: CLI ID Conflict Rejection, Multi-ID Selection & Error/Warning Banners

## Changes
- **ID Conflict Rejection (`src/cli.rs`)**:
  - Updated `resolve_task()` so both dot-pattern matches (`AB.`) and prefix matches (`AB`) reject when matching multiple tasks (`AB1`, `AB2`) with an explicit `ID conflict for '...'` error listing the conflicting IDs.
- **Multi-ID Visual-Like Support (`src/cli.rs`)**:
  - Added `resolve_tasks()` supporting comma-separated (`AB1,AB2`) and multi-argument (`AB1 AB2`) IDs/dot-patterns with deduplication and atomic pre-validation.
  - Enabled multi-ID targeting in `done` / `undo-done`, `edit` (excluding title changes when targeting >1 task, requiring at least one metadata token), `schedule`, and `rm`.
- **Standardized Error & Warning Banners (`src/main.rs`, `src/cli.rs`)**:
  - All CLI errors print in bold red (`theme.danger_color()`) prefixed with `!!! [ERROR] !!!` via `cli::print_error()`.
  - Warnings (typo auto-correction, unknown view fallback, ignored title in multi-ID edit, ignored `--remind` with `clear`) print in bold warning color (`theme.warning_color()`) prefixed with `??? [WARN] ???`.
- **Tests (`src/cli.rs`)**:
  - Added `id_conflict_rejects_ambiguous_dot_pattern_and_prefix` and `resolves_multiple_comma_and_space_ids`.

## Verification
- `cargo fmt --all` — passed
- `cargo clippy --all-targets --all-features -- -D warnings` — passed (0 warnings)
- `cargo test --all-features` — 30 passed, 0 failed
