# Super Productivity TUI

A crazy fast vim-supported terminal client for **Super Productivity**, made in Rust (ratatui) and connected to Super Productivity's Local REST API.

Manage your tasks, subtasks, projects, tags, deadlines, focus sessions, daily habits, worklogs, and terminal colorschemes without having to open super-productivity EVER...

---

## Features

- **Views & Organization**: `Inbox`, `Today`, `Planned`, `History`, `Counters`, `Projects`, and `Tags` in a collapsible navigation sidebar (`Space` / `Enter` / `l`).
- **Flexible Keybinds (`Both`, `NVIM`, `Normal`)**:
  - `hjkl` and arrow movement, `gg` / `G` jump to top/bottom, live `/` fuzzy task search.
  - Neovim-style `:` command palette (`:c` colorscheme, `:h` help, `:s` settings, `:p` focus timer, `:hist` worklog, `:cnt` counters, `:q` quit).
  - Vim-style pending command display in the status bar and `jk` alternative to `Esc` in modals/dropdowns.
- **Short Syntax Task Creation (`a`)**:
  - Type `+project`, `#tag`, `t1h5m3s` (or `t30m/1h`), `@tomorrow` / `@2026-12-01`, and `!friday` directly in the task title with floating live completion.
  - Pre-selects the currently viewed project automatically.
- **Inline Subtasks & Markdown Notes**:
  - Expand/collapse subtasks directly under parent tasks (`Tab` or `>`), navigate into subtasks with `hjkl`, toggle completion checkboxes (`Space`), and view Markdown-styled notes (full view with `Shift+N`).
- **Context-Aware Properties Dropdown (`o` / `l`)**:
  - Schedule shortcuts (`sd` Today, `sm` Tomorrow, `sn` Next Week, `u` Unschedule, `R` Interactive Calendar Date Picker).
  - Repeat presets, Move Top/Bottom (`mt`/`mb`), Move to Project (`mp`), Add Subtask (`as`), Duplicate (`d`), Delete (`x`).
  - Also works on Projects and Tags in the sidebar.
- **3-Mode Focus Timer (`:p` or `P`)**:
  - **Pomodoro**: 25m focus / 5m short break / 15m long break sprints.
  - **Flowtime**: Open-ended count-up timer that calculates an earned break (`elapsed / 5`, minimum 1m).
  - **Countdown**: Custom target countdown (`5m`–`180m`, adjustable with `+`/`-`).
- **100-Step Undo / Redo**: Press `u` to undo and `Ctrl+R` (or `U`/`r` in Normal mode) to redo up to 100 task mutations.
- **Worklog / History (`:hist`) & Simple Counters (`:cnt`)**: Inspect time spent by Day/Week/Month across Projects and Tags, and track daily counters/habits.
- **Live Theme Editor & 2D Color Picker (`:c`)**:
  - 21 built-in themes including `AMOLED`, `Catppuccin Mocha`, `Catppuccin Latte`, `Tokyo Night`, `Gruvbox Dark`, `Nord`, `Dracula`, `One Dark`, `Solarized Dark/Light`, `Monokai Pro`, `Everforest Dark`, `Kanagawa`, `Rose Pine`, `Rose Pine Moon`, `GitHub Dark`, `Ayu Dark`, `Nightfox`, `Cyberpunk`, `Material Ocean`, and `Default`.
  - Live non-dimmed preview, interactive 2D RGB map + HEX color editor, custom theme saving.
- **Auto-Sync & Manual Sync (`S`)**: Automatically syncs on startup, after API mutations, and every **30 seconds** in the background while idle; press `S` at any time for an immediate sync.

---

## Requirements

- [Rust toolchain](https://rustup.rs/) (`cargo`, `rustc` supporting Rust 2024 edition)
- [Super Productivity](https://super-productivity.com/) desktop app running locally with the **Local REST API** enabled (default: `http://127.0.0.1:3876`)
- A Nerd Font in your terminal emulator for crisp folder, tag, checkbox, and calendar icons (optional, degrades cleanly)

---

## Getting Started

### Build & Run

```bash
git clone https://github.com/your-org/Super-Productivity-TUI.git
cd Super-Productivity-TUI
cargo run --release
```

Or install the binary locally:

```bash
cargo install --path .
spt
```

### Configuration

1. Open Super Productivity and enable the **Local REST API** + copy your Bearer token.
2. Launch `spt` and press `:s` (or click Settings in the sidebar) to enter:
   - **API URL** (default: `http://127.0.0.1:3876`)
   - **API Token**
   - **Keybind Mode** (`Both`, `NVIM`, or `Normal`)
   - **Keybind Hinting** (underline/highlight mnemonic letters)
3. Configuration, saved themes, and local counter fallbacks are stored automatically at:
   - Linux/macOS: `~/.config/super-productivity-tui/config.toml`
   - Windows: `%APPDATA%\super-productivity-tui\config.toml`

---

## Keybind Quick Reference

Press `:h` inside the app for the searchable, sectioned keybind manual (`j`/`k`/`g`/`G` to scroll, `/` to search).

| Context            | Key                             | Action                                                                           |
| :----------------- | :------------------------------ | :------------------------------------------------------------------------------- |
| **Movement**       | `j` / `k` / `h` / `l` / Arrows  | Move cursor / switch panels / open dropdown                                      |
|                    | `gg` / `G`                      | Jump to top / bottom of current list                                             |
| **Tasks**          | `Space`                         | Toggle task or subtask completion                                                |
|                    | `a`                             | Add task (or add Project/Tag when sidebar is focused)                            |
|                    | `o` / `l`                       | Open context properties dropdown                                                 |
|                    | `O` / `L`                       | Quick actions (in sidebar: add Project/Tag; in tasks: details)                   |
|                    | `/`                             | Live fuzzy task search                                                           |
|                    | `Shift+N`                       | Toggle full task notes viewer                                                    |
|                    | `x` / `Delete`                  | Delete selected task                                                             |
| **Undo / Redo**    | `u`                             | Undo last task operation (up to 100 steps)                                       |
|                    | `Ctrl+R` (or `U`/`r` in Normal) | Redo undone task operation                                                       |
| **Views & Timers** | `:p` / `P`                      | Open Focus Timer (`m`/`Tab` mode, `+/-` mins, `Space` play/pause, `l` link task) |
|                    | `:hist`                         | Open Worklog / History view                                                      |
|                    | `:cnt`                          | Open Counters & Habits view                                                      |
|                    | `:c`                            | Open Theme / Colorscheme editor & color picker                                   |
| **System**         | `S`                             | Sync now with Super Productivity                                                 |
|                    | `:s`                            | Open Settings                                                                    |
|                    | `:h`                            | Open Help / Keybinds manual                                                      |
|                    | `q` / `:q`                      | Quit                                                                             |

---

## Short Syntax in Add Task (`a`)

When typing a task **Title**, you can embed inline tokens (with floating autocomplete):

- `+ProjectName` — Assign to a project
- `#TagName` — Attach one or more tags
- `t1h5m3s` — Log time spent (also supports estimates with `t30m/1h` or `2d`)
- `@today`, `@tomorrow`, `@mon`..`@sun`, `@YYYY-MM-DD` — Set scheduled due day
- `!today`, `!friday`, `!2026-12-31` — Set deadline date

---

## License

MIT
