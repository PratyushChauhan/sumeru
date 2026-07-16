const { invoke } = window.__TAURI__.core;

const $ = (sel) => document.querySelector(sel);

/** @type {null | { oauth: boolean, label?: string, needs_client_id?: boolean }} */
let oauthProbe = null;
let oauthConnected = false;
let probeTimer = null;

/**
 * Inputs: text. Outputs: clipboard write promise.
 */
async function copy(text) {
  await navigator.clipboard.writeText(text);
}

/**
 * Inputs: status, token, and whether the live token may appear in the snippet.
 * Outputs: updated endpoint UI.
 */
function renderEndpoint(status, token, revealToken) {
  const running = !!status.running;
  $("#status-pill").textContent = running ? "running" : "stopped";
  $("#status-pill").className = `pill ${running ? "on" : "off"}`;
  $("#btn-toggle").textContent = running ? "Pause" : "Resume";
  $("#endpoint-url").value = status.endpoint;
  $("#endpoint-token").value = token;
  const auth = revealToken ? `Bearer ${token}` : "Bearer <TOKEN>";
  $("#client-snippet").textContent = JSON.stringify(
    {
      mcpServers: {
        funnelit: {
          url: status.endpoint,
          headers: { Authorization: auth },
        },
      },
    },
    null,
    2,
  );
}

/**
 * Inputs: server list. Outputs: rebuilt MCP list DOM.
 */
function renderServers(servers) {
  const list = $("#server-list");
  list.replaceChildren();
  if (!servers.length) {
    const empty = document.createElement("li");
    empty.textContent = "No MCPs yet. Add a stdio command or paste an MCP URL.";
    list.append(empty);
    return;
  }
  for (const server of servers) {
    const li = document.createElement("li");
    const info = document.createElement("div");
    const title = document.createElement("strong");
    title.textContent = server.name;
    const meta = document.createElement("div");
    meta.className = "meta";
    const kind = server.transport.kind;
    const detail =
      kind === "stdio"
        ? `${server.transport.command} ${(server.transport.args || []).join(" ")}`
        : server.transport.url;
    meta.textContent = `${kind} · ${server.enabled ? "enabled" : "disabled"} · ${detail}`;
    info.append(title, meta);

    const actions = document.createElement("div");
    actions.className = "actions";
    const testBtn = document.createElement("button");
    testBtn.type = "button";
    testBtn.textContent = "Test";
    testBtn.addEventListener("click", async () => {
      try {
        alert(await invoke("test_server", { id: server.id }));
      } catch (e) {
        alert(String(e));
      }
    });
    const editBtn = document.createElement("button");
    editBtn.type = "button";
    editBtn.textContent = "Edit";
    editBtn.addEventListener("click", () => openEditor(server));
    const delBtn = document.createElement("button");
    delBtn.type = "button";
    delBtn.className = "danger";
    delBtn.textContent = "Delete";
    delBtn.addEventListener("click", async () => {
      if (!confirm(`Delete ${server.name}?`)) return;
      await invoke("remove_server", { id: server.id });
      await refresh();
    });
    actions.append(testBtn, editBtn, delBtn);
    li.append(info, actions);
    list.append(li);
  }
}

/**
 * Inputs: KEY=value text. Outputs: object map or throws on malformed lines.
 */
function parsePairs(text) {
  const out = {};
  for (const [index, line] of text.split("\n").entries()) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    const i = trimmed.indexOf("=");
    if (i <= 0) {
      throw new Error(`Invalid KEY=value entry on line ${index + 1}`);
    }
    out[trimmed.slice(0, i).trim()] = trimmed.slice(i + 1);
  }
  return out;
}

/**
 * Inputs: connection string. Outputs: true when it looks like an MCP URL.
 */
function isHttpSource(value) {
  return /^https?:\/\//i.test(value.trim());
}

/**
 * Inputs: command line text. Outputs: { command, args } after a simple split.
 */
