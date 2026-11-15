import AppKit
import SwiftUI
import SumeruUI

/// Native macOS Sumeru: menu-bar extra + main window over the bundled Rust engine.
@main
struct SumeruApp: App {
    @State private var model = AppModel.live()
    /// Launched by the login item (`--hidden`): stay in the menu bar until opened.
    private let hidden = CommandLine.arguments.contains("--hidden")

    init() {
        AppModel.migrateTauriAutostart()
    }

    var body: some Scene {
        Window("Sumeru", id: "main") {
            MainWindow(model: model, hidden: hidden).frame(minWidth: 780, minHeight: 520)
        }
        .defaultSize(width: 900, height: 600)
        MenuBarExtra {
            MenuBarContent(model: model)
        } label: {
            Image(systemName: model.status.running ? "point.3.filled.connected.trianglepath.dotted" : "point.3.connected.trianglepath.dotted")
        }
    }
}

/// Main window wrapper: shows a Dock icon only while open; closes itself on a hidden launch.
struct MainWindow: View {
    let model: AppModel
    let hidden: Bool
    @Environment(\.dismissWindow) private var dismissWindow
    private static var launched = false

    var body: some View {
        MainView(model: model)
            .onAppear {
                if hidden && !Self.launched {
                    Self.launched = true
                    dismissWindow(id: "main")
                    return
                }
                Self.launched = true
                NSApp.setActivationPolicy(.regular)
                NSApp.activate(ignoringOtherApps: true)
            }
            .onDisappear { NSApp.setActivationPolicy(.accessory) }
    }
}
