# Feature Report: Themes, Undo/Redo, Properties, and Add Task

## Implemented
- **Theme Editor (`:c`):** Live interactive colorscheme editor (`src/theme.rs`, `src/ui.rs`) with 5 built-in presets (`Dark`, `Catppuccin Mocha`, `Gruvbox Dark`, `Tokyo Night`, `Nord`), live preview, and template saving (`Ctrl+S`) in `config.toml`.
- **Undo / Redo:** `u` for Undo; `Ctrl+r` (NVIM/Both) and `U` (Normal) for Redo across task done, rename, schedule, move, delete, and create.
- **Expanded Properties (`p` / `O` / Right-Click):**
  - Tasks: `rename`, `schedule today`, `schedule tomorow`, `schedule next week`, `unschedule`, `Reschedule` (placeholder), `move to top`, `move to bottom`, `add subtask`, `duplicate`, `move to project`.
  - Projects & Tags: `rename`, `delete`.
- **Add Task (`a`) Improvements:** Dedicated Project & Tag rows, green foreground (no green background) when typing, `Repeat` selector, and floating `#tag` / `@time` completion popup.
- **Visuals:** Entity colors from Super Productivity, removed `#` prefix on tags, calendar deadline icon (red past due, yellow today), green keybind-only coloring in `:h`.

## Verification
- `cargo clippy --all-targets --all-features -- -D warnings`: passed
- `cargo test --all-features`: 7 passed, 0 failed
