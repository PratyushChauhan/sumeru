# Connect your agent

1. Start Sumeru and confirm the badge shows **running**
2. On **Configure → Connect an agent**, pick your agent

![Configure tab with running badge, endpoint URL, and bearer token](/images/configure.png)

3. Install it:

| Agent | How |
| --- | --- |
| Claude Code | **Copy** the `claude mcp add …` command and run it in a terminal |
| Codex | **Copy** the `[mcp_servers.sumeru]` table into `~/.codex/config.toml` |
| Cursor | Click **Add to Cursor** (or paste the snippet into `~/.cursor/mcp.json`) |
| VS Code | Click **Add to VS Code** (or paste the snippet into your user `mcp.json`) |
| Gemini CLI | **Copy** the `gemini mcp add …` command and run it in a terminal |
| Windsurf | **Copy** the snippet into `~/.codeium/windsurf/mcp_config.json` |

The preview masks the token unless **Show** is on; **Copy** and **Add to …** always use the real token.

4. Reload MCP servers in your agent and call `list_mcps` to confirm your upstreams appear

After you [rotate the token](./rotate-token.md), repeat these steps — agents keep the old token until you do.

## Stdio alternative

If you prefer stdio instead of HTTP:

```json
{
  "mcpServers": {
    "sumeru": {
      "command": "sumeru",
      "args": ["mcp-stdio"]
    }
  }
}
```

Install the CLI with `npm i -g sumeru-mcp` first (or point `SUMERU_BINARY` at a local build).
