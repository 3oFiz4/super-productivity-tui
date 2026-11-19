# Fix Report: Schedule Hints, Undo Duplicate, Help Rows

## Changes
- **Schedule Hints (`src/ui.rs`):** Added `hint_indices` so `schedule today` underlines `s`/`d` (`sd`) and `schedule tommorow` underlines `s`/`m` (`sm`).
- **Undo Duplicate Task (`src/app.rs`):** Removed premature local `tasks.insert` on `DeleteTask` undo and `CreateTask` redo so only the API-returned task is inserted, with `record_undo = false`.
- **Help Rows (`src/keybinds.rs`, `src/ui.rs`):** Split combined `Normal / NVIM` keybind strings into separate rows with the same description.

## Verification
- `cargo clippy --all-targets --all-features -- -D warnings`: passed
- `cargo test --all-features`: 7 passed, 0 failed
