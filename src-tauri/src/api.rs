//! UI-agnostic app API: one function per command, `dispatch` by name, and the
//! `sumeru engine` stdio loop. Shared by the Tauri app (`desktop.rs`) and the
//! native macOS app, which drives `sumeru engine` over pipes.

use std::sync::Arc;

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use uuid::Uuid;

use crate::config::{
    self, delete_oauth_secrets, delete_secrets, prune_secrets, redact, save_config, store_secrets,
    validate_key, validate_server, validate_unique, ApiKey, KeyMode, McpServer, McpTransport,
    SecretMap, DEFAULT_KEY_ID,
};
use crate::gateway::{self, endpoint_url, AppInner};
use crate::oauth;

type App = Arc<AppInner>;

#[derive(Serialize)]
pub struct StatusDto {
    running: bool,
    endpoint: String,
}

#[derive(Serialize)]
pub struct ServerDto {
    id: String,
    name: String,
    enabled: bool,
    transport: McpTransport,
}

#[derive(Serialize)]
pub struct KeyDto {
    #[serde(flatten)]
    key: ApiKey,
    token: String,
}

#[derive(Deserialize)]
struct IdParam {
    id: String,
}

#[derive(Deserialize)]
struct UrlParam {
    url: String,
}

#[derive(Deserialize)]
struct UpsertServer {
    id: Option<String>,
    name: String,
    enabled: bool,
    transport: McpTransport,
    secrets: SecretMap,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpsertKey {
    id: Option<String>,
    name: String,
    mode: KeyMode,
    mcp_ids: Vec<String>,
}

#[derive(Deserialize)]
struct ProbeAuth {
    url: String,
    id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartOauth {
    url: String,
    id: String,
    client_id: Option<String>,
    client_secret: Option<String>,
}

#[derive(Deserialize)]
struct TestDraft {
    name: String,
    transport: McpTransport,
    secrets: SecretMap,
}

/// Inputs: app state, command name, JSON params (camelCase, as `src/lib/api.js` sends them).
/// Outputs: the command's JSON result, or a user-facing error string.
pub async fn dispatch(app: &App, method: &str, params: Value) -> Result<Value, String> {
    fn p<T: DeserializeOwned>(v: Value) -> Result<T, String> {
        serde_json::from_value(v).map_err(|e| format!("invalid params: {e}"))
    }
    fn ok<T: Serialize>(v: T) -> Result<Value, String> {
        serde_json::to_value(v).map_err(|e| e.to_string())
    }
    match method {
        "get_status" => ok(get_status(app).await),
        "list_servers" => ok(list_servers(app).await),
        "upsert_server" => ok(upsert_server(app, p(params)?).await?),
        "remove_server" => ok(remove_server(app, p::<IdParam>(params)?.id).await?),
        "start_funnel" => ok(gateway::start(app.clone()).await?),
        "stop_funnel" => ok(gateway::stop(app.clone()).await?),
        "list_keys" => ok(list_keys(app).await),
        "upsert_key" => ok(upsert_key(app, p(params)?).await?),
        "remove_key" => ok(remove_key(app, p::<IdParam>(params)?.id).await?),
        "rotate_key" => ok(rotate_key(app, p::<IdParam>(params)?.id).await?),
        "test_server" => ok(test_server(app, p::<IdParam>(params)?.id).await?),
        "test_draft" => ok(test_draft(app, p(params)?).await?),
        "probe_mcp_auth" => {
            let a: ProbeAuth = p(params)?;
            ok(oauth::probe(&a.url, a.id.as_deref()).await?)
        }
        "start_mcp_oauth" => ok(start_mcp_oauth(app, p(params)?).await?),
        "open_url" => ok(open_url(&p::<UrlParam>(params)?.url)?),
        "mcp_stdio_command" => ok(mcp_stdio_command()?),
        _ => Err(format!("unknown method {method:?}")),
    }
}

/// Inputs: app state. Outputs: funnel running flag (local or healthy external) and endpoint URL.
async fn get_status(app: &App) -> StatusDto {
    let running = app.running.load(std::sync::atomic::Ordering::SeqCst)
        || gateway::existing_endpoint_healthy(&app.default_token().await).await;
    StatusDto {
        running,
        endpoint: endpoint_url().into(),
    }
}

/// Inputs: app state. Outputs: configured MCP servers (no secret values).
async fn list_servers(app: &App) -> Vec<ServerDto> {
    let cfg = app.config.read().await;
    cfg.servers
        .iter()
        .map(|s| ServerDto {
            id: s.id.clone(),
            name: s.name.clone(),
            enabled: s.enabled,
            transport: s.transport.clone(),
        })
        .collect()
}

/// Inputs: optional id, name, enabled, transport, secrets. Outputs: saved server id.
async fn upsert_server(app: &App, a: UpsertServer) -> Result<String, String> {
    let id =
        a.id.filter(|s| !s.is_empty())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
    let mut transport = a.transport;
    if let McpTransport::Http { has_bearer, .. } = &mut transport {
        let keep_bearer = !*has_bearer
            && a.secrets.bearer.as_ref().is_none_or(|b| b.is_empty())
            && config::get_bearer_secret(&id).is_some();
        if keep_bearer {
            *has_bearer = true;
        }
    }
    let server = McpServer {
        id: id.clone(),
        name: a.name,
        enabled: a.enabled,
        transport,
    };
    validate_server(&server).map_err(|e| e.to_string())?;

    let mut staged = app.config.read().await.clone();
    let previous = staged.servers.iter().find(|s| s.id == id).cloned();
    validate_unique(&staged, &server, previous.is_some()).map_err(|e| e.to_string())?;
    if let Some(existing) = staged.servers.iter_mut().find(|s| s.id == id) {
        *existing = server.clone();
    } else {
        staged.servers.push(server.clone());
    }
    save_config(&app.dir, &staged).map_err(|e| e.to_string())?;
    store_secrets(&id, &a.secrets).map_err(|e| e.to_string())?;
    if let Some(prev) = &previous {
        prune_secrets(prev, &server).map_err(|e| e.to_string())?;
    }

    *app.config.write().await = staged;
    app.pool.invalidate(&id).await;
    Ok(id)
}

/// Inputs: mcp id. Outputs: unit after removal (also pruned from every key's scope).
async fn remove_server(app: &App, id: String) -> Result<(), String> {
    let mut staged = app.config.read().await.clone();
    let Some(pos) = staged.servers.iter().position(|s| s.id == id) else {
        return Err("unknown mcp".into());
    };
    let removed = staged.servers.remove(pos);
    for key in &mut staged.keys {
        key.mcp_ids.retain(|m| *m != id);
    }
    save_config(&app.dir, &staged).map_err(|e| e.to_string())?;
    *app.config.write().await = staged;
    app.pool.invalidate(&id).await;
    let cleanup = match &removed.transport {
        McpTransport::Stdio { env_keys, .. } => delete_secrets(&id, env_keys, &[], false),
        McpTransport::Http {
            header_keys,
            has_bearer,
            ..
        } => delete_secrets(&id, &[], header_keys, *has_bearer)
            .and_then(|_| delete_oauth_secrets(&id)),
    };
    if let Err(err) = cleanup {
        eprintln!("sumeru secret cleanup after remove failed: {err}");
    }
    Ok(())
}

/// Inputs: app state. Outputs: API keys with their bearer tokens (for agent setup).
async fn list_keys(app: &App) -> Vec<KeyDto> {
    let tokens = app.tokens.read().await;
    let token = |id: &str| tokens.iter().find(|(k, _)| k == id).map(|(_, t)| t.clone());
    let cfg = app.config.read().await;
    cfg.keys
        .iter()
        .map(|k| KeyDto {
            key: k.clone(),
            token: token(&k.id).unwrap_or_default(),
        })
        .collect()
}

/// Inputs: optional id, name, scope mode, MCP ids. Outputs: saved key id (new keys get a token).
async fn upsert_key(app: &App, a: UpsertKey) -> Result<String, String> {
    let key = ApiKey {
        id: a
            .id
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| Uuid::new_v4().to_string()),
        name: a.name.trim().into(),
        mode: a.mode,
        mcp_ids: a.mcp_ids,
    };
    let mut staged = app.config.read().await.clone();
    validate_key(&staged, &key).map_err(|e| e.to_string())?;
    match staged.keys.iter_mut().find(|k| k.id == key.id) {
        Some(existing) => *existing = key.clone(),
        None => staged.keys.push(key.clone()),
    }
    save_config(&app.dir, &staged).map_err(|e| e.to_string())?;
    *app.config.write().await = staged;
    app.reload_tokens().await?;
    Ok(key.id)
}

/// Inputs: key id. Outputs: unit after deleting the key and its token (Default stays).
async fn remove_key(app: &App, id: String) -> Result<(), String> {
    if id == DEFAULT_KEY_ID {
        return Err("the Default key cannot be deleted".into());
    }
    let mut staged = app.config.read().await.clone();
    staged.keys.retain(|k| k.id != id);
    save_config(&app.dir, &staged).map_err(|e| e.to_string())?;
    *app.config.write().await = staged;
    config::delete_key_token(&id).map_err(|e| e.to_string())?;
    app.reload_tokens().await
}

/// Inputs: key id. Outputs: the key's new token (the old one stops working immediately).
async fn rotate_key(app: &App, id: String) -> Result<String, String> {
    let token = config::rotate_key_token(&id).map_err(|e| e.to_string())?;
    app.reload_tokens().await?;
    Ok(token)
}

/// Inputs: http(s) URL or Cursor/VS Code MCP install deeplink. Outputs: unit after opening it.
fn open_url(url: &str) -> Result<(), String> {
    let parsed = url::Url::parse(url).map_err(|e| e.to_string())?;
    if !matches!(parsed.scheme(), "http" | "https" | "cursor" | "vscode") {
        return Err("only http(s), cursor, and vscode URLs can be opened".into());
    }
    open::that(url).map_err(|e| e.to_string())
}

/// Inputs: none. Outputs: preferred path for stdio MCP (`~/.local/bin/sumeru` when present).
fn mcp_stdio_command() -> Result<String, String> {
    let stable = dirs::home_dir()
        .map(|h| h.join(".local/bin/sumeru"))
        .filter(|p| p.is_file());
    if let Some(p) = stable {
        return Ok(p.to_string_lossy().into_owned());
    }
    std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|e| e.to_string())
}

