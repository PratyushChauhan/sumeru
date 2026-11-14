# Endpoint

- **URL:** `http://127.0.0.1:7341/mcp`
- **Auth:** `Authorization: Bearer <token>` — one token per API key (**Configure → Keys**); each key reaches only its MCPs ([Scoped API keys](./cookbook/scoped-keys.md))
- Browser `Origin` requests are rejected
- Transport: stateless Streamable HTTP with JSON POSTs; GET `/mcp` returns a keep-alive SSE stream

## Example client config

**Configure → Connect an agent** generates this for Claude Code, Codex, Cursor, VS Code, Gemini CLI, and Windsurf (one-click for Cursor and VS Code). See [Connect your agent](./cookbook/connect-agent.md).

```json
{
  "mcpServers": {
    "sumeru": {
      "url": "http://127.0.0.1:7341/mcp",
      "headers": {
        "Authorization": "Bearer <token>"
      }
    }
  }
}
```

## Gateway tools

Sumeru exposes exactly three MCP tools:

| Tool | Inputs | Outputs |
| --- | --- | --- |
| `list_mcps` | none | configured MCP ids, names, transports, enabled flags |
| `list_mcp_tools` | `mcp_id` | upstream tool names, descriptions, schemas |
| `execute_mcp_tool` | `mcp_id`, `tool_name`, `arguments?` | upstream `CallToolResult` |

All three are scoped by the request's API key: MCPs outside it are not listed and return `unknown mcp_id`.

## Lifecycle

- Upstream clients connect lazily on first `list_mcp_tools` / `execute_mcp_tool`
- Connections are reused until Sumeru quits, the MCP is edited/deleted, or the transport closes
- Tool execution is never auto-retried after an ambiguous failure

## Storage and security

- Config: app config dir `/sumeru/servers.json`
- Secrets: OS keychain service `sumeru` (Default key token: `endpoint-token`; other keys: `key:<id>`)
- Funnel binds only to `127.0.0.1`
- Stdio commands are argv-based (no shell strings)
- Upstream tool metadata/output is untrusted data
