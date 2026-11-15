import { invoke } from "@tauri-apps/api/core";

/**
 * Inputs: command name (see `src-tauri/src/api.rs` `dispatch`), params object.
 * Outputs: the command's result (same contract the native macOS app uses via `sumeru engine`).
 */
const call = (method, params = null) => invoke("api", { method, params });

/**
 * Inputs: none. Outputs: funnel status `{ running, endpoint }`.
 */
export const getStatus = () => call("get_status");

/**
 * Inputs: none. Outputs: API keys `[{ id, name, mode, mcp_ids, token }]`.
 */
export const listKeys = () => call("list_keys");

/**
 * Inputs: `{ id?, name, mode, mcpIds }`. Outputs: saved key id.
 */
export const upsertKey = (args) => call("upsert_key", args);

/**
 * Inputs: key id. Outputs: void after deleting the key (not Default).
 */
export const removeKey = (id) => call("remove_key", { id });

/**
 * Inputs: key id. Outputs: the key's new token.
 */
export const rotateKey = (id) => call("rotate_key", { id });

/**
 * Inputs: none. Outputs: configured MCP server list.
 */
export const listServers = () => call("list_servers");

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
export const startFunnel = () => call("start_funnel");

/**
 * Inputs: none. Outputs: void after stopping the funnel.
 */
export const stopFunnel = () => call("stop_funnel");

/**
 * Inputs: server fields. Outputs: void after upsert.
 */
export const upsertServer = (args) => call("upsert_server", args);

/**
 * Inputs: server id. Outputs: void after delete.
 */
export const removeServer = (id) => call("remove_server", { id });

/**
 * Inputs: server id. Outputs: test result string.
 */
export const testServer = (id) => call("test_server", { id });

/**
 * Inputs: draft name, transport, secrets. Outputs: test result string.
 */
export const testDraft = (args) => call("test_draft", args);

/**
 * Inputs: url and optional server id. Outputs: OAuth probe result.
 */
export const probeMcpAuth = (args) => call("probe_mcp_auth", args);

/**
 * Inputs: url, id, optional client credentials. Outputs: void after OAuth.
 */
export const startMcpOauth = (args) => call("start_mcp_oauth", args);

/**
 * Inputs: http(s) URL or Cursor/VS Code deeplink. Outputs: void after opening it.
 */
export const openUrl = (url) => call("open_url", { url });

/**
 * Inputs: none. Outputs: local VitePress docs URL after starting the static server.
 */
export const openDocs = () => invoke("open_docs");

/**
 * Inputs: none. Outputs: absolute path to the sumeru binary for stdio MCP.
 */
export const mcpStdioCommand = () => call("mcp_stdio_command");