/// Inputs: mcp id. Outputs: Ok message or connection error.
async fn test_server(app: &App, id: String) -> Result<String, String> {
    let server = app.find(&id).await.ok_or("unknown mcp")?;
    let tools = app
        .pool
        .list_tools(&server)
        .await
        .map_err(|e| redact(&e.to_string()))?;
    Ok(format!("connected ({} tools)", tools.len()))
}

/// Inputs: MCP URL, mcp id, optional client credentials. Outputs: unit after browser sign-in.
/// Forces a fresh provider login when another MCP already uses the URL (another account).
async fn start_mcp_oauth(app: &App, a: StartOauth) -> Result<(), String> {
    let fresh_login = app.config.read().await.servers.iter().any(|s| {
        s.id != a.id && matches!(&s.transport, McpTransport::Http { url: u, .. } if *u == a.url)
    });
    oauth::authorize(&a.url, &a.id, a.client_id, a.client_secret, fresh_login).await?;
    app.pool.invalidate(&a.id).await;
    Ok(())
}

/// Inputs: unsaved transport + secrets. Outputs: Ok message when a temporary connect works.
async fn test_draft(app: &App, a: TestDraft) -> Result<String, String> {
    let id = format!("draft-{}", Uuid::new_v4());
    let server = McpServer {
        id: id.clone(),
        name: a.name,
        enabled: true,
        transport: a.transport.clone(),
    };
    validate_server(&server).map_err(|e| e.to_string())?;
    store_secrets(&id, &a.secrets).map_err(|e| e.to_string())?;
    let result = app.pool.list_tools(&server).await;
    app.pool.invalidate(&id).await;
    let cleanup = match &a.transport {
        McpTransport::Stdio { env_keys, .. } => delete_secrets(&id, env_keys, &[], false),
        McpTransport::Http {
            header_keys,
            has_bearer,
            ..
        } => delete_secrets(&id, &[], header_keys, *has_bearer),
    };
    cleanup.map_err(|e| e.to_string())?;
    result
        .map(|t| format!("connected ({} tools)", t.len()))
        .map_err(|e| redact(&e.to_string()))
}

