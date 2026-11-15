import SwiftUI
import SumeruKit

/// Configured MCPs: enable switch, Test, Edit, Remove; Add opens the editor sheet.
struct ServersView: View {
    let model: AppModel
    @State private var editing: EditTarget?
    @State private var result: String?

    /// Editor sheet target: a new MCP, or an existing one.
    struct EditTarget: Identifiable {
        let server: Server?
        var id: String { server?.id ?? "new" }
    }

    var body: some View {
        List {
            ForEach(serverGroups(model.servers, model.catalog)) { g in
                AppGroup(group: g, detail: groupSummary(g.servers)) { ForEach(g.servers, content: row) }
            }
        }
        .overlay {
            if model.servers.isEmpty {
                ContentUnavailableView("No MCPs yet", systemImage: "server.rack",
                                       description: Text("Add a local command or an MCP URL, or install one from the Marketplace."))
            }
        }
        .toolbar { ToolbarItem { Button("Add MCP", systemImage: "plus") { editing = EditTarget(server: nil) } } }
        .sheet(item: $editing) { ServerEditor(model: model, server: $0.server) }
        .alert("Test", isPresented: Binding(get: { result != nil }, set: { if !$0 { result = nil } })) {
            Button("OK") {}
        } message: { Text(result ?? "") }
    }

    /// Inputs: server. Outputs: its row: enable switch, name + detail, Test, Edit/Remove menu.
    private func row(_ s: Server) -> some View {
        HStack(spacing: 12) {
            Toggle("Enabled", isOn: Binding(get: { s.enabled }, set: { v in Task { await model.setEnabled(s, v) } }))
                .toggleStyle(.switch).labelsHidden().controlSize(.small)
            VStack(alignment: .leading, spacing: 2) {
                Text(s.name)
                Text(Self.detail(s.transport)).font(.caption).foregroundStyle(.secondary).lineLimit(1).truncationMode(.middle)
            }
            Spacer()
            Button("Test") { Task { result = await model.test(s) } }
            Menu {
                Button("Edit…") { editing = EditTarget(server: s) }
                Button("Remove", role: .destructive) { Task { await model.remove(s) } }
            } label: { Image(systemName: "ellipsis.circle") }
                .menuStyle(.borderlessButton).menuIndicator(.hidden).fixedSize()
        }
        .padding(.vertical, 4)
    }

    /// Inputs: transport. Outputs: one-line summary (URL, or command + args).
    static func detail(_ t: Transport) -> String {
        switch t {
        case let .http(url, _, _): url
        case let .stdio(command, args, _): ([command] + args).joined(separator: " ")
        }
    }
}

/// Add/edit an MCP: paste a URL or a command; HTTP URLs get an OAuth sign-in row.
struct ServerEditor: View {
    let model: AppModel
    let server: Server?
    @Environment(\.dismiss) private var dismiss
    @State private var editId = ""
    @State private var name = ""
    @State private var enabled = true
    @State private var source = ""
    @State private var argsText = ""
    @State private var envText = ""
    @State private var headersText = ""
    @State private var bearer = ""
    @State private var clientId = ""
    @State private var clientSecret = ""
    @State private var probe: AuthProbe?
    @State private var connected = false
    @State private var message = ""
    /// Source as loaded; editing it away drops the OAuth connection (new URL, new sign-in).
    @State private var original = ""

    private var http: Bool { isHttpSource(source) }

