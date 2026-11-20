# Super Productivity TUI

https://github.com/user-attachments/assets/220b082d-f626-4e31-a14c-3818a0de2199

<details>
  <summary>Thumbnails (PNG)</summary>
  <img width="784" height="698" alt="image" src="https://github.com/user-attachments/assets/f679b4a8-16d2-4230-a913-c360776fee22" />
  <img width="784" height="698" alt="image" src="https://github.com/user-attachments/assets/699e8888-96a5-48b3-94b7-ecf01de84536" />
  <img width="784" height="698" alt="image" src="https://github.com/user-attachments/assets/766e0f35-06c3-4cb3-9047-fdc292a3651d" />
  <img width="784" height="698" alt="image" src="https://github.com/user-attachments/assets/c01812e8-16cf-4207-90be-2228e47351cc" />
  <img width="784" height="698" alt="image" src="https://github.com/user-attachments/assets/cb474b4e-abc5-4ae1-b922-95fa9d135785" />
  <img width="784" height="698" alt="image" src="https://github.com/user-attachments/assets/547ce18b-5a0b-462e-ab99-c45e0586744f" />
  <img width="784" height="698" alt="image" src="https://github.com/user-attachments/assets/79f1317e-d207-429e-b631-98551b61dff2" />

</details>




A blazing fast vim-supported terminal client for **Super Productivity (SP)**, made in Rust (ratatui) and connected to Super Productivity's Local REST API.

Freely manage your tasks, subtasks, projects, tags, deadlines, focus sessions, daily habits, worklogs, and terminal colorschemes without having to open super-productivity EVER... i mean it.

---

## Features

- **Views & Organization**: `Inbox`, `Today`, `Planned`, `History`, `Counters`, `Projects`, and `Tags` literally in a small collapsible navigation sidebar (`Space` / `Enter` / `l`).
- **Flexible Keybinds (`Both`, `NVIM`, `Normal`)**:
  - Incase you're not familiar with vim (which is sad but is ok), there's an alternative!
  - `hjkl` and arrow movement, `gg` / `G` jump to top/bottom, live `/` fuzzy task search.
  - Neovim-style `:` command palette (`:c` colorscheme, `:h` help, `:s` settings, `:p` focus timer, `:hist` worklog, `:cnt` counters, `:q` quit).
  - Vim-like pending command display in the status bar and `jk` alternative to `Esc` in modals/dropdowns.
- **Short Syntax Task Creation (`a`)**:
  - Just type `+project`, `#tag`, `t1h5m3s` (or `t30m/1h`), `@tomorrow` / `@2026-12-01`, and `!friday` directly in the task title with floating live completion.
  - It also pre-selects the currently viewed project automatically :-).
- **Inline Subtasks & Markdown Notes**:
  - Expand/collapse subtasks directly under parent tasks (`Tab` or `>`), navigate into subtasks with `hjkl`, toggle completion checkboxes (`Space`), and view Markdown-styled notes (full view with `Shift+N`).
- **Context-Aware Properties Dropdown (`o` / `l`)**:
  - Schedule shortcuts (`sd` Today, `sm` Tomorrow, `sn` Next Week, `u` Unschedule, `R` Interactive Calendar Date Picker). Mapping sounds weird eh? Until you decide to see it directly then you'll get the intuition later!
  - Repeat presets, Move Top/Bottom (`mt`/`mb`), Move to Project (`mp`), Add Subtask (`as`), Duplicate (`d`), Delete (`x`).
  - Also works on Projects and Tags in the sidebar.
- **3-Mode Focus Timer (`:p` or `P`)**:
  - **Pomodoro**: 25m focus / 5m short break / 15m long break sprints.
  - **Flowtime**: Open-ended count-up timer that calculates an earned break (`elapsed / 5`, minimum 1m).
  - **Countdown**: Custom target countdown (`5m`–`180m`, adjustable with `+`/`-`).
- **100-Step Undo / Redo**: Press `u` to undo and `Ctrl+R` (or `U`/`r` in Normal mode) to redo up to 100 task mutations. Now it's not really clear what mistake could happen up to 100 steps lol.
- **Worklog / History (`:hist`) & Simple Counters (`:cnt`)**: Inspect time spent by Day/Week/Month across Projects and Tags, and track daily counters/habits.
- **Live Theme Editor & 2D Color Picker (`:c`)**:
  - 21 built-in themes including `AMOLED`, `Catppuccin Mocha`, `Catppuccin Latte`, `Tokyo Night`, `Gruvbox Dark`, `Nord`, `Dracula`, `One Dark`, `Solarized Dark/Light`, `Monokai Pro`, `Everforest Dark`, `Kanagawa`, `Rose Pine`, `Rose Pine Moon`, `GitHub Dark`, `Ayu Dark`, `Nightfox`, `Cyberpunk`, `Material Ocean`, and `Default`.
  - Live non-dimmed preview, interactive 2D RGB map + HEX color editor, custom theme saving.
- **Auto-Sync & Manual Sync (`S`)**: Automatically syncs on startup, after API mutations, and every **30 seconds** in the background while idle; press `S` at any time for an immediate sync, if the automatic sync fails :3.

---

## Incoming Features

