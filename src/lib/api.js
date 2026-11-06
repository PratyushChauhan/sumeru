import { invoke } from "@tauri-apps/api/core";

/**
 * Inputs: none. Outputs: funnel status `{ running, endpoint }`.
 */
export const getStatus = () => invoke("get_status");

/**
 * Inputs: none. Outputs: API keys `[{ id, name, mode, mcp_ids, token }]`.
 */
export const listKeys = () => invoke("list_keys");

/**
 * Inputs: `{ id?, name, mode, mcpIds }`. Outputs: saved key id.
 */
export const upsertKey = (args) => invoke("upsert_key", args);

/**
 * Inputs: key id. Outputs: void after deleting the key (not Default).
 */
export const removeKey = (id) => invoke("remove_key", { id });

/**
 * Inputs: key id. Outputs: the key's new token.
 */
export const rotateKey = (id) => invoke("rotate_key", { id });

/**
 * Inputs: none. Outputs: configured MCP server list.
 */
export const listServers = () => invoke("list_servers");

/**
 * Inputs: none. Outputs: whether autostart is enabled.
 */
export const getAutostart = () => invoke("get_autostart");

/**
 * Inputs: enabled flag. Outputs: void.
 */
export const setAutostart = (enabled) => invoke("set_autostart", { enabled });

/**
 * Inputs: none. Outputs: void after starting the funnel.
 */
export const startFunnel = () => invoke("start_funnel");

/**
 * Inputs: none. Outputs: void after stopping the funnel.
 */
export const stopFunnel = () => invoke("stop_funnel");

/**
 * Inputs: server fields. Outputs: void after upsert.
 */
export const upsertServer = (args) => invoke("upsert_server", args);

/**
 * Inputs: server id. Outputs: void after delete.
 */
export const removeServer = (id) => invoke("remove_server", { id });

/**
 * Inputs: server id. Outputs: test result string.
 */
export const testServer = (id) => invoke("test_server", { id });

/**
 * Inputs: draft name, transport, secrets. Outputs: test result string.
 */
export const testDraft = (args) => invoke("test_draft", args);

/**
 * Inputs: url and optional server id. Outputs: OAuth probe result.
 */
export const probeMcpAuth = (args) => invoke("probe_mcp_auth", args);

/**
 * Inputs: url, id, optional client credentials. Outputs: void after OAuth.
 */
export const startMcpOauth = (args) => invoke("start_mcp_oauth", args);

/**
 * Inputs: http(s) URL or Cursor/VS Code deeplink. Outputs: void after opening it.
 */
export const openUrl = (url) => invoke("open_url", { url });

/**
 * Inputs: none. Outputs: local VitePress docs URL after starting the static server.
 */
export const openDocs = () => invoke("open_docs");

/**
 * Inputs: none. Outputs: absolute path to the sumeru binary for stdio MCP.
 */
export const mcpStdioCommand = () => invoke("mcp_stdio_command");
