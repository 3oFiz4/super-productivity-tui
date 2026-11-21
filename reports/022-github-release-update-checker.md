# Report 022: GitHub Release Auto-Update Checker (`src/update.rs`)

## Changes
- **GitHub Releases Checker & Cache (`src/update.rs`, `src/config.rs`, `src/main.rs`)**:
  - Polls `https://api.github.com/repos/3oFiz4/super-productivity-tui/releases/latest` with a 6-hour local cache (`update_cache.json`) and semver comparison (`is_newer_version`).
  - Added `ignore_update_forever` and `ignored_update_version` to `AppConfig` (`config.toml`).
- **TUI Alert Banner & Changelog Modal (`src/app.rs`, `src/ui.rs`, `src/keybinds.rs`)**:
  - Displays a colored `[alert] new version <tag> released: click :vc see version changelog, :vi ignore until next release, or :vI ignore forever` banner above the status bar with `:vc`, `:vi`, `:vI` highlighted/underlined.
  - Added `ActiveModal::Changelog` (`:vc`) with Markdown release notes and `j`/`k`/`g`/`G` scrolling.
- **CLI Subcommands & Left-Bordered Alert (`src/cli.rs`)**:
  - Added `spt vc`, `spt vi`, and `spt vI`.
  - After normal CLI command output, prints a `theme.success_color()` 1-cell left-bordered (`┃`) `[alert]` reminder when a newer release is available.
- **Tests (`src/update.rs`, `src/ui.rs`)**:
  - Added `compares_semver_tags_correctly`, `respects_ignore_until_next_and_ignore_forever`, and `ActiveModal::Changelog` render coverage.

## Verification
- `cargo fmt --all` — passed
- `cargo clippy --all-targets --all-features -- -D warnings` — passed (0 warnings)
- `cargo test --all-features` — 32 passed, 0 failed
