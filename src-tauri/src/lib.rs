mod config;
mod docs_server;
mod gateway;
mod oauth;
mod pool;

use std::sync::Arc;

use config::{
    delete_oauth_secrets, delete_secrets, prune_secrets, redact, save_config, store_secrets,
    validate_key, validate_server, validate_unique, ApiKey, KeyMode, McpServer, McpTransport,
    SecretMap, DEFAULT_KEY_ID,
};
use gateway::{endpoint_url, AppInner};
use serde::Serialize;
use std::path::PathBuf;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::Manager;
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_autostart::ManagerExt;
use uuid::Uuid;

/// Bundle id from `tauri.conf.json` — must match GUI `app_config_dir()`.
const APP_IDENTIFIER: &str = "com.sumeru.app";

/// Inputs: none. Outputs: same path as Tauri `app_config_dir()/sumeru`, or error.
fn sumeru_config_dir() -> Result<PathBuf, String> {
    let base = dirs::config_dir().ok_or_else(|| {
        "could not resolve app config directory (no HOME / platform config path)".to_string()
    })?;
    Ok(base.join(APP_IDENTIFIER).join("sumeru"))
}

/// Inputs: none. Outputs: preferred path for Cursor stdio MCP (`~/.local/bin/sumeru` when present).
#[tauri::command]
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

/// Inputs: process args (`--key <name|id>` picks the API key scope; default: Default key).
/// Outputs: runs gateway tools over MCP stdio until EOF.
pub fn run_mcp_stdio() {
    let args: Vec<String> = std::env::args().collect();
    let key_arg = args
        .iter()
        .position(|a| a == "--key")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| DEFAULT_KEY_ID.into());
    let dir = match sumeru_config_dir() {
        Ok(dir) => dir,
        Err(err) => {
            eprintln!("sumeru mcp-stdio: {err}");
            std::process::exit(1);
        }
    };
    if let Err(err) = std::fs::create_dir_all(&dir) {
        eprintln!("sumeru mcp-stdio: failed to create config directory: {err}");
        std::process::exit(1);
    }
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    rt.block_on(async move {
        let inner = match AppInner::for_stdio(dir) {
            Ok(inner) => Arc::new(inner),
            Err(err) => {
                eprintln!("sumeru mcp-stdio: {err}");
                std::process::exit(1);
            }
        };
        let key = inner.config.read().await.find_key(&key_arg).map(|k| k.id.clone());
        let Some(key) = key else {
            eprintln!("sumeru mcp-stdio: unknown key {key_arg:?}");
            std::process::exit(1);
        };
        if let Err(err) = gateway::serve_stdio(inner, key).await {
            eprintln!("sumeru mcp-stdio: {err}");
            std::process::exit(1);
        }
    });
}

/// Inputs: none. Outputs: GLib default log handler that drops ayatana deprecation noise.
///
/// Tauri's Linux tray still uses libayatana-appindicator; the library logs a
/// deprecation warning on every tray create. Filter only that message.
#[cfg(target_os = "linux")]
fn silence_ayatana_deprecation() {
    glib::log_set_default_handler(|domain, level, message| {
        if domain == Some("libayatana-appindicator") && message.contains("is deprecated") {
            return;
        }
        glib::log_default_handler(domain, level, Some(message));
    });
}

