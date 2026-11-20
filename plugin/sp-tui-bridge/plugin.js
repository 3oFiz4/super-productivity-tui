/// <reference types="@super-productivity/plugin-api/global" />
(function () {
  const BRIDGE_URL = 'http://127.0.0.1:3877';
  const POLL_MS = 2000;
  let busy = false;

  async function ack(cmdId, ok, entityId, error) {
    try {
      await fetch(`${BRIDGE_URL}/commands/ack`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          cmd_id: cmdId,
          ok,
          entity_id: entityId || null,
          error: error || null,
        }),
      });
    } catch (_) {
      // spt closed mid-ack; ignore
    }
  }

  async function runCommand(cmd) {
    switch (cmd.action) {
      case 'create_project': {
        const id = await PluginAPI.addProject({
          title: cmd.title,
          theme: cmd.color ? { primary: cmd.color } : undefined,
        });
        return typeof id === 'string' ? id : id?.id || null;
      }
      case 'update_project': {
        const patch = {};
        if (cmd.title) patch.title = cmd.title;
        if (cmd.color) patch.theme = { primary: cmd.color };
        await PluginAPI.updateProject(cmd.id, patch);
        return cmd.id;
      }
      case 'delete_project': {
        if (typeof PluginAPI.deleteProject !== 'function') {
          throw new Error('SP PluginAPI.deleteProject is unavailable in this SP version');
        }
        await PluginAPI.deleteProject(cmd.id);
        return cmd.id;
      }
      case 'create_tag': {
        const id = await PluginAPI.addTag({
          title: cmd.title,
          color: cmd.color || '#a6e3a1',
        });
        return typeof id === 'string' ? id : id?.id || null;
      }
      case 'update_tag': {
        const patch = {};
        if (cmd.title) patch.title = cmd.title;
        if (cmd.color) patch.color = cmd.color;
        await PluginAPI.updateTag(cmd.id, patch);
        return cmd.id;
      }
      case 'delete_tag': {
        if (typeof PluginAPI.deleteTag !== 'function') {
          throw new Error('SP PluginAPI does not expose deleteTag (delete tag in SP UI)');
        }
        await PluginAPI.deleteTag(cmd.id);
        return cmd.id;
      }
      default:
        throw new Error(`Unknown action: ${cmd.action}`);
    }
  }

  async function poll() {
    if (busy) return;
    busy = true;
    try {
      const resp = await fetch(`${BRIDGE_URL}/commands`);
      if (!resp.ok) return;
      const commands = await resp.json();
      if (!Array.isArray(commands) || commands.length === 0) return;

      for (const cmd of commands) {
        try {
          const entityId = await runCommand(cmd);
          await ack(cmd.cmd_id, true, entityId, null);
        } catch (err) {
          await ack(cmd.cmd_id, false, null, String(err));
        }
      }
    } catch (_) {
      // spt is not running; silent
    } finally {
      busy = false;
    }
  }

  setInterval(poll, POLL_MS);
  poll();
})();
