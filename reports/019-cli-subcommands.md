# Report 019: Scriptable CLI Subcommands (`src/cli.rs`)

## Changes
- **CLI Entrypoint (`src/main.rs`, `src/cli.rs`)**:
  - Added early CLI dispatch in `main()` when arguments are passed (`spt <command>`), bypassing raw terminal mode for non-interactive scripting, cron jobs, and status bars.
  - Implemented subcommands with `--json` support and automatic offline cache/queue fallback:
    - **Tasks**: `add` (with `parse_short_syntax`, `--top`, `--note`, `--parent`), `ls` (`all`/`today`/`inbox`/`planned`/`overdue`, `--project`, `--tag`, `--search`, `--done`, `--limit`), `done` / `undo-done`, `edit`, `schedule` (`today`/`tomorrow`/`next-week`/`YYYY-MM-DD`/`clear`, `--remind HH:MM`), `rm`.
    - **Time & Status Bars**: `start`, `stop`, `log <id|query> <duration>`, `status` (`--waybar`, `--tmux`, `--json`), `current`.
    - **Entities, Reports & Diagnostics**: `projects`, `tags`, `counter` (`list`/`inc`/`dec`/`set`), `summary`, `worklog` (`--day`/`--week`/`--month`, `--by project|tag`), `sync`, `doctor`, `--help`, `--version`.
- **Tests (`src/cli.rs`)**:
  - Added `resolves_task_by_id_prefix_or_title`, `parses_schedule_when_keywords_and_dates`, and `positional_args_skips_value_flags`.
- **Docs (`README.md`)**:
  - Added a **CLI Subcommands** usage section with task, timer, and Waybar/tmux examples.

## Verification
- `cargo fmt --all` — passed
- `cargo clippy --all-targets --all-features -- -D warnings` — passed (0 warnings)
- `cargo test --all-features` — 27 passed, 0 failed
