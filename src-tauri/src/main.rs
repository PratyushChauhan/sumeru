// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(all(target_os = "linux", feature = "gui"))]
mod linux_wayland;

/// Inputs: process args (`SUMERU_SECRETS=memory` keeps secrets off the keychain). Outputs: the
/// CLI (`servers …`, `keys …`), stdio MCP (`mcp-stdio`), the JSON engine for the native macOS
/// app (`engine`), else the Tauri desktop UI (only in `gui` builds).
fn main() {
    if std::env::var("SUMERU_SECRETS").as_deref() == Ok("memory") {
        sumeru_lib::use_memory_secrets();
    }
    let command = std::env::args().skip(1).find(|a| !a.starts_with('-')); // `--json` may lead
    if matches!(command.as_deref(), Some("servers" | "keys")) {
        let args: Vec<String> = std::env::args().skip(1).collect();
        std::process::exit(sumeru_lib::run_cli(&args));
    }
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
        eprintln!("sumeru: headless build; run `sumeru mcp-stdio`, `sumeru servers`, `sumeru keys` or `sumeru engine`");
        std::process::exit(2);
    }
}
