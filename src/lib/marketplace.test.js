import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  filterMarketplaceEntries,
  groupSummary,
  installedServers,
  listMarketplaceEntries,
  marketplaceHost,
  marketplaceIdForUrl,
  nameTaken,
  nextAccountName,
  serverGroups,
} from "./marketplace.js";

describe("marketplace helpers", () => {
  it("lists curated catalog entries", () => {
    const entries = listMarketplaceEntries();
    assert.deepEqual(
      entries.map((e) => e.id).sort(),
      [
        "canva",
        "fathom",
        "granola",
        "intercom",
        "linear",
        "neon",
        "notion",
        "plane",
        "render",
        "sentry",
        "stripe",
      ],
    );
  });

  it("marks API-key providers with an https key page", () => {
    const keyed = listMarketplaceEntries().filter((e) => e.apiKeyUrl);
    assert.deepEqual(keyed.map((e) => e.id), ["render"]);
    for (const e of keyed) assert.match(e.apiKeyUrl, /^https:\/\//);
  });

  it("lists every installed account for an entry URL", () => {
    const entry = { name: "Plane", url: "https://mcp.plane.so/http/mcp" };
    const http = (id, name) => ({
      id,
      name,
      transport: { kind: "http", url: entry.url },
    });
    const servers = [http("1", "Plane"), http("2", "Plane 2"), { id: "3", name: "x", transport: { kind: "stdio" } }];
    assert.deepEqual(installedServers(entry, servers).map((s) => s.id), ["1", "2"]);
    assert.deepEqual(installedServers(entry, []), []);
    assert.equal(nextAccountName(entry, [http("1", "Plane")]), "Plane 2");
    assert.equal(nextAccountName(entry, servers), "Plane 3");
    assert.equal(nextAccountName(entry, [http("1", "Plane"), { id: "9", name: "plane 2" }]), "Plane 3");
    assert.equal(nameTaken(servers, " plane 2 "), true);
    assert.equal(nameTaken(servers, "Plane · acme"), false);
  });

  it("filters by name description or url", () => {
    const entries = [
      { name: "Fathom", description: "meetings", url: "https://api.fathom.ai/mcp" },
      { name: "Linear", description: "issues", url: "https://mcp.linear.app/mcp" },
    ];
    assert.equal(filterMarketplaceEntries(entries, "fathom").length, 1);
    assert.equal(filterMarketplaceEntries(entries, "issues").length, 1);
    assert.equal(filterMarketplaceEntries(entries, "linear.app").length, 1);
    assert.equal(filterMarketplaceEntries(entries, "").length, 2);
  });

  it("returns hostname for display", () => {
    assert.equal(marketplaceHost("https://api.fathom.ai/mcp"), "api.fathom.ai");
    assert.equal(marketplaceHost("not-a-url"), "not-a-url");
  });

  it("resolves catalog id from MCP URL", () => {
    assert.equal(marketplaceIdForUrl("https://api.fathom.ai/mcp"), "fathom");
    assert.equal(marketplaceIdForUrl("https://mcp.linear.app/mcp"), "linear");
    assert.equal(marketplaceIdForUrl("https://mcp.granola.ai/mcp"), "granola");
    assert.equal(marketplaceIdForUrl("https://mcp.plane.so/http/mcp"), "plane");
    assert.equal(marketplaceIdForUrl("https://example.com/mcp"), "");
  });

  it("groups servers by app: catalog entry, then URL host, then local commands", () => {
    const http = (id, name, url, enabled = true) => ({ id, name, enabled, transport: { kind: "http", url } });
    const servers = [
      http("a", "doppl", "https://mcp.render.com/mcp"),
      { id: "b", name: "fs", enabled: true, transport: { kind: "stdio", command: "npx", args: [] } },
      http("c", "vinci", "https://mcp.render.com/mcp", false),
      http("d", "Sentry-vinci", "https://mcp.sentry.dev/other"),
      http("e", "Custom", "https://example.com/mcp"),
      http("f", "Local box", "https://local/mcp"),
    ];
    assert.deepEqual(
      serverGroups(servers).map((g) => [g.id, g.name, g.servers.map((s) => s.id)]),
      [
        ["render", "Render", ["a", "c"]],
        ["local", "Local commands", ["b"]],
        ["sentry", "Sentry", ["d"]],
        ["host:example.com", "example.com", ["e"]],
        ["host:local", "local", ["f"]],
      ],
    );
    assert.equal(groupSummary(servers.slice(0, 3)), "3 MCPs · 1 off");
    assert.equal(groupSummary(servers.slice(0, 1)), "1 MCP");
  });
});
