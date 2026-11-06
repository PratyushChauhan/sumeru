/**
 * Build per-agent install recipes for the Sumeru endpoint.
 *
 * Inputs: endpoint URL, bearer token (or a placeholder for display), MCP server name
 * the agent installs it under (default `sumeru`; one per API key).
 * Outputs: `[{ id, name, hint, text, link? }]` — `text` is the command or
 * config snippet to copy; `link` is an optional one-click install deeplink.
 */
export function agentInstalls(url, token, server = "sumeru") {
  const auth = `Bearer ${token}`;
  const headers = { Authorization: auth };
  const json = (o) => JSON.stringify(o, null, 2);
  const cursor = { url, headers };
  const vscode = { type: "http", url, headers };
  return [
    {
      id: "claude-code",
      name: "Claude Code",
      hint: "Run in a terminal",
      text: `claude mcp remove --scope user ${server} 2>/dev/null; claude mcp add --transport http --scope user ${server} ${url} --header "Authorization: ${auth}"`,
    },
    {
      id: "codex",
      name: "Codex",
      hint: `Paste into ~/.codex/config.toml (replace any existing [mcp_servers.${server}])`,
      text: `[mcp_servers.${server}]\nurl = "${url}"\nhttp_headers = { Authorization = "${auth}" }`,
    },
    {
      id: "cursor",
      name: "Cursor",
      hint: "Click Add to Cursor, or paste into ~/.cursor/mcp.json",
      text: json({ mcpServers: { [server]: cursor } }),
      link: `cursor://anysphere.cursor-deeplink/mcp/install?name=${encodeURIComponent(server)}&config=${encodeURIComponent(btoa(JSON.stringify(cursor)))}`,
    },
    {
      id: "vscode",
      name: "VS Code",
      hint: "Click Add to VS Code, or paste into your user mcp.json",
      text: json({ servers: { [server]: vscode } }),
      link: `vscode:mcp/install?${encodeURIComponent(JSON.stringify({ name: server, ...vscode }))}`,
    },
    {
      id: "gemini",
      name: "Gemini CLI",
      hint: "Run in a terminal",
      text: `gemini mcp add --scope user --transport http ${server} ${url} --header "Authorization: ${auth}"`,
    },
    {
      id: "windsurf",
      name: "Windsurf",
      hint: "Paste into ~/.codeium/windsurf/mcp_config.json",
      text: json({ mcpServers: { [server]: { serverUrl: url, headers } } }),
    },
  ];
}
