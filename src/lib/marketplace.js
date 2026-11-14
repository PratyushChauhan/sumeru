import catalog from "./marketplace.json" with { type: "json" };

/**
 * Inputs: none. Outputs: marketplace catalog entries array.
 */
export function listMarketplaceEntries() {
  return catalog.entries ?? [];
}

/**
 * Inputs: catalog entry, configured servers. Outputs: HTTP servers on the entry URL (one per account).
 */
export function installedServers(entry, servers) {
  const url = entry?.url?.trim();
  if (!url) return [];
  return (servers || []).filter(
    (s) => s?.transport?.kind === "http" && s.transport.url === url,
  );
}

/**
 * Inputs: configured servers, candidate name. Outputs: true when a server already uses it (case-insensitive).
 */
export function nameTaken(servers, name) {
  const n = (name || "").trim().toLowerCase();
  return (servers || []).some((s) => s?.name?.trim().toLowerCase() === n);
}

/**
 * Inputs: catalog entry, configured servers. Outputs: first unused "<name> <n>" for another account.
 */
export function nextAccountName(entry, servers) {
  let n = installedServers(entry, servers).length + 1;
  while (nameTaken(servers, `${entry.name} ${n}`)) n++;
  return `${entry.name} ${n}`;
}

/**
 * Inputs: entries, search query. Outputs: entries matching name/description/url.
 */
export function filterMarketplaceEntries(entries, query) {
  const q = (query || "").trim().toLowerCase();
  if (!q) return entries || [];
  return (entries || []).filter((e) => {
    const hay = `${e.name || ""} ${e.description || ""} ${e.url || ""}`.toLowerCase();
    return hay.includes(q);
  });
}

/**
 * Inputs: catalog URL. Outputs: hostname for display, or the URL.
 */
export function marketplaceHost(url) {
  try {
    return new URL(url).hostname;
  } catch {
    return url || "";
  }
}

/**
 * Inputs: MCP URL. Outputs: catalog entry id when host/url matches, else "".
 */
export function marketplaceIdForUrl(url) {
  const trimmed = (url || "").trim();
  if (!trimmed) return "";
  const entries = listMarketplaceEntries();
  const exact = entries.find((e) => e.url === trimmed);
  if (exact) return exact.id;
  const host = marketplaceHost(trimmed);
  if (!host) return "";
  return entries.find((e) => marketplaceHost(e.url) === host)?.id || "";
}
