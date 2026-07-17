/**
 * Capture docs/README UI screenshots via Vite + mocked Tauri IPC.
 * Inputs: optional BASE_URL (default http://127.0.0.1:1420).
 * Outputs: PNGs under docs/public/images/.
 */
import { chromium } from "playwright";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const outDir = path.join(root, "docs/public/images");
const base = process.env.BASE_URL || "http://127.0.0.1:1420";
const size = { width: 939, height: 1131 };

const token = "sumeru_docs_screenshot_token_0001";
const servers = [
  {
    id: "fathom-demo",
    name: "Fathom",
    enabled: true,
    transport: {
      kind: "http",
      url: "https://api.fathom.ai/mcp",
      header_keys: [],
      has_bearer: true,
    },
  },
];

/**
 * Inputs: page, filename, optional clip-to-#app.
 * Outputs: PNG written under docs/public/images.
 */
async function shot(page, name, { clipApp = false } = {}) {
  const opts = { path: path.join(outDir, name), type: "png" };
  if (clipApp) {
    const bottom = await page.evaluate(() => {
      const app = document.getElementById("app");
      return Math.ceil(app.getBoundingClientRect().bottom + 16);
    });
    opts.clip = {
      x: 0,
      y: 0,
      width: size.width,
      height: Math.min(Math.max(bottom, 640), size.height),
    };
  }
  await page.screenshot(opts);
  console.log("wrote", name);
}

const browser = await chromium.launch();
const page = await browser.newPage({
  viewport: size,
  deviceScaleFactor: 1,
  colorScheme: "dark",
});

await page.addInitScript(
  ({ token, servers }) => {
    const state = {
      running: true,
      endpoint: "http://127.0.0.1:7341/mcp",
      token,
      servers,
      autostart: false,
    };
    window.__TAURI_INTERNALS__ = {
      metadata: {
        currentWindow: { label: "main" },
        currentWebview: { windowLabel: "main", label: "main" },
      },
      transformCallback: () => 0,
      unregisterCallback: () => {},
      convertFileSrc: (p) => p,
      invoke: async (cmd) => {
        switch (cmd) {
          case "get_status":
            return { running: state.running, endpoint: state.endpoint };
          case "get_token":
            return state.token;
          case "list_servers":
            return state.servers;
          case "get_autostart":
            return state.autostart;
          case "set_autostart":
          case "start_funnel":
          case "stop_funnel":
          case "rotate_token":
          case "start_mcp_oauth":
          case "upsert_server":
          case "remove_server":
          case "open_url":
          case "open_docs":
            return;
          case "test_server":
          case "test_draft":
          case "mcp_stdio_command":
            return "ok";
          case "probe_mcp_auth":
            return {
              oauth: true,
              supports_dcr: false,
              has_saved_client: false,
              label: "Sign in",
            };
          default:
            if (String(cmd).startsWith("plugin:window|")) return;
            throw new Error(`unmocked invoke: ${cmd}`);
        }
      },
    };
  },
  { token, servers },
);

await page.goto(base, { waitUntil: "networkidle" });
await page.getByRole("heading", { name: "sumeru" }).waitFor();
await page.getByText("running").waitFor();
await shot(page, "configure.png", { clipApp: true });

await page.getByRole("tab", { name: "Marketplace" }).click();
await page.getByText("Installed").waitFor();
await shot(page, "marketplace.png", { clipApp: true });

await page.getByRole("tab", { name: "Configure" }).click();
await page.getByRole("button", { name: "Add MCP" }).click();
await page.getByRole("heading", { name: "Add MCP" }).waitFor();
await shot(page, "add-mcp.png");

await page.locator("#edit-source").fill("https://mcp.example.com/mcp");
await page.getByText("Detected HTTP MCP").waitFor();
await page
  .locator("details")
  .filter({ hasText: "Advanced" })
  .locator("summary")
  .click();
await page.getByLabel("OAuth client ID (optional)").waitFor();
await page.locator("#edit-headers").scrollIntoViewIfNeeded();
await page.waitForTimeout(400);
await shot(page, "add-mcp-advanced.png");

await browser.close();
