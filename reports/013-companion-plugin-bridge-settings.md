# Report 013: Companion Plugin Bridge Settings & End-to-End Verification

## Changes
- **Settings UI (`src/app.rs`, `src/ui.rs`)**:
  - Added `Bridge Port` (`SettingsField::BridgePort`, default `3877`, `0` to disable) to `:s` Settings.
  - Added live `SP Plugin` status indicator (`● Active` / `○ Waiting (upload plugin/sp-tui-bridge.zip in SP)`).
- **Bridge Integration Test (`src/bridge.rs`)**:
  - Added `bridge_serves_commands_and_processes_ack` testing `GET /commands`, `POST /commands/ack`, `BridgeHandle::is_plugin_active()`, and `ApiEvent::BridgeAcked`.
- **Documentation (`README.md`)**:
  - Added step-by-step Companion Plugin installation instructions under **Configuration**.

## Verification
- `cargo fmt --all` — passed
- `cargo clippy --all-targets --all-features -- -D warnings` — passed (0 warnings)
- `cargo test --all-features` — 16 passed, 0 failed
