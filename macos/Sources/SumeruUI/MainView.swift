import AppKit
import SwiftUI
import SumeruKit

/// Sidebar panes of the main window.
public enum Pane: String, CaseIterable, Identifiable {
    case mcps = "MCPs", marketplace = "Marketplace", keys = "API Keys", connect = "Connect an Agent", general = "General"
    public var id: Self { self }
    var icon: String {
        switch self {
        case .mcps: "server.rack"
        case .marketplace: "square.grid.2x2"
        case .keys: "key"
        case .connect: "terminal"
        case .general: "gearshape"
        }
    }
}

/// Main window: sidebar of panes (reopens on the last one), funnel status + Start/Stop in the toolbar.
public struct MainView: View {
    @Bindable var model: AppModel
    @AppStorage("pane") private var section: Pane = .mcps

    /// Inputs: app model. Outputs: the main window content.
    public init(model: AppModel) { self.model = model }

    public var body: some View {
        NavigationSplitView {
            List(Pane.allCases, selection: Binding<Pane?>(get: { section }, set: { if let p = $0 { section = p } })) { s in
                Label(s.rawValue, systemImage: s.icon).tag(s)
            }
            .navigationSplitViewColumnWidth(min: 180, ideal: 200)
        } detail: {
            Group {
                switch section {
                case .mcps: ServersView(model: model)
                case .marketplace: MarketplaceView(model: model)
                case .keys: KeysView(model: model)
                case .connect: ConnectView(model: model)
                case .general: GeneralView(model: model)
                }
            }
            .navigationTitle(section.rawValue)
            .toolbar {
                // Status light in its own glass capsule; the spacer stops macOS 26 grouping it with Start/Stop.
                ToolbarItem(placement: .primaryAction) { StatusLight(running: model.status.running) }
                if #available(macOS 26, *) { ToolbarSpacer(.fixed, placement: .primaryAction) }
                ToolbarItem(placement: .primaryAction) { FunnelToggle(model: model) }
            }
        }
        .alert("Sumeru", isPresented: Binding(get: { model.error != nil }, set: { if !$0 { model.error = nil } })) {
            Button("OK") { model.error = nil }
        } message: { Text(model.error ?? "") }
    }
}

/// Inputs: running flag. Outputs: green (running) or grey (stopped) light, labelled for VoiceOver.
struct StatusLight: View {
    let running: Bool
    var body: some View {
        Circle().fill(running ? Color.green : Color.secondary).frame(width: 9, height: 9)
            .frame(width: 36, height: 22) // toolbar glass adds height only; 36 pt wide makes it round like the + button
            .help(running ? "Funnel running" : "Funnel stopped")
            .accessibilityLabel(running ? "Funnel running" : "Funnel stopped")
    }
}

/// Status text + Start/Stop for the toolbar.
struct FunnelToggle: View {
    let model: AppModel
    var body: some View {
        HStack(spacing: 8) {
            Text(model.status.running ? "Running" : "Stopped").foregroundStyle(.secondary)
            Button(model.status.running ? "Stop" : "Start") { Task { await model.toggleFunnel() } }
        }
    }
}

/// Menu-bar extra: status, endpoint copy, Start/Stop, open window, quit.
public struct MenuBarContent: View {
    let model: AppModel
    @Environment(\.openWindow) private var openWindow

    public init(model: AppModel) { self.model = model }

    public var body: some View {
        Text(model.status.running ? "Sumeru is running" : "Sumeru is stopped")
        Button("Copy Endpoint URL") { model.copy(model.status.endpoint) }
        Button(model.status.running ? "Stop Funnel" : "Start Funnel") { Task { await model.toggleFunnel() } }
        Divider()
        Button("Open Sumeru…") {
            openWindow(id: "main")
            NSApp.activate(ignoringOtherApps: true)
        }
        .keyboardShortcut(",")
        Divider()
        Button("Quit Sumeru") { NSApp.terminate(nil) }.keyboardShortcut("q")
    }
}

/// Endpoint URL, launch at login, docs.
struct GeneralView: View {
    @Bindable var model: AppModel
    var body: some View {
        Form {
            Section("Endpoint") {
                LabeledContent("URL") {
                    HStack {
                        Text(model.status.endpoint).textSelection(.enabled).font(.body.monospaced())
                        Button("Copy") { model.copy(model.status.endpoint) }
                    }
                }
                LabeledContent("Status", value: model.status.running ? "Running" : "Stopped")
            }
            Section {
                Toggle("Open at login", isOn: $model.openAtLogin)
            } footer: {
                Text("Starts Sumeru in the menu bar after you log in.").foregroundStyle(.secondary)
            }
            Section {
                Link("Documentation", destination: URL(string: "https://pratyushchauhan.github.io/sumeru/")!)
            }
        }
        .formStyle(.grouped)
    }
}

/// Inputs: Marketplace id, display name. Outputs: provider icon, or an initial badge.
struct ProviderIcon: View {
    let id: String
    let name: String
    var body: some View {
        if let image = Self.image(id) {
            Image(nsImage: image).resizable().scaledToFit().padding(3)
                .frame(width: 28, height: 28).background(.white, in: .rect(cornerRadius: 6))
                .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(.black.opacity(0.1)))
        } else {
            Text(name.prefix(1)).font(.headline).frame(width: 28, height: 28)
                .background(.quaternary, in: .rect(cornerRadius: 6))
        }
    }

    /// Inputs: id. Outputs: icon from the app bundle, or the repo's assets when run unbundled.
    static func image(_ id: String) -> NSImage? {
        let dirs = [Bundle.main.resourceURL?.appending(path: "marketplace"),
                    URL(filePath: #filePath).deletingLastPathComponent().appending(path: "../../../src/assets/marketplace")]
        for dir in dirs.compactMap({ $0 }) {
            for ext in ["png", "svg"] {
                if let img = NSImage(contentsOf: dir.appending(path: "\(id).\(ext)")) { return img }
            }
        }
        return nil
    }
}