/// Inputs: app state. Outputs: unit after loading key tokens (may wait on a keychain prompt)
/// and starting the gateway; failures are logged, not fatal (Resume retries the start).
pub async fn autostart_gateway(app: App) {
    let started = match app.reload_tokens().await {
        Ok(()) => gateway::start(app).await,
        Err(err) => Err(err),
    };
    if let Err(err) = started {
        eprintln!("sumeru auto-start failed: {err}");
    }
}

/// Inputs: app state, one request line `{"id", "method", "params"?}`.
/// Outputs: one reply line `{"id", "result"}` or `{"id", "error"}`.
pub async fn handle_line(app: &App, line: &str) -> String {
    #[derive(Deserialize)]
    struct Req {
        id: Value,
        method: String,
        #[serde(default)]
        params: Value,
    }
    match serde_json::from_str::<Req>(line) {
        Ok(r) => match dispatch(app, &r.method, r.params).await {
            Ok(result) => json!({ "id": r.id, "result": result }),
            Err(error) => json!({ "id": r.id, "error": error }),
        },
        Err(e) => json!({ "id": null, "error": format!("invalid request: {e}") }),
    }
    .to_string()
}

/// Inputs: app state. Outputs: runs the gateway and answers newline-delimited JSON requests on
/// stdin (replies on stdout, concurrently) until stdin closes, then stops the gateway.
pub async fn serve_engine(app: App) {
    tokio::spawn(autostart_gateway(app.clone()));
    let out = Arc::new(tokio::sync::Mutex::new(tokio::io::stdout()));
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() {
            continue;
        }
        let (app, out) = (app.clone(), out.clone());
        tokio::spawn(async move {
            let reply = handle_line(&app, &line).await + "\n";
            let mut out = out.lock().await;
            let _ = out.write_all(reply.as_bytes()).await;
            let _ = out.flush().await;
        });
    }
    let _ = gateway::stop(app).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> (tempfile::TempDir, App) {
        let dir = tempfile::tempdir().unwrap();
        let app = Arc::new(AppInner::new(dir.path().to_path_buf()).unwrap());
        (dir, app)
    }

    #[tokio::test]
    async fn dispatch_lists_and_rejects_unknown() {
        let (_dir, app) = app();
        assert_eq!(
            dispatch(&app, "list_servers", Value::Null).await,
            Ok(json!([]))
        );
        let err = dispatch(&app, "nope", Value::Null).await.unwrap_err();
        assert!(err.contains("unknown method"), "{err}");
        let err = dispatch(&app, "remove_server", json!({}))
            .await
            .unwrap_err();
        assert!(err.starts_with("invalid params"), "{err}");
        let err = dispatch(&app, "remove_key", json!({ "id": DEFAULT_KEY_ID }))
            .await
            .unwrap_err();
        assert!(err.contains("cannot be deleted"), "{err}");
    }

    #[tokio::test]
    async fn engine_lines_echo_ids() {
        let (_dir, app) = app();
        let ok: Value =
            serde_json::from_str(&handle_line(&app, r#"{"id":7,"method":"list_keys"}"#).await)
                .unwrap();
        assert_eq!(ok["id"], 7);
        assert_eq!(ok["result"][0]["id"], DEFAULT_KEY_ID);
        let err: Value =
            serde_json::from_str(&handle_line(&app, r#"{"id":"x","method":"nope"}"#).await)
                .unwrap();
        assert_eq!(err["id"], "x");
        assert!(err["error"].as_str().unwrap().contains("unknown method"));
        let bad: Value = serde_json::from_str(&handle_line(&app, "not json").await).unwrap();
        assert!(bad["id"].is_null() && bad["error"].is_string());
    }
}
