# Setting up the `tdd-workflow` MCP server in your agent

The ready-to-run server entry lives in [config/mcp.json](../config/mcp.json).
Build the jar first (`mvn -q package`), then register the server with your
client of choice below. One thing to know before you copy:
`${workspaceFolder}` is a Cursor variable — every other client needs it
replaced with the **absolute path** to your repo clone (it appears twice).

The server itself is always the same command, whatever the client:

```bash
java -Dworkshop.root=/absolute/path/to/tdd-bdd-agentic \
     -jar /absolute/path/to/tdd-bdd-agentic/mcp-server/target/tdd-mcp-server.jar
```

---

## Cursor

Nothing to do — this repo ships `.cursor/mcp.json` with the same entry, and
Cursor picks it up automatically when you open the project. To register it
yourself in another project, copy `config/mcp.json` to `.cursor/mcp.json`
(project) or merge it into `~/.cursor/mcp.json` (global).

To find the MCP settings: open **Cursor Settings** (gear icon in the top
right, or `Cmd+Shift+J` on macOS / `Ctrl+Shift+J` on Windows/Linux), go to
**Customize**, then the **MCP** tab. Each configured server is listed there
with its status — `tdd-workflow` should show green, and toggling it off/on
restarts it.

- Docs: [Cursor — Model Context Protocol](https://cursor.com/docs/mcp)

## Claude Desktop

Open **Settings → Developer → Edit Config** and merge the `mcpServers` entry
from `config/mcp.json` into `claude_desktop_config.json`, replacing
`${workspaceFolder}` with your absolute repo path. Fully restart the app.

- Config file: `~/Library/Application Support/Claude/claude_desktop_config.json`
  (macOS) or `%APPDATA%\Claude\claude_desktop_config.json` (Windows)
- Docs: [MCP — Connect to local servers](https://modelcontextprotocol.io/docs/develop/connect-local-servers)

## Claude Code

One command from the repo root registers the server for this project:

```bash
claude mcp add tdd-workflow -- java -Dworkshop.root="$PWD" -jar "$PWD/mcp-server/target/tdd-mcp-server.jar"
```

Or create `.mcp.json` in the project root with the `mcpServers` block from
`config/mcp.json` (absolute paths). Verify with `/mcp` inside a session.

- Docs: [Claude Code — MCP](https://code.claude.com/docs/en/mcp)

## OpenAI Codex (CLI / IDE extension)

Codex uses TOML, not JSON. Add this to `~/.codex/config.toml` (or a trusted
project's `.codex/config.toml`):

```toml
[mcp_servers.tdd-workflow]
command = "java"
args = [
  "-Dworkshop.root=/absolute/path/to/tdd-bdd-agentic",
  "-jar",
  "/absolute/path/to/tdd-bdd-agentic/mcp-server/target/tdd-mcp-server.jar"
]
```

Or use the CLI: `codex mcp add tdd-workflow -- java ...` (same arguments).

- Docs: [Codex — Model Context Protocol](https://developers.openai.com/codex/mcp)

## VS Code (GitHub Copilot)

Create `.vscode/mcp.json` in the project. Note VS Code's top-level key is
`servers` (not `mcpServers`) and each server takes a `type`:

```json
{
  "servers": {
    "tdd-workflow": {
      "type": "stdio",
      "command": "java",
      "args": [
        "-Dworkshop.root=${workspaceFolder}",
        "-jar",
        "${workspaceFolder}/mcp-server/target/tdd-mcp-server.jar"
      ]
    }
  }
}
```

(VS Code supports `${workspaceFolder}` too, so this one works as-is.)

- Docs: [VS Code — Add and manage MCP servers](https://code.visualstudio.com/docs/agent-customization/mcp-servers)

## Windsurf

Open **Settings → Tools → Windsurf Settings → Add Server**, or edit
`~/.codeium/windsurf/mcp_config.json` directly (global only — Windsurf has no
per-project config). Merge the `mcpServers` block from `config/mcp.json` with
absolute paths, then press the refresh button in the MCP panel.

- Docs: [Windsurf — Model Context Protocol](https://docs.windsurf.com/plugins/cascade/mcp)

## Gemini CLI

From the repo root:

```bash
gemini mcp add tdd-workflow java -- -Dworkshop.root="$PWD" -jar "$PWD/mcp-server/target/tdd-mcp-server.jar"
```

Or add the `mcpServers` block from `config/mcp.json` (absolute paths) to
`.gemini/settings.json` (project) or `~/.gemini/settings.json` (user).

- Docs: [Gemini CLI — MCP servers](https://github.com/google-gemini/gemini-cli/blob/main/docs/tools/mcp-server.md)

---

Whichever client you use, success looks the same: the server shows as
connected/green and its seven tools appear — `list_requirements`,
`get_requirement`, `validate_spec`, `refine_requirement`, `run_tests`,
`get_tdd_state`, `start_refactor`. If it won't connect, the usual cause is a
missing jar: run `mvn -q package` and reload the server.
