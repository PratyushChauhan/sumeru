//! Tauri desktop app (Linux; macOS before the native app): window, tray, autostart.
//! Built only with the `gui` feature; app commands go through `api::dispatch`.

use std::sync::Arc;

use serde_json::Value;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::Manager;
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

use crate::gateway::{self, AppInner};
use crate::{api, docs_server};

#[derive(Clone)]
struct State(Arc<AppInner>);

/// Inputs: command name and JSON params from `src/lib/api.js`. Outputs: the command's JSON result.
#[tauri::command]
async fn api(
    state: tauri::State<'_, State>,
    method: String,
    params: Option<Value>,
) -> Result<Value, String> {
    api::dispatch(&state.0, &method, params.unwrap_or(Value::Null)).await
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

/// Inputs: app handle. Outputs: local VitePress docs URL after starting the static server.
#[tauri::command]
async fn open_docs(app: tauri::AppHandle) -> Result<String, String> {
    docs_server::ensure_and_open(&app).await
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

/// Inputs: none. Outputs: runs the Tauri desktop app (tray, autostart, gateway).
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .setup(|app| {
            let dir = crate::sumeru_config_dir()?;
            std::fs::create_dir_all(&dir)?;
            let inner = Arc::new(AppInner::new(dir).map_err(|e| e.to_string())?);
            app.manage(State(inner.clone()));
            // Keychain reads can prompt; do them here, not on the main thread, so the window shows.
            tauri::async_runtime::spawn(api::autostart_gateway(inner));

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
            api,
            get_autostart,
            set_autostart,
            open_docs
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
