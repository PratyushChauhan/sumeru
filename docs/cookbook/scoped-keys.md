# Scoped API keys

Give each agent or repo its own key that reaches only the MCPs it needs. Sumeru enforces the scope at the endpoint: `list_mcps` only lists reachable MCPs, and `list_mcp_tools` / `execute_mcp_tool` treat any other MCP as unknown.

1. On **Configure → Keys**, click **Add key** (or **Edit** an existing one)
2. Name it (e.g. `work repo`) — names are unique
3. Under **New MCPs**, choose what happens to MCPs you add later:
   - **Allowed** — every MCP except the ones you switch off (an exclude list)
   - **Blocked** — only the MCPs you switch on (an include list)
4. Switch each MCP on (reachable) or off, then **Save**
5. In **Connect an agent**, pick the key, then your agent, and install it as usual

Each key installs under its own server name (`sumeru` for Default, `sumeru-<key name>` for others), so one agent can hold several keys side by side.

## Default key

Every install has a **Default** key that reaches all MCPs. It keeps the original `endpoint-token` keychain entry, so agents configured before keys existed keep working. You can rescope it, but not delete it.

## Stdio

`sumeru mcp-stdio --key <name>` applies the same scope to a stdio session (default: the Default key). Stdio runs as your user with keychain access, so this narrows what the agent sees but is not a security boundary.

## Rotate or delete

**Rotate** issues a new token for that key only; **Delete** revokes it immediately. Agents using it need the new token — re-run **Connect an agent** for that key.
