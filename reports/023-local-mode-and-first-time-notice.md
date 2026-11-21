# Report 023: Standalone Local Mode & First-Time `[info]` Notice

## Changes
- **SP Executable Detection & Dev Override (`src/config.rs`)**:
  - Added `is_sp_installed()` checking standard Windows/Linux/macOS install locations and `PATH`.
  - Added `SPT_FORCE_NO_SP=1` (or `true`) developer env override to simulate Super Productivity not being installed.
  - Added `seen_local_notice: bool` to `AppConfig`.
- **TUI `ConnectionState::Local` & 10s Auto-Closing `[info]` Banner (`src/app.rs`, `src/ui.rs`)**:
  - Status bar shows `● Local` (in `theme.accent_color()` blue) when SP is not installed, switching to `● Offline` as soon as SP is installed and `● Online` when running.
  - On first launch in `Local` mode, displays a blue `[info]` banner with a live `(closing in 10s)` -> `9s` -> ... countdown that auto-closes after 10 seconds.
  - Added a Local mode note to the `:s` Settings modal.
- **CLI `Local` Status & First-Time `[info]` Notice (`src/cli.rs`)**:
  - Updated `ls`, `status`, and `doctor` to report `Local` instead of `Offline` when SP is not installed.
  - Prints a 1-cell `theme.accent_color()` left-bordered (`┃`) `[info]` notice on first CLI run in `Local` mode.
- **Docs (`README.md`)**:
  - Updated `## Features` only with Local Mode, Update Checker, and Desktop Notifications/Sound Alerts.

## Verification
- `cargo fmt --all` — passed
- `cargo clippy --all-targets --all-features -- -D warnings` — passed (0 warnings)
- `cargo test --all-features` — 33 passed, 0 failed
