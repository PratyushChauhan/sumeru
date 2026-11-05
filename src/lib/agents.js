/**
 * Build per-agent install recipes for the Sumeru endpoint.
 *
 * Inputs: endpoint URL, bearer token (or a placeholder for display).
 * Outputs: `[{ id, name, hint, text, link? }]` — `text` is the command or
 * config snippet to copy; `link` is an optional one-click install deeplink.
 */
export function agentInstalls(url, token) {
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
      text: `claude mcp add --transport http --scope user sumeru ${url} --header "Authorization: ${auth}"`,
    },
    {
      id: "codex",
      name: "Codex",
      hint: "Paste into ~/.codex/config.toml (replace any existing [mcp_servers.sumeru])",
      text: `[mcp_servers.sumeru]\nurl = "${url}"\nhttp_headers = { Authorization = "${auth}" }`,
    },
    {
      id: "cursor",
      name: "Cursor",
      hint: "Click Add to Cursor, or paste into ~/.cursor/mcp.json",
      text: json({ mcpServers: { sumeru: cursor } }),
      link: `cursor://anysphere.cursor-deeplink/mcp/install?name=sumeru&config=${encodeURIComponent(btoa(JSON.stringify(cursor)))}`,
    },
    {
      id: "vscode",
      name: "VS Code",
      hint: "Click Add to VS Code, or paste into your user mcp.json",
      text: json({ servers: { sumeru: vscode } }),
      link: `vscode:mcp/install?${encodeURIComponent(JSON.stringify({ name: "sumeru", ...vscode }))}`,
    },
    {
      id: "gemini",
      name: "Gemini CLI",
      hint: "Run in a terminal",
      text: `gemini mcp add --scope user --transport http --header "Authorization: ${auth}" sumeru ${url}`,
    },
    {
      id: "windsurf",
      name: "Windsurf",
      hint: "Paste into ~/.codeium/windsurf/mcp_config.json",
      text: json({ mcpServers: { sumeru: { serverUrl: url, headers } } }),
    },
  ];
}
