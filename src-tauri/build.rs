/// Inputs: Cargo features. Outputs: Tauri codegen for `gui` builds; nothing for headless ones.
fn main() {
    #[cfg(feature = "gui")]
    tauri_build::build()
}
