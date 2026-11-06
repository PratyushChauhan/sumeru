/**
 * Inputs: API key `{ mode, mcp_ids }`, mcp id.
 * Outputs: true when the key may reach the MCP (mirrors Rust `ApiKey::allows`).
 */
export const keyAllows = (key, id) =>
  (key?.mcp_ids ?? []).includes(id) === (key?.mode === "include");

/**
 * Inputs: mode ("include" | "exclude"), ids switched on (reachable), all servers.
 * Outputs: `mcp_ids` to store for that mode.
 */
export const scopeIds = (mode, allowed, servers) =>
  servers.map((s) => s.id).filter((id) => allowed.includes(id) === (mode === "include"));

/**
 * Inputs: API key, servers. Outputs: short scope summary for the keys list.
 */
export function scopeLabel(key, servers) {
  const n = servers.filter((s) => keyAllows(key, s.id)).length;
  if (key.mode === "include") return n ? `Only ${n} of ${servers.length} MCPs` : "No MCPs";
  return n === servers.length ? "All MCPs" : `All except ${servers.length - n}`;
}

/**
 * Inputs: API key. Outputs: MCP server name agents install it under
 * (`sumeru` for Default, `sumeru-<slug>` otherwise, so several keys fit in one agent).
 */
export function agentServerName(key) {
  if (!key || key.id === "default") return "sumeru";
  const slug = key.name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
  return `sumeru-${slug || key.id.slice(0, 8)}`;
}
