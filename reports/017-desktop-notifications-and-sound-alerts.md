# Report 017: Desktop Notifications & Sound Alerts

## Changes
- **Cross-Platform Notification & Audio Dispatcher (`src/notify.rs`, `src/main.rs`)**:
  - Added `notify::dispatch(desktop_enabled, sound_enabled, title, body)` running non-blocking on background threads (suppressed under `#[cfg(test)]`).
  - **Sound Alerts**: Terminal bell (`\x07`) + OS audio cue (`SystemSounds::Asterisk` via hidden PowerShell on Windows, `afplay` on macOS, `canberra-gtk-play` / `paplay` on Linux).
  - **Desktop Notifications**: WinRT Toast Notification on Windows (with XML escaping), `osascript` on macOS, and `notify-send` on Linux.
- **Configuration & Settings UI (`src/config.rs`, `src/app.rs`, `src/ui.rs`)**:
  - Added `desktop_notifications` and `sound_alerts` (both default `true`) to `AppConfig`, `SettingsField`, `SettingsState`, and the `:s` Settings modal.
- **Notification Triggers (`src/app.rs`, `src/models.rs`)**:
  - **Focus Mode**: Alerts on Countdown completion, Flowtime break completion, Pomodoro Focus completion, and Pomodoro Break completion.
  - **Scheduled Reminders**: Added `remind_at: Option<u64>` to `Task`; `check_scheduled_reminders()` fires once per task when `remind_at` is reached (and resets on reschedule via `R`).
  - **Due Dates & Deadlines**: `check_due_and_deadline_notifications()` summarizes overdue/today deadlines and tasks due today once per day on load/tick.
- **Tests (`src/notify.rs`, `src/app.rs`)**:
  - Added `escapes_xml_special_characters`, `due_deadline_and_reminder_notifications_fire_once`, and `settings_toggles_desktop_notifications_and_sound_alerts`.

## Verification
- `cargo fmt --all` — passed
- `cargo clippy --all-targets --all-features -- -D warnings` — passed (0 warnings)
- `cargo test --all-features` — 23 passed, 0 failed
