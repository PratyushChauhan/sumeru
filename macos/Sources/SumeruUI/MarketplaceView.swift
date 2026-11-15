import SwiftUI
import SumeruKit

/// Curated OAuth MCPs: Install (browser sign-in), or Add account when one is already installed.
struct MarketplaceView: View {
    let model: AppModel
    @State private var query = ""
    @State private var adding: MarketplaceEntry?
    @State private var accountName = ""
    @State private var addError: String?

    private var entries: [MarketplaceEntry] {
        let q = query.trimmingCharacters(in: .whitespaces).lowercased()
        return q.isEmpty ? model.catalog : model.catalog.filter {
            "\($0.name) \($0.description) \($0.url)".lowercased().contains(q)
        }
    }

    var body: some View {
        ScrollView {
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 260), spacing: 12)], spacing: 12) {
                ForEach(entries) { e in card(e) }
            }
            .padding()
        }
        .searchable(text: $query, placement: .toolbar)
        .sheet(item: $adding) { e in
            Form {
                Section {
                    TextField("Account name", text: $accountName)
                } header: {
                    Text("Add another \(e.name) account")
                } footer: {
                    Text(addError ?? "If sign-in returns the same account, sign out of \(e.name) in your browser first, or use another browser profile.")
                        .foregroundStyle(addError == nil ? Color.secondary : Color.red)
                }
            }
            .formStyle(.grouped)
            .frame(width: 420)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { adding = nil } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Sign In & Add") { Task { await install(e, name: accountName) } }
                        .disabled(accountName.trimmingCharacters(in: .whitespaces).isEmpty || model.installing != nil)
                }
            }
        }
    }

    private func card(_ e: MarketplaceEntry) -> some View {
        let count = installedServers(e, model.servers).count
        return GroupBox {
            HStack(alignment: .top, spacing: 10) {
                ProviderIcon(id: e.id, name: e.name)
                VStack(alignment: .leading, spacing: 4) {
                    Text(e.name).font(.headline)
                    Text(e.description).font(.caption).foregroundStyle(.secondary).lineLimit(2, reservesSpace: true)
                    HStack {
                        if count > 0 { Label("\(count) installed", systemImage: "checkmark.circle.fill").font(.caption).foregroundStyle(.green) }
                        Spacer()
                        if model.installing == e.id { ProgressView().controlSize(.small) }
                        if count > 0 {
                            Button("Add Account") { addError = nil; accountName = nextAccountName(e, model.servers); adding = e }
                        } else {
                            Button("Install") { Task { await install(e, name: e.name) } }.buttonStyle(.borderedProminent)
                        }
                    }
                    .disabled(model.installing != nil)
                }
            }
            .padding(4)
        }
    }

    /// Inputs: entry, account name. Outputs: installed (sheet closed) or the error shown.
    private func install(_ e: MarketplaceEntry, name: String) async {
        if let err = await model.install(e, name: name.trimmingCharacters(in: .whitespaces)) {
            if adding != nil { addError = err } else { model.error = err }
        } else {
            adding = nil
        }
    }
}
