# Feature & API Check Report

## Changes
- **Preselect Active Project/Tag (`src/app.rs`):** Added `default_add_task_state()` so pressing `a` while viewing a Project or Tag preselects that Project or Tag in the Add Task modal. Added unit test `add_task_preselects_active_project`.

## API Verification
- Checked Super Productivity's Local REST API (`local-rest-api-handler.service.ts` / `docs/wiki/3.01-API.md`):
  - Only `GET /projects` and `GET /tags` are exposed.
  - `POST /projects` and `POST /tags` do not exist in the Local REST API; projects and tags must be created in the Super Productivity desktop app (or via the internal Plugin API), then assigned to tasks via `projectId` / `tagIds`.

## Verification
- `cargo clippy --all-targets --all-features -- -D warnings`: passed
- `cargo test --all-features`: 10 passed, 0 failed
