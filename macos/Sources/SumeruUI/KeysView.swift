import SwiftUI
import SumeruKit

/// Endpoint API keys: copy, rotate, edit scope, delete (not Default).
struct KeysView: View {
    let model: AppModel
    @State private var editing: KeyTarget?

    struct KeyTarget: Identifiable {
        let key: ApiKey?
        var id: String { key?.id ?? "new" }
    }

    var body: some View {
        List(model.keys) { k in
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text(k.name)
                    Text("\(scopeLabel(k, model.servers)) · installs as \(agentServerName(k))")
                        .font(.caption).foregroundStyle(.secondary)
                }
                Spacer()
                Button("Copy Token") { model.copy(k.token) }.disabled(k.token.isEmpty)
                Menu {
                    Button("Edit Scope…") { editing = KeyTarget(key: k) }
                    Button("Rotate Token") { Task { await model.rotateKey(k) } }
                    if !k.isDefault {
                        Divider()
                        Button("Delete", role: .destructive) { Task { await model.removeKey(k) } }
                    }
                } label: { Image(systemName: "ellipsis.circle") }
                    .menuStyle(.borderlessButton).menuIndicator(.hidden).fixedSize()
            }
            .padding(.vertical, 4)
        }
        .toolbar { ToolbarItem { Button("Add Key", systemImage: "plus") { editing = KeyTarget(key: nil) } } }
        .sheet(item: $editing) { KeyEditor(model: model, key: $0.key) }
    }
}

/// Name + scope: which MCPs this key reaches, and whether MCPs added later are allowed or blocked.
struct KeyEditor: View {
    let model: AppModel
    let key: ApiKey?
    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var mode = KeyMode.exclude
    @State private var allowed: Set<String> = []

    var body: some View {
        Form {
            Section { TextField("Name", text: $name).disabled(key?.isDefault == true) }
            Section {
                Picker("New MCPs", selection: $mode) {
                    Text("Allowed").tag(KeyMode.exclude)
                    Text("Blocked").tag(KeyMode.include)
                }
                .pickerStyle(.segmented)
            } footer: {
                Text(mode == .exclude ? "MCPs you add later are reachable with this key." : "MCPs you add later stay blocked until you allow them here.")
                    .foregroundStyle(.secondary)
            }
            Section("Reachable MCPs") {
                if model.servers.isEmpty { Text("No MCPs configured yet.").foregroundStyle(.secondary) }
                ForEach(model.servers) { s in
                    Toggle(s.name, isOn: Binding(get: { allowed.contains(s.id) },
                                                 set: { if $0 { allowed.insert(s.id) } else { allowed.remove(s.id) } }))
                }
            }
        }
        .formStyle(.grouped)
        .frame(width: 440)
        .navigationTitle(key == nil ? "New Key" : "Edit Key")
        .toolbar {
            ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
            ToolbarItem(placement: .confirmationAction) {
                Button("Save") {
                    Task {
                        let ids = scopeIds(mode, allowed: allowed, servers: model.servers)
                        if await model.saveKey(id: key?.id, name: name.trimmingCharacters(in: .whitespaces), mode: mode, mcpIds: ids) { dismiss() }
                    }
                }
                .disabled(name.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
        .onAppear {
            guard let key else { allowed = Set(model.servers.map(\.id)); return }
            (name, mode) = (key.name, key.mode)
            allowed = Set(model.servers.filter { keyAllows(key, $0.id) }.map(\.id))
        }
    }
}
