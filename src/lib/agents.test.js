import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { agentInstalls } from "./agents.js";

const URL_ = "http://127.0.0.1:7341/mcp";
const TOKEN = "tok+/=";
const byId = (id) => agentInstalls(URL_, TOKEN).find((a) => a.id === id);

describe("agent install recipes", () => {
  it("covers each supported agent once", () => {
    assert.deepEqual(
      agentInstalls(URL_, TOKEN).map((a) => a.id),
      ["claude-code", "codex", "cursor", "vscode", "gemini", "windsurf"],
    );
  });

  it("embeds endpoint and bearer token in every snippet", () => {
    for (const a of agentInstalls(URL_, TOKEN)) {
      assert.ok(a.text.includes(URL_), a.id);
      assert.ok(a.text.includes(`Bearer ${TOKEN}`), a.id);
    }
  });

  it("builds a Cursor deeplink with base64 mcp.json config", () => {
    const link = new URL(byId("cursor").link);
    assert.equal(link.protocol, "cursor:");
    assert.equal(link.searchParams.get("name"), "sumeru");
    assert.deepEqual(JSON.parse(atob(link.searchParams.get("config"))), {
      url: URL_,
      headers: { Authorization: `Bearer ${TOKEN}` },
    });
  });

  it("builds a VS Code deeplink with http server config", () => {
    const link = byId("vscode").link;
    assert.ok(link.startsWith("vscode:mcp/install?"));
    assert.deepEqual(JSON.parse(decodeURIComponent(link.split("?")[1])), {
      name: "sumeru",
      type: "http",
      url: URL_,
      headers: { Authorization: `Bearer ${TOKEN}` },
    });
  });

  it("uses each agent's config shape", () => {
    assert.match(byId("codex").text, /^\[mcp_servers\.sumeru\]\nurl = /);
    assert.equal(JSON.parse(byId("windsurf").text).mcpServers.sumeru.serverUrl, URL_);
    assert.match(
      byId("claude-code").text,
      /^claude mcp remove --scope user sumeru 2>\/dev\/null; claude mcp add --transport http --scope user sumeru http\S+ --header /,
    );
    assert.match(byId("gemini").text, /^gemini mcp add --scope user --transport http sumeru http\S+ --header /);
  });

  it("installs under the per-key server name", () => {
    const work = agentInstalls(URL_, TOKEN, "sumeru-work");
    for (const a of work) assert.ok(a.text.includes("sumeru-work"), a.id);
    const find = (id) => work.find((a) => a.id === id);
    assert.match(find("claude-code").text, /remove --scope user sumeru-work .* --scope user sumeru-work http/);
    assert.match(find("codex").text, /^\[mcp_servers\.sumeru-work\]/);
    assert.equal(new URL(find("cursor").link).searchParams.get("name"), "sumeru-work");
    assert.equal(JSON.parse(decodeURIComponent(find("vscode").link.split("?")[1])).name, "sumeru-work");
  });

  it("builds committable repo recipes that never contain the token", () => {
    const repo = agentInstalls(URL_, TOKEN, "sumeru-work", "SUMERU_WORK_TOKEN");
    assert.deepEqual(repo.map((a) => a.id), ["claude-code", "codex", "cursor", "vscode", "gemini"]);
    for (const a of repo) {
      assert.ok(!a.text.includes(TOKEN), a.id);
      assert.ok(a.text.includes(URL_) && a.text.includes("sumeru-work"), a.id);
      assert.equal(a.link, undefined, a.id);
    }
    const find = (id) => repo.find((a) => a.id === id).text;
    assert.match(find("claude-code"), /--scope project sumeru-work .*'Authorization: Bearer \$\{SUMERU_WORK_TOKEN\}'$/);
    assert.match(find("codex"), /bearer_token_env_var = "SUMERU_WORK_TOKEN"/);
    assert.equal(JSON.parse(find("cursor")).mcpServers["sumeru-work"].headers.Authorization, "Bearer ${env:SUMERU_WORK_TOKEN}");
    const vscode = JSON.parse(find("vscode"));
    assert.equal(vscode.inputs[0].password, true);
    assert.equal(vscode.servers["sumeru-work"].headers.Authorization, `Bearer \${input:${vscode.inputs[0].id}}`);
    assert.match(find("gemini"), /--scope project --transport http sumeru-work http\S+ --header 'Authorization: Bearer \$\{SUMERU_WORK_TOKEN\}'$/);
  });
});
