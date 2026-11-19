# Feature & Fix Report: Undo 100, Redo, `jk`, `l`, Showcmd, Timer

## Changes
- **Undo Cap (`src/app.rs`):** Capped `undo_stack` at 100 operations.
- **Redo Fix (`src/app.rs`):** Supported `Ctrl+r` (`\u{12}` fallback) and `U` across `Normal`/`Both`, and updated `DeleteTask` ID on restore so subsequent redo deletes the recreated task.
- **Properties `jk` (`src/app.rs`, `src/ui.rs`):** Added `prop_jk_tracker` so `jk` closes Properties as an alternative to `Esc`.
- **Main List `l` (`src/app.rs`, `src/keybinds.rs`):** Mapped `l` in main task list to toggle done (NVIM/Both).
- **Status Bar (`src/ui.rs`, `src/app.rs`):** Removed `[Both]` badge, added Vim-style `showcmd` (`typed_key_display`).
- **Time Display & Active Timer (`src/models.rs`, `src/api.rs`, `src/ui.rs`, `src/app.rs`):** Displayed `timeSpent / timeEstimate` on tasks and added `t` to start/stop `/task-control/current` with live status bar indicator.

## Verification
- `cargo clippy --all-targets --all-features -- -D warnings`: passed
- `cargo test --all-features`: 9 passed, 0 failed
