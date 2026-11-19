# Bugfix Report: Duplicate Key Events

## Cause
On Windows, `crossterm` emits both `KeyEventKind::Press` and `KeyEventKind::Release` for each keystroke, causing `App::handle_key` to execute twice.

## Fix
- Ignored `KeyEventKind::Release` at the entry of `App::handle_key` in `src/app.rs`.
- Added regression test `app::tests::release_key_events_are_ignored`.

## Verification
- `cargo clippy --all-targets --all-features -- -D warnings`: passed
- `cargo test --all-features`: 6 passed, 0 failed
