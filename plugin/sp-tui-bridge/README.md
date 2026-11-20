# Super Productivity TUI Bridge Plugin

## Why this plugin exists

Super Productivity's built-in **Local REST API** (`http://127.0.0.1:3876`) exposes read-only `GET /projects` and `GET /tags` endpoints, and does **not** implement `POST`, `PATCH`, or `DELETE` routes for Projects or Tags (all return `404 Route not found`).

However, Super Productivity's internal **Plugin API** (`PluginAPI.addProject`, `updateProject`, `deleteProject`, `addTag`, `updateTag`, `deleteTag`) *does* support full Project & Tag mutations.

This lightweight companion plugin bridges `spt` (listening on `http://127.0.0.1:3877`) to `PluginAPI` inside Super Productivity.

## Installation

1. Zip the contents of `plugin/sp-tui-bridge/` (`manifest.json` and `plugin.js`) into `sp-tui-bridge.zip`.
2. In Super Productivity, go to **Settings → Plugins → Upload Plugin** and select `sp-tui-bridge.zip`.
3. Enable the plugin. Whenever `spt` is running, creating/renaming/recoloring/deleting Projects and Tags in `spt` will automatically sync into Super Productivity!
