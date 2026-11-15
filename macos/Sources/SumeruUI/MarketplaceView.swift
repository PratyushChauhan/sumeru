import SwiftUI
import SumeruKit

/// Curated MCPs: Install (browser sign-in, or an API key for key-based providers), or Add account.
struct MarketplaceView: View {
    let model: AppModel
    @State private var query = ""
    @State private var adding: MarketplaceEntry?
    @State private var accountName = ""
    @State private var apiKey = ""
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
            let first = installedServers(e, model.servers).isEmpty
            Form {
                Section {
                    TextField("Account name", text: $accountName)
                    if e.apiKeyUrl != nil { SecureField("\(e.name) API key", text: $apiKey) }
                } header: {
                    Text(e.apiKeyUrl != nil && first ? "Connect \(e.name)" : "Add another \(e.name) account")
                } footer: {
                    VStack(alignment: .leading, spacing: 4) {
                        if let url = e.apiKeyUrl.flatMap(URL.init(string:)) {
                            Link("Create a key in the \(e.name) dashboard", destination: url)
                        }
                        Text(addError ?? (e.apiKeyUrl != nil ? "Stored in your keychain after a test connect succeeds." : "If sign-in returns the same account, sign out of \(e.name) in your browser first, or use another browser profile."))
                            .foregroundStyle(addError == nil ? Color.secondary : Color.red)
                    }
                }
            }
            .formStyle(.grouped)
            .frame(width: 420)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { adding = nil } }
                ToolbarItem(placement: .confirmationAction) {
                    Button(e.apiKeyUrl != nil ? "Connect" : "Sign In & Add") { Task { await install(e, name: accountName) } }
                        .disabled(accountName.trimmingCharacters(in: .whitespaces).isEmpty || model.installing != nil
                                  || (e.apiKeyUrl != nil && apiKey.trimmingCharacters(in: .whitespaces).isEmpty))
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
                            Button("Add Account") { open(e) }
                        } else {
                            Button("Install") { if e.apiKeyUrl != nil { open(e) } else { Task { await install(e, name: e.name) } } }
                                .buttonStyle(.borderedProminent)
                        }
                    }
                    .disabled(model.installing != nil)
                }
            }
            .padding(4)
        }
    }

    /// Inputs: entry. Outputs: account sheet, named after the provider or the next free "<Provider> N".
    private func open(_ e: MarketplaceEntry) {
        addError = nil
        apiKey = ""
        accountName = installedServers(e, model.servers).isEmpty ? e.name : nextAccountName(e, model.servers)
        adding = e
    }

    /// Inputs: entry, account name. Outputs: installed (sheet closed) or the error shown.
    private func install(_ e: MarketplaceEntry, name: String) async {
        if let err = await model.install(e, name: name.trimmingCharacters(in: .whitespaces), apiKey: apiKey) {
            if adding != nil { addError = err } else { model.error = err }
        } else {
            adding = nil
        }
    }
}