Ranked from most prioritized to less. Want to suggest a feature? Create a Issue, and let me know!

1. **Offline Queue & Local Cache**: Queue task creations/updates locally when Super Productivity is closed and replay them automatically on reconnect.
2. **Active Time-Tracking Controls**: Start/pause/stop Super Productivity's live task timer directly from the task list with a ticking status-bar indicator.
3. **Daily Summary & End-of-Day Review**: Interactive wrap-up modal showing completed tasks, total focus time, and carry-over prompts for unfinished tasks.
4. **Vim Visual / Multi-Select Mode (`v` / `V`)**: Select multiple tasks to batch-complete, reschedule, move to a project, or delete in one action. Even SP didn't support this lol.
5. **Custom Keybind Remapping**: Override or bind any action to custom keys or leader-key chords in `config.toml`. Keybind too hard to follow? (sorry :3), make your own then.
6. **Natural Language Due & Repeat Parsing**: Support phrases like `every weekday`, `in 3 days`, `next month`, and `every 2 weeks` in short syntax.
7. **Inline Task Editing (`e` / `i`)**: Edit task titles, time estimates, and due dates inline without opening a full modal.
8. **External `$EDITOR` Notes**: Press a keybind to edit long Markdown task notes in `nvim`, `vim`, or `$EDITOR`.
9. **Task Attachments & Links**: View, add, and open URLs or local file attachments linked to a task.
10. **Saved Filters & Smart Views**: User-defined filter queries (e.g., `overdue`, `high-estimate`, `@work + !today`) pinned to the sidebar.
11. **Project & Tag Management**: Full CRUD/reordering/color editing for Projects and Tags once supported by the Local REST API.
12. **Split-Pane / Dual-Column Mode**: View two lists side-by-side (e.g., `Inbox` + `Today`) for drag-free keyboard triage.
13. **Desktop Notifications & Sound Alerts**: Optional OS notifications and terminal bell/audio cues when Pomodoro, Flowtime, or Countdown timers finish.
14. **Time-Blocking / Agenda Timeline**: Hourly day-planner view to slot scheduled tasks into time blocks.
15. **Habit Streaks & Counter History**: Multi-day streak tracking and sparkline charts for Simple Counters.
16. **CSV / JSON / Markdown Export**: Export Worklog and History reports directly to Markdown tables, CSV, or JSON.
17. **Mouse Drag & Click Support**: Clickable sidebar items, task checkboxes, and scroll-wheel support across all panels.
18. **CLI Subcommands (`spt add`, `spt ls`, `spt sync`)**: Scriptable non-interactive CLI mode for shell aliases, cron jobs, and status bars (Waybar/Polybar/tmux).
19. **Plugin / Webhook Hooks**: Run custom shell scripts on events like `task_completed` or `pomodoro_finished`.
20. **Multi-Instance Profiles**: Switch between multiple Super Productivity API endpoints/tokens (e.g., Work vs. Personal) from `:s` or CLI flags.
21. **Undo/Redo Persistence**: Persist the 100-step undo history across `spt` restarts.
22. **Custom Nerd Font / ASCII Icon Toggle**: Config option to switch cleanly between Nerd Font glyphs and pure ASCII/Unicode symbols.

## Bugs

1. `HTTP POST` for Project and Tag creation is not possible right now, fallback to local creation instead.

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

ORRRRRR just go to release and install it already as ready-to-use `.exe` :3:

### Configuration

1. Open Super Productivity and enable the **Local REST API** + copy your Bearer token.
2. Launch `spt` and press `:s` (or click Settings in the sidebar) to enter:
   - **API URL** (default: `http://127.0.0.1:3876`)
   - **API Token** (still not sure where? Please check ![#### Getting your token])
   - **Keybind Mode** (`Both`, `NVIM`, or `Normal`)
   - **Keybind Hinting** (underline/highlight mnemonic letters)
3. Configuration, saved themes, and local counter fallbacks are stored automatically at:
   - Linux/macOS: `~/.config/super-productivity-tui/config.toml`
   - Windows: `%APPDATA%\super-productivity-tui\config.toml`

#### Getting your Token

1. On Super productivity, head to Settings.
2. Go to `General`
3. Scroll down to `Misc Settings` section
4. Enable `Enable local REST API`
5. Copy your `Access Token`
6. Done :3.... don't leak it though.

#### Companion Plugin (Create / Edit / Delete Projects & Tags)

Super Productivity's Local REST API only exposes read-only `GET /projects` and `GET /tags`, so `spt` includes a lightweight companion plugin bridge (`127.0.0.1:3877`) that uses SP's internal `PluginAPI` to create, rename, recolor, and delete Projects and Tags directly inside Super Productivity:

1. Open Super Productivity and go to **Settings → Plugins**.
2. Click **Upload Plugin** and select [`plugin/sp-tui-bridge.zip`](plugin/sp-tui-bridge.zip).
3. Enable **Super Productivity TUI Bridge**.
4. In `spt`, press `:s` to confirm `SP Plugin: ● Active` (and optionally adjust **Bridge Port**, default `3877`, `0` to disable).

> **Note:** Even without the plugin installed, Projects and Tags created/edited in `spt` are saved locally in `cache.json` and persist across syncs and restarts.

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
