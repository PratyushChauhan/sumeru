mod api;
mod config;
#[cfg(feature = "gui")]
mod desktop;
#[cfg(feature = "gui")]
mod docs_server;
mod gateway;
mod oauth;
mod pool;

use std::path::PathBuf;
use std::sync::Arc;

use config::DEFAULT_KEY_ID;
use gateway::AppInner;

#[cfg(feature = "gui")]
pub use desktop::run;

/// Bundle id from `tauri.conf.json` — must match GUI `app_config_dir()`.
const APP_IDENTIFIER: &str = "com.sumeru.app";

/// Inputs: none. Outputs: same path as Tauri `app_config_dir()/sumeru`, or error.
fn sumeru_config_dir() -> Result<PathBuf, String> {
    let base = dirs::config_dir().ok_or_else(|| {
        "could not resolve app config directory (no HOME / platform config path)".to_string()
    })?;
    Ok(base.join(APP_IDENTIFIER).join("sumeru"))
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
        let inner = match AppInner::new(dir) {
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

/// Inputs: none. Outputs: runs `sumeru engine` (gateway + JSON API on stdio, used by the
/// native macOS app) until stdin closes.
pub fn run_engine() {
    let dir = sumeru_config_dir().and_then(|d| {
        std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
        Ok(d)
    });
    let inner = dir.and_then(|d| AppInner::new(d).map_err(|e| e.to_string()));
    let inner = match inner {
        Ok(inner) => Arc::new(inner),
        Err(err) => {
            eprintln!("sumeru engine: {err}");
            std::process::exit(1);
        }
    };
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime")
        .block_on(api::serve_engine(inner));
}