function splitCommandLine(text) {
  const parts = text.trim().match(/(?:[^\s"]+|"[^"]*")+/g) || [];
  const tokens = parts.map((p) => p.replace(/^"|"$/g, ""));
  return { command: tokens[0] || "", args: tokens.slice(1) };
}

/**
 * Inputs: none. Outputs: editor mcp id, creating one when missing.
 */
function ensureEditId() {
  if (!$("#edit-id").value) {
    $("#edit-id").value = crypto.randomUUID();
  }
  return $("#edit-id").value;
}

/**
 * Inputs: none. Outputs: transport + secrets from the editor form.
 */
function readDraft() {
  const source = $("#edit-source").value.trim();
  if (isHttpSource(source)) {
    const headers = parsePairs($("#edit-headers").value);
    const bearer = $("#edit-bearer").value.trim();
    return {
      transport: {
        kind: "http",
        url: source,
        header_keys: Object.keys(headers),
        has_bearer: oauthConnected || !!bearer,
      },
      secrets: { env: {}, headers, bearer: bearer || null },
    };
  }
  const env = parsePairs($("#edit-env").value);
  const argLines = $("#edit-args")
    .value.split("\n")
    .map((s) => s.trim())
    .filter(Boolean);
  const parsed = splitCommandLine(source);
  return {
    transport: {
      kind: "stdio",
      command: parsed.command,
      args: argLines.length ? parsed.args.concat(argLines) : parsed.args,
      env_keys: Object.keys(env),
    },
    secrets: { env, headers: {}, bearer: null },
  };
}

/**
 * Inputs: none. Outputs: OAuth panel synced to latest probe.
 */
function renderOauth() {
  const panel = $("#oauth-panel");
  const creds = $("#oauth-creds");
  const advanced = $("#http-advanced");
  const btn = $("#btn-oauth");
  const status = $("#oauth-status");
  if (!oauthProbe?.oauth) {
    panel.classList.add("hidden");
    creds.classList.add("hidden");
    advanced.classList.add("bare");
    advanced.open = true;
    return;
  }
  panel.classList.remove("hidden");
  advanced.classList.remove("bare");
  advanced.open = false;
  creds.classList.toggle("hidden", !oauthProbe.needs_client_id);
  status.textContent = oauthConnected
    ? "Connected"
    : "Browser sign-in required";
  btn.textContent = oauthConnected
    ? "Reconnect"
    : oauthProbe.label || "Sign in";
}

/**
 * Inputs: none. Outputs: probe + OAuth UI for the current HTTP URL.
 */
async function probeAuth() {
  const url = $("#edit-source").value.trim();
  if (!isHttpSource(url)) {
    oauthProbe = null;
    renderOauth();
    return;
  }
  const probedUrl = url;
  $("#oauth-status").textContent = "Checking sign-in…";
  $("#oauth-panel").classList.remove("hidden");
  let next;
  try {
    next = await invoke("probe_mcp_auth", {
      url,
      id: $("#edit-id").value || null,
    });
  } catch {
    next = { oauth: false };
  }
  if ($("#edit-source").value.trim() !== probedUrl) return;
  oauthProbe = next;
  if (next.connected) oauthConnected = true;
  renderOauth();
}

/**
 * Inputs: none. Outputs: form sections matched to detected transport.
 */
function syncKind() {
  const http = isHttpSource($("#edit-source").value);
  $("#stdio-fields").classList.toggle("hidden", http);
  $("#http-fields").classList.toggle("hidden", !http);
  $("#edit-kind-hint").textContent = http
    ? "Detected HTTP MCP"
    : "Detected stdio command";
  if (!http) {
    oauthProbe = null;
    renderOauth();
    return;
  }
  clearTimeout(probeTimer);
  probeTimer = setTimeout(() => {
    probeAuth().catch(console.error);
  }, 250);
}

/**
 * Inputs: optional server. Outputs: opened editor dialog.
 */
function openEditor(server) {
  for (const selector of [
    "#edit-source",
    "#edit-args",
    "#edit-env",
    "#edit-bearer",
    "#edit-headers",
    "#edit-oauth-client-id",
    "#edit-oauth-client-secret",
  ]) {
    $(selector).value = "";
  }

  $("#editor-title").textContent = server ? "Edit MCP" : "Add MCP";
  $("#edit-id").value = server?.id || "";
  $("#edit-name").value = server?.name || "";
  $("#edit-enabled").checked = server?.enabled ?? true;
  $("#editor-msg").textContent = "";
  $("#editor-msg").className = "msg";
  oauthConnected = !!(
    server?.transport?.kind === "http" && server.transport.has_bearer
  );
  oauthProbe = null;
  if (server?.transport?.kind === "http") {
    $("#edit-source").value = server.transport.url || "";
    $("#edit-headers").value = (server.transport.header_keys || [])
      .map((k) => `${k}=`)
      .join("\n");
  } else {
    $("#edit-source").value = server?.transport?.command || "";
    $("#edit-args").value = (server?.transport?.args || []).join("\n");
    $("#edit-env").value = (server?.transport?.env_keys || [])
      .map((k) => `${k}=`)
      .join("\n");
  }
  syncKind();
  $("#editor").showModal();
}

/**
 * Inputs: none. Outputs: refreshed status, token, and server list.
 */
async function refresh() {
  const [status, token, servers, autostart] = await Promise.all([
    invoke("get_status"),
    invoke("get_token"),
    invoke("list_servers"),
    invoke("get_autostart"),
  ]);
  const reveal = $("#endpoint-token").type === "text";
  renderEndpoint(status, token, reveal);
  renderServers(servers);
  $("#autostart").checked = !!autostart;
}

window.addEventListener("DOMContentLoaded", () => {
  $("#edit-source").addEventListener("input", () => {
    oauthConnected = false;
    syncKind();
  });
  $("#edit-source").addEventListener("paste", () => {
    oauthConnected = false;
    queueMicrotask(syncKind);
  });
  $("#btn-oauth").addEventListener("click", async () => {
    const msg = $("#editor-msg");
    const url = $("#edit-source").value.trim();
    msg.className = "msg";
    msg.textContent = "Waiting for browser sign-in…";
    try {
      const id = ensureEditId();
      await invoke("start_mcp_oauth", {
        url,
        id,
        clientId: $("#edit-oauth-client-id").value.trim() || null,
        clientSecret: $("#edit-oauth-client-secret").value.trim() || null,
      });
      oauthConnected = true;
      oauthProbe = {
        ...(oauthProbe || { oauth: true }),
        oauth: true,
        needs_client_id: false,
      };
      renderOauth();
      msg.textContent = "Signed in";
      msg.className = "msg ok";
    } catch (e) {
      msg.textContent = String(e);
      msg.className = "msg err";
    }
  });
  $("#btn-add").addEventListener("click", () => openEditor(null));
  $("#btn-copy-url").addEventListener("click", () => copy($("#endpoint-url").value));
  $("#btn-copy-token").addEventListener("click", () => copy($("#endpoint-token").value));
  $("#btn-show-token").addEventListener("click", async () => {
    const input = $("#endpoint-token");
    const hide = input.type === "text";
    input.type = hide ? "password" : "text";
    $("#btn-show-token").textContent = hide ? "Show" : "Hide";
    const status = await invoke("get_status");
    renderEndpoint(status, input.value, !hide);
  });
  $("#btn-rotate-token").addEventListener("click", async () => {
    await invoke("rotate_token");
    await refresh();
  });
  $("#btn-toggle").addEventListener("click", async () => {
    const status = await invoke("get_status");
    if (status.running) await invoke("stop_funnel");
    else await invoke("start_funnel");
    await refresh();
  });
  $("#autostart").addEventListener("change", async (e) => {
    try {
      await invoke("set_autostart", { enabled: e.target.checked });
    } catch (err) {
      e.target.checked = !e.target.checked;
      alert(String(err));
    }
  });
  $("#btn-test-draft").addEventListener("click", async () => {
    const msg = $("#editor-msg");
    msg.className = "msg";
    msg.textContent = "Testing…";
    try {
      const draft = readDraft();
      const result = await invoke("test_draft", {
        name: $("#edit-name").value || "draft",
        transport: draft.transport,
        secrets: draft.secrets,
      });
      msg.textContent = result;
      msg.className = "msg ok";
    } catch (e) {
      msg.textContent = String(e);
      msg.className = "msg err";
    }
  });
  $("#editor-form").addEventListener("submit", async (e) => {
    if (e.submitter?.value !== "save") return;
    e.preventDefault();
    const msg = $("#editor-msg");
    try {
      const draft = readDraft();
      const id = $("#edit-id").value || null;
      await invoke("upsert_server", {
        id: oauthConnected ? ensureEditId() : id,
        name: $("#edit-name").value.trim(),
        enabled: $("#edit-enabled").checked,
        transport: draft.transport,
        secrets: draft.secrets,
      });
      $("#editor").close();
      await refresh();
    } catch (error) {
      msg.textContent = String(error);
      msg.className = "msg err";
    }
  });
  (async () => {
    for (let i = 0; i < 25; i++) {
      await refresh();
      if ((await invoke("get_status")).running) break;
      await new Promise((r) => setTimeout(r, 100));
    }
  })().catch((e) => {
    console.error(e);
    alert(String(e));
  });
});