/// Inputs: app handle. Outputs: main window shown and focused when present.
fn show_main(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[derive(Clone)]
struct State(Arc<AppInner>);

#[derive(Serialize)]
struct StatusDto {
    running: bool,
    endpoint: String,
}

#[derive(Serialize)]
struct ServerDto {
    id: String,
    name: String,
    enabled: bool,
    transport: McpTransport,
}

/// Inputs: app state. Outputs: funnel running flag and endpoint URL.
#[tauri::command]
async fn get_status(state: tauri::State<'_, State>) -> Result<StatusDto, String> {
    let local = state.0.running.load(std::sync::atomic::Ordering::SeqCst);
    let running = if local {
        true
    } else {
        gateway::existing_endpoint_healthy(&state.0.default_token().await).await
    };
    Ok(StatusDto {
        running,
        endpoint: endpoint_url().into(),
    })
}

/// Inputs: app state. Outputs: configured MCP servers (no secret values).
#[tauri::command]
async fn list_servers(state: tauri::State<'_, State>) -> Result<Vec<ServerDto>, String> {
    let cfg = state.0.config.read().await;
    Ok(cfg
        .servers
        .iter()
        .map(|s| ServerDto {
            id: s.id.clone(),
            name: s.name.clone(),
            enabled: s.enabled,
            transport: s.transport.clone(),
        })
        .collect())
}

/// Inputs: optional id, name, enabled, transport, secrets. Outputs: saved server id.
#[tauri::command]
async fn upsert_server(
    state: tauri::State<'_, State>,
    id: Option<String>,
    name: String,
    enabled: bool,
    transport: McpTransport,
    secrets: SecretMap,
) -> Result<String, String> {
    let id = id
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let mut transport = transport;
    if let McpTransport::Http { has_bearer, .. } = &mut transport {
        let keep_bearer = !*has_bearer
            && secrets.bearer.as_ref().is_none_or(|b| b.is_empty())
            && config::get_bearer_secret(&id).is_some();
        if keep_bearer {
            *has_bearer = true;
        }
    }
    let server = McpServer {
        id: id.clone(),
        name,
        enabled,
        transport,
    };
    validate_server(&server).map_err(|e| e.to_string())?;

    let mut staged = state.0.config.read().await.clone();
    let previous = staged.servers.iter().find(|s| s.id == id).cloned();
    validate_unique(&staged, &server, previous.is_some()).map_err(|e| e.to_string())?;
    if let Some(existing) = staged.servers.iter_mut().find(|s| s.id == id) {
        *existing = server.clone();
    } else {
        staged.servers.push(server.clone());
    }
    save_config(&state.0.dir, &staged).map_err(|e| e.to_string())?;
    store_secrets(&id, &secrets).map_err(|e| e.to_string())?;
    if let Some(prev) = &previous {
        prune_secrets(prev, &server).map_err(|e| e.to_string())?;
    }

    let mut cfg = state.0.config.write().await;
    *cfg = staged;
    state.0.pool.invalidate(&id).await;
    Ok(id)
}

/// Inputs: mcp id. Outputs: unit after removal.
#[tauri::command]
async fn remove_server(state: tauri::State<'_, State>, id: String) -> Result<(), String> {
    let mut staged = state.0.config.read().await.clone();
    let Some(pos) = staged.servers.iter().position(|s| s.id == id) else {
        return Err("unknown mcp".into());
    };
    let removed = staged.servers.remove(pos);
    for key in &mut staged.keys {
        key.mcp_ids.retain(|m| *m != id);
    }
    save_config(&state.0.dir, &staged).map_err(|e| e.to_string())?;
    {
        let mut cfg = state.0.config.write().await;
        *cfg = staged;
    }
    state.0.pool.invalidate(&id).await;
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

/// Inputs: none. Outputs: unit when funnel is listening.
#[tauri::command]
async fn start_funnel(state: tauri::State<'_, State>) -> Result<(), String> {
    gateway::start(state.0.clone()).await
}

/// Inputs: none. Outputs: unit when funnel has stopped.
#[tauri::command]
async fn stop_funnel(state: tauri::State<'_, State>) -> Result<(), String> {
    gateway::stop(state.0.clone()).await
}

#[derive(Serialize)]
struct KeyDto {
    #[serde(flatten)]
    key: ApiKey,
    token: String,
}

/// Inputs: app state. Outputs: API keys with their bearer tokens (for agent setup).
#[tauri::command]
async fn list_keys(state: tauri::State<'_, State>) -> Result<Vec<KeyDto>, String> {
    let tokens = state.0.tokens.read().await;
    let token = |id: &str| tokens.iter().find(|(k, _)| k == id).map(|(_, t)| t.clone());
    let cfg = state.0.config.read().await;
    Ok(cfg
        .keys
        .iter()
        .map(|k| KeyDto {
            key: k.clone(),
            token: token(&k.id).unwrap_or_default(),
        })
        .collect())
}

/// Inputs: optional id, name, scope mode, MCP ids. Outputs: saved key id (new keys get a token).
#[tauri::command]
async fn upsert_key(
    state: tauri::State<'_, State>,
    id: Option<String>,
    name: String,
    mode: KeyMode,
    mcp_ids: Vec<String>,
) -> Result<String, String> {
    let key = ApiKey {
        id: id
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| Uuid::new_v4().to_string()),
        name: name.trim().into(),
        mode,
        mcp_ids,
    };
    let mut staged = state.0.config.read().await.clone();
    validate_key(&staged, &key).map_err(|e| e.to_string())?;
    match staged.keys.iter_mut().find(|k| k.id == key.id) {
        Some(existing) => *existing = key.clone(),
        None => staged.keys.push(key.clone()),
    }
    save_config(&state.0.dir, &staged).map_err(|e| e.to_string())?;
    *state.0.config.write().await = staged;
    state.0.reload_tokens().await?;
    Ok(key.id)
}

/// Inputs: key id. Outputs: unit after deleting the key and its token (Default stays).
#[tauri::command]
async fn remove_key(state: tauri::State<'_, State>, id: String) -> Result<(), String> {
    if id == DEFAULT_KEY_ID {
        return Err("the Default key cannot be deleted".into());
    }
    let mut staged = state.0.config.read().await.clone();
    staged.keys.retain(|k| k.id != id);
    save_config(&state.0.dir, &staged).map_err(|e| e.to_string())?;
    *state.0.config.write().await = staged;
    config::delete_key_token(&id).map_err(|e| e.to_string())?;
    state.0.reload_tokens().await
}

/// Inputs: key id. Outputs: the key's new token (the old one stops working immediately).
#[tauri::command]
async fn rotate_key(state: tauri::State<'_, State>, id: String) -> Result<String, String> {
    let token = config::rotate_key_token(&id).map_err(|e| e.to_string())?;
    state.0.reload_tokens().await?;
    Ok(token)
}

/// Inputs: app handle. Outputs: whether launch-at-login is enabled.
#[tauri::command]
fn get_autostart(app: tauri::AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

/// Inputs: desired enabled flag. Outputs: unit after OS autostart is updated.
#[tauri::command]
fn set_autostart(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let auto = app.autolaunch();
    if enabled {
        auto.enable()
    } else {
        auto.disable()
    }
    .map_err(|e| e.to_string())
}

/// Inputs: http(s) URL or Cursor/VS Code MCP install deeplink. Outputs: unit after opening it.
#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    let parsed = url::Url::parse(&url).map_err(|e| e.to_string())?;
    if !matches!(parsed.scheme(), "http" | "https" | "cursor" | "vscode") {
        return Err("only http(s), cursor, and vscode URLs can be opened".into());
    }
    open::that(url).map_err(|e| e.to_string())
}

/// Inputs: app handle. Outputs: local VitePress docs URL after starting the static server.
#[tauri::command]
async fn open_docs(app: tauri::AppHandle) -> Result<String, String> {
    docs_server::ensure_and_open(&app).await
}

/// Inputs: mcp id. Outputs: Ok message or connection error.
#[tauri::command]
async fn test_server(state: tauri::State<'_, State>, id: String) -> Result<String, String> {
    let server = state
        .0
        .find(&id)
        .await
        .ok_or_else(|| "unknown mcp".to_string())?;
    let tools = state
        .0
        .pool
        .list_tools(&server)
        .await
        .map_err(|e| redact(&e.to_string()))?;
    Ok(format!("connected ({} tools)", tools.len()))
}

/// Inputs: MCP URL and optional mcp id. Outputs: whether browser OAuth is available.
#[tauri::command]
async fn probe_mcp_auth(url: String, id: Option<String>) -> Result<oauth::AuthProbe, String> {
    oauth::probe(&url, id.as_deref()).await
}

/// Inputs: MCP URL, mcp id, optional client credentials. Outputs: unit after browser sign-in.
/// Forces a fresh provider login when another MCP already uses the URL (another account).
#[tauri::command]
async fn start_mcp_oauth(
    state: tauri::State<'_, State>,
    url: String,
    id: String,
    client_id: Option<String>,
    client_secret: Option<String>,
) -> Result<(), String> {
    let fresh_login = state.0.config.read().await.servers.iter().any(|s| {
        s.id != id && matches!(&s.transport, McpTransport::Http { url: u, .. } if *u == url)
    });
    oauth::authorize(&url, &id, client_id, client_secret, fresh_login).await?;
    state.0.pool.invalidate(&id).await;
    Ok(())
}

/// Inputs: unsaved transport + secrets. Outputs: Ok message when a temporary connect works.
#[tauri::command]
async fn test_draft(
    state: tauri::State<'_, State>,
    name: String,
    transport: McpTransport,
    secrets: SecretMap,
) -> Result<String, String> {
    let id = format!("draft-{}", Uuid::new_v4());
    let server = McpServer {
        id: id.clone(),
        name,
        enabled: true,
        transport: transport.clone(),
    };
    validate_server(&server).map_err(|e| e.to_string())?;
    store_secrets(&id, &secrets).map_err(|e| e.to_string())?;
    let result = state.0.pool.list_tools(&server).await;
    state.0.pool.invalidate(&id).await;
    match &transport {
        McpTransport::Stdio { env_keys, .. } => {
            delete_secrets(&id, env_keys, &[], false).map_err(|e| e.to_string())?
        }
        McpTransport::Http {
            header_keys,
            has_bearer,
            ..
        } => delete_secrets(&id, &[], header_keys, *has_bearer).map_err(|e| e.to_string())?,
    }
    result
        .map(|t| format!("connected ({} tools)", t.len()))
        .map_err(|e| redact(&e.to_string()))
}

/// Inputs: none. Outputs: runs the Tauri desktop app (tray, autostart, gateway).
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .setup(|app| {
            let dir = app
                .path()
                .app_config_dir()
                .expect("app config dir")
                .join("sumeru");
            std::fs::create_dir_all(&dir)?;
            let inner = Arc::new(AppInner::new(dir).map_err(|e| e.to_string())?);
            app.manage(State(inner.clone()));
            tauri::async_runtime::spawn(async move {
                if let Err(err) = gateway::start(inner).await {
                    eprintln!("sumeru auto-start failed: {err}");
                }
            });

            #[cfg(target_os = "linux")]
            silence_ayatana_deprecation();

            let open = MenuItem::with_id(app, "open", "Open", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &quit])?;
            let mut tray = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("sumeru")
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "open" => show_main(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;

            if std::env::args().any(|a| a == "--hidden") {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            list_servers,
            upsert_server,
            remove_server,
            start_funnel,
            stop_funnel,
            list_keys,
            upsert_key,
            remove_key,
            rotate_key,
            get_autostart,
            set_autostart,
            open_url,
            open_docs,
            probe_mcp_auth,
            start_mcp_oauth,
            test_server,
            test_draft,
            mcp_stdio_command,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| match &event {
            tauri::RunEvent::WindowEvent {
                label,
                event: tauri::WindowEvent::CloseRequested { api, .. },
                ..
            } if label == "main" => {
                api.prevent_close();
                if let Some(window) = app_handle.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
            tauri::RunEvent::ExitRequested { api, .. } => {
                let Some(state) = app_handle.try_state::<State>() else {
                    return;
                };
                if state
                    .0
                    .exiting
                    .swap(true, std::sync::atomic::Ordering::SeqCst)
                {
                    return;
                }
                api.prevent_exit();
                let handle = app_handle.clone();
                tauri::async_runtime::spawn(async move {
                    if let Some(state) = handle.try_state::<State>() {
                        let _ = gateway::stop(state.0.clone()).await;
                    }
                    handle.exit(0);
                });
            }
            _ => {}
        });
}
