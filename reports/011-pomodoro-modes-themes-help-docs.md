# Report 011: Focus Modes, Themes, Sync, Help Scroll, Add-Task Tip & Docs

## Summary

- **Focus Timer (`FocusMode`)**: Extended `PomodoroState` in `src/app.rs` and `src/ui.rs` with 3 modes selectable via `m`/`Tab`:
  - `Pomodoro`: 25m focus / 5m short break / 15m long break sprints.
  - `Flowtime`: count-up `elapsed_secs` focus timer with `elapsed / 5` earned break (`n` to switch, min 60s).
  - `Countdown`: configurable countdown (`5`–`180` mins, adjusted via `+`/`-`/`h`/`l`) that stops with a completion reminder at `00:00`.
  - Status bar badge now displays active mode, phase, and elapsed/remaining `MM:SS`.
- **Theme Presets (`src/theme.rs`)**: Added 16 new built-in themes (21 presets total), including `AMOLED` (`#000000` background, `#ffffff` foreground), `Dracula`, `One Dark`, `Solarized Dark`, `Solarized Light`, `Monokai Pro`, `Everforest Dark`, `Kanagawa`, `Rose Pine`, `Rose Pine Moon`, `Catppuccin Latte`, `GitHub Dark`, `Ayu Dark`, `Nightfox`, `Cyberpunk`, and `Material Ocean`.
- **Manual & Auto Sync (`src/app.rs`, `src/keybinds.rs`)**: Added `'S'` manual sync keybind alongside startup sync, post-mutation refresh, and a 30-second periodic background auto-sync when idle.
- **Sectioned & Scrollable `:h` Help (`src/keybinds.rs`, `src/app.rs`, `src/ui.rs`)**: Added `section` grouping (`--- Movement ---`, `--- Navigation ---`, `--- Tasks ---`, `--- Undo / Redo ---`, `--- Views & Modals ---`, `--- System ---`) plus `j`/`k`/`Up`/`Down`/`g`/`G` scrolling (`/` or `i` to focus search filter).
- **Add Task Tip (`src/ui.rs`)**: Added a short-syntax usage note in the Add Task modal (`+project`, `#tag`, `t1h5m3s`, `@due`, `!deadline`).
- **Documentation**: Created user-facing `README.md` and architecture-focused `README_DEV.md` with Mermaid flowcharts.

## Verification

- `cargo fmt --all`: passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed (0 warnings).
- `cargo test --all-features`: passed (13/13 unit tests passed).
