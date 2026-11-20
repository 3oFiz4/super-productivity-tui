# Report 016: Vim Visual / Multi-Select Mode (`v` / `V`)

## Changes
- **Visual & Multi-Select State (`src/app.rs`)**:
  - Added `visual_mode`, `visual_anchor`, and `selected_task_ids` to `App`, plus `PropertiesTarget::Batch(Vec<String>)`.
  - `v` enters visual mode and toggles individual tasks in/out of the selection.
  - `V` enters line-wise visual range mode; moving with `j`/`k`/`Up`/`Down`/`gg`/`G` dynamically updates the contiguous selection range.
  - `Esc` exits visual mode and clears the selection.
- **Batch Actions & Dedicated Batch Properties (`src/app.rs`, `src/ui.rs`)**:
  - `Space`: batch-complete (or reopen if all selected are done) and exit visual mode.
  - `d`: batch-delete all selected tasks (with undo entries) and exit visual mode.
  - `O` / `L` / `p`: opens **Batch Properties** (`N task(s) selected`), intentionally excluding single-item-only actions like `rename`, `add subtask`, `duplicate`, and `move to top/bottom`, while supporting:
    - `c`: complete / toggle
    - `sd` / `sm` / `sn` / `u`: schedule Today / Tomorrow / Next Week / Unschedule
    - `mp`: Move to Project
    - `d`: delete
- **Visual Indicators (`src/ui.rs`)**:
  - Status bar displays `[VISUAL (N selected)]` pill while in visual mode.
  - Multi-selected task rows show a `▪` marker and accent highlight.
- **Tests (`src/app.rs`)**:
  - Added `visual_mode_selects_range_and_batch_completes_and_moves_project`.

## Verification
- `cargo fmt --all` — passed
- `cargo clippy --all-targets --all-features -- -D warnings` — passed (0 warnings)
- `cargo test --all-features` — 19 passed, 0 failed
