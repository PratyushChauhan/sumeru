// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(all(target_os = "linux", feature = "gui"))]
mod linux_wayland;

/// Inputs: process args. Outputs: stdio MCP (`mcp-stdio`), the JSON engine for the native
/// macOS app (`engine`), else the Tauri desktop UI (only in `gui` builds).
fn main() {
    if std::env::args().any(|a| a == "mcp-stdio") {
        return sumeru_lib::run_mcp_stdio();
    }
    if std::env::args().nth(1).as_deref() == Some("engine") {
        return sumeru_lib::run_engine();
    }
    #[cfg(feature = "gui")]
    {
        #[cfg(target_os = "linux")]
        linux_wayland::maybe_reexec_with_system_wayland();
        sumeru_lib::run()
    }
    #[cfg(not(feature = "gui"))]
    {
        eprintln!("sumeru: headless build; run `sumeru mcp-stdio` or `sumeru engine`");
        std::process::exit(2);
    }
}
