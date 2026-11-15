import SwiftUI
import SumeruKit

/// Install Sumeru into a coding agent: pick key, scope (user / this repo) and agent, then copy
/// the snippet or use the agent's one-click deeplink. Recipes come from `agentInstalls`.
struct ConnectView: View {
    let model: AppModel
    @State private var keyId = "default"
    @State private var repo = false
    @State private var agent = "claude-code"

    private var key: ApiKey? { model.keys.first { $0.id == keyId } ?? model.keys.first }
    private var env: String { repo ? tokenEnvVar(key) : "" }

    /// Inputs: token or placeholder. Outputs: recipes for the selected key and scope.
    private func recipes(_ token: String) -> [AgentRecipe] {
        agentInstalls(url: model.status.endpoint, token: token, server: agentServerName(key), env: env)
    }
    private var shown: AgentRecipe? {
        let list = recipes("<TOKEN>")
        return list.first { $0.id == agent } ?? list.first
    }
    private var real: AgentRecipe? { recipes(key?.token ?? "").first { $0.id == shown?.id } }

    var body: some View {
        Form {
            Section {
                if model.keys.count > 1 {
                    Picker("API key", selection: $keyId) { ForEach(model.keys) { Text($0.name).tag($0.id) } }
                }
                Picker("Install for", selection: $repo) {
                    Text("User").tag(false)
                    Text("This repo").tag(true)
                }
                .pickerStyle(.segmented)
                Picker("Agent", selection: $agent) { ForEach(recipes("")) { Text($0.name).tag($0.id) } }
            }
            if let shown {
                Section {
                    Text(shown.text).font(.callout.monospaced()).textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                } header: {
                    Text(shown.hint)
                } footer: {
                    HStack {
                        Spacer()
                        if let link = real?.link { Button("Add to \(shown.name)") { model.open(link) } }
                        Button("Copy") { model.copy(real?.text ?? "") }.buttonStyle(.borderedProminent)
                    }
                }
            }
            if repo && shown?.id != "vscode" {
                Section {
                    Text("export \(env)=<TOKEN>").font(.callout.monospaced())
                } header: {
                    Text("Set the token where the agent runs")
                } footer: {
                    VStack(alignment: .leading) {
                        Text("The repo config never holds the token. Put this in a gitignored .envrc (direnv) or your shell profile. Apps opened from the Dock may not see shell variables; start them from a terminal.")
                            .foregroundStyle(.secondary)
                        HStack { Spacer(); Button("Copy Export") { model.copy("export \(env)=\(key?.token ?? "")") } }
                    }
                }
            }
            if let shown {
                Section {
                    Text(agentPrompt(shown, env: env)).font(.callout.monospaced()).textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                } header: {
                    Text("Or paste this prompt into \(shown.name)")
                } footer: {
                    HStack { Spacer(); Button("Copy Prompt") { model.copy(agentPrompt(real ?? shown, env: env)) } }
                }
            }
        }
        .formStyle(.grouped)
        .onChange(of: repo) { if shown?.id != agent { agent = shown?.id ?? agent } }
    }
}