    var body: some View {
        Form {
            Section {
                TextField("Name", text: $name)
                Toggle("Enabled", isOn: $enabled)
                TextField("Command or MCP URL", text: $source, prompt: Text("https://mcp.slack.com/mcp or npx …"))
                    .onChange(of: source) { if source != original { connected = false } }
            } footer: {
                Text(source.isEmpty ? "Paste a URL or a local command" : http ? "Detected HTTP MCP" : "Detected stdio command")
                    .foregroundStyle(.secondary)
            }
            if !source.isEmpty && !http {
                Section("Arguments (one per line)") { TextEditor(text: $argsText).font(.body.monospaced()).frame(height: 60) }
                Section("Environment (KEY=value)") { TextEditor(text: $envText).font(.body.monospaced()).frame(height: 60) }
            }
            if http, let probe, probe.oauth {
                Section("Sign-in") {
                    LabeledContent(connected ? "Connected" : probe.supportsDcr ? "Sign in registers automatically" : "Add a Client ID under Advanced, then sign in") {
                        Button(connected ? "Reconnect" : probe.label) { Task { await signIn() } }
                    }
                }
            }
            if http {
                Section("Advanced") {
                    SecureField("Bearer token", text: $bearer)
                    TextField("OAuth Client ID", text: $clientId)
                    SecureField("OAuth Client Secret", text: $clientSecret)
                    LabeledContent("Headers (KEY=value)") { TextEditor(text: $headersText).font(.body.monospaced()).frame(height: 44) }
                }
            }
            if !message.isEmpty { Text(message).foregroundStyle(.secondary).textSelection(.enabled) }
        }
        .formStyle(.grouped)
        .frame(width: 520)
        .navigationTitle(server == nil ? "Add MCP" : "Edit MCP")
        .toolbar {
            ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
            ToolbarItem { Button("Test") { Task { await test() } }.disabled(source.isEmpty) }
            ToolbarItem(placement: .confirmationAction) { Button("Save") { Task { await save() } }.disabled(name.isEmpty || source.isEmpty) }
        }
        .onAppear(perform: load)
        .task(id: source) {
            // Debounced OAuth probe for HTTP sources (like the 250 ms probe in Editor.svelte).
            guard http else { probe = nil; return }
            try? await Task.sleep(for: .milliseconds(250))
            guard !Task.isCancelled else { return }
            probe = await model.probe(url: source.trimmingCharacters(in: .whitespaces), id: editId.isEmpty ? nil : editId)
            if probe?.connected == true { connected = true }
        }
    }

    /// Inputs: none. Outputs: fields filled from the server being edited (secrets are never shown).
    private func load() {
        guard let s = server else { return }
        (editId, name, enabled) = (s.id, s.name, s.enabled)
        switch s.transport {
        case let .http(url, headerKeys, hasBearer):
            (source, original) = (url, url)
            headersText = headerKeys.map { "\($0)=" }.joined(separator: "\n")
            connected = hasBearer
        case let .stdio(command, args, envKeys):
            source = command
            argsText = args.joined(separator: "\n")
            envText = envKeys.map { "\($0)=" }.joined(separator: "\n")
        }
    }

    private func draft() throws -> (Transport, Secrets) {
        try readDraft(source: source, headersText: headersText, bearer: bearer, envText: envText, argsText: argsText, oauthConnected: connected)
    }

    private func ensureId() -> String {
        if editId.isEmpty { editId = UUID().uuidString.lowercased() }
        return editId
    }

    private func signIn() async {
        message = "Waiting for browser sign-in…"
        let trim = { (s: String) in s.trimmingCharacters(in: .whitespaces).isEmpty ? nil : s.trimmingCharacters(in: .whitespaces) }
        if let err = await model.signIn(url: source.trimmingCharacters(in: .whitespaces), id: ensureId(),
                                        clientId: trim(clientId), clientSecret: trim(clientSecret)) {
            message = err
        } else {
            connected = true
            message = "Signed in"
        }
    }

    private func test() async {
        do {
            let (t, s) = try draft()
            message = "Testing…"
            message = await model.testDraft(name: name.isEmpty ? "draft" : name, transport: t, secrets: s)
        } catch { message = error.localizedDescription }
    }

    private func save() async {
        do {
            let (t, s) = try draft()
            let id = connected ? ensureId() : (editId.isEmpty ? nil : editId)
            if await model.save(id: id, name: name.trimmingCharacters(in: .whitespaces), enabled: enabled, transport: t, secrets: s) {
                dismiss()
            }
        } catch { message = error.localizedDescription }
    }
}
