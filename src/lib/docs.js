import { marked } from "marked";
import overview from "../../docs/index.md?raw";
import gettingStarted from "../../docs/getting-started.md?raw";
import endpoint from "../../docs/endpoint.md?raw";
import oauth from "../../docs/oauth.md?raw";
import marketplace from "../../docs/marketplace.md?raw";
import cli from "../../docs/cli.md?raw";
import connectCursor from "../../docs/cookbook/connect-cursor.md?raw";
import addStdioMcp from "../../docs/cookbook/add-stdio-mcp.md?raw";
import oauthWithoutDcr from "../../docs/cookbook/oauth-without-dcr.md?raw";
import installFromMarketplace from "../../docs/cookbook/install-from-marketplace.md?raw";
import runAtStartup from "../../docs/cookbook/run-at-startup.md?raw";
import rotateToken from "../../docs/cookbook/rotate-token.md?raw";

marked.setOptions({ gfm: true, breaks: false });

/**
 * Inputs: none.
 * Outputs: docs pages for Guides and Cookbook (id, title, section, body).
 */
export const DOC_PAGES = [
  { id: "overview", title: "Overview", section: "Guides", body: overview },
  {
    id: "getting-started",
    title: "Getting started",
    section: "Guides",
    body: gettingStarted,
  },
  { id: "endpoint", title: "Endpoint", section: "Guides", body: endpoint },
  { id: "oauth", title: "OAuth", section: "Guides", body: oauth },
  {
    id: "marketplace",
    title: "Marketplace",
    section: "Guides",
    body: marketplace,
  },
  { id: "cli", title: "CLI", section: "Guides", body: cli },
  {
    id: "cookbook/connect-cursor",
    title: "Connect Cursor",
    section: "Cookbook",
    body: connectCursor,
  },
  {
    id: "cookbook/add-stdio-mcp",
    title: "Add a stdio MCP",
    section: "Cookbook",
    body: addStdioMcp,
  },
  {
    id: "cookbook/oauth-without-dcr",
    title: "OAuth without DCR",
    section: "Cookbook",
    body: oauthWithoutDcr,
  },
  {
    id: "cookbook/install-from-marketplace",
    title: "Install from Marketplace",
    section: "Cookbook",
    body: installFromMarketplace,
  },
  {
    id: "cookbook/run-at-startup",
    title: "Run at startup",
    section: "Cookbook",
    body: runAtStartup,
  },
  {
    id: "cookbook/rotate-token",
    title: "Rotate the endpoint token",
    section: "Cookbook",
    body: rotateToken,
  },
];

/**
 * Inputs: none.
 * Outputs: ordered section names present in DOC_PAGES.
 */
export function docSections() {
  return [...new Set(DOC_PAGES.map((p) => p.section))];
}

/**
 * Inputs: page id. Outputs: page record or undefined.
 */
export function getDocPage(id) {
  return DOC_PAGES.find((p) => p.id === id);
}

/**
 * Inputs: markdown body. Outputs: HTML string for first-party docs.
 */
export function renderDocMarkdown(body) {
  return marked.parse(body ?? "", { async: false });
}
