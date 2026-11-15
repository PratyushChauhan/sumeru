import AppKit
import Foundation
import Observation
import ServiceManagement
import SumeruKit

/// Parameter payloads for `api::dispatch` (camelCase top-level, snake_case nested — as api.js sends).
struct IdParams: Encodable, Sendable { let id: String }
struct ServerParams: Encodable, Sendable {
    let id: String?, name: String, enabled: Bool, transport: Transport, secrets: Secrets
}
struct DraftParams: Encodable, Sendable { let name: String, transport: Transport, secrets: Secrets }
struct KeyParams: Encodable, Sendable { let id: String?, name: String, mode: KeyMode, mcpIds: [String] }
struct ProbeParams: Encodable, Sendable { let url: String, id: String? }
struct OauthParams: Encodable, Sendable { let url: String, id: String, clientId: String?, clientSecret: String? }

/// App state and every user action. Views read it; failures land in `error` for an alert.
@MainActor @Observable
public final class AppModel {
    public let engine: any Engine
    public let catalog: [MarketplaceEntry]
    public var status = Status(running: false, endpoint: "http://127.0.0.1:7341/mcp")
    public var servers: [Server] = []
    public var keys: [ApiKey] = []
    public var error: String?
    /// Marketplace entry id currently installing (one at a time).
    public var installing: String?

    /// Inputs: engine (real process or mock), Marketplace catalog.
    public init(engine: any Engine, catalog: [MarketplaceEntry]) {
        self.engine = engine
        self.catalog = catalog
    }

    /// Inputs: none. Outputs: model wired to the bundled `Contents/Helpers/sumeru` engine and catalog.
    public static func live() -> AppModel {
        let helpers = Bundle.main.bundleURL.appending(path: "Contents/Helpers/sumeru")
        let catalog = Bundle.main.url(forResource: "marketplace", withExtension: "json")
            .flatMap { try? Data(contentsOf: $0) }.flatMap { try? decodeMarketplace($0) } ?? []
        let model = AppModel(engine: EngineProcess(executable: helpers), catalog: catalog)
        model.poll()
        return model
    }

    /// Inputs: none. Outputs: status/lists refreshed every 3 s for the menu bar and window
    /// (quietly: a slow keychain prompt at startup must not raise alerts).
    func poll() {
        Task { [weak self] in
            while let self {
                await self.refresh(quiet: true)
                try? await Task.sleep(for: .seconds(3))
            }
        }
    }

    /// Inputs: none. Outputs: the Tauri app's launch-at-login agent (~/Library/LaunchAgents/sumeru.plist)
    /// replaced by this app's login item, keeping the user's choice.
    public static func migrateTauriAutostart() {
        let old = FileManager.default.homeDirectoryForCurrentUser.appending(path: "Library/LaunchAgents/sumeru.plist")
        guard FileManager.default.fileExists(atPath: old.path) else { return }
        try? FileManager.default.removeItem(at: old)
        try? loginAgent.register()
    }

    /// Inputs: none. Outputs: status, servers and keys re-read from the engine.
    public func refresh(quiet: Bool = false) async {
        do {
            async let s: Status = engine.call("get_status")
            async let v: [Server] = engine.call("list_servers")
            async let k: [ApiKey] = engine.call("list_keys")
            (status, servers, keys) = try await (s, v, k)
        } catch {
            if !quiet { self.error = error.localizedDescription }
        }
    }

    /// Inputs: an engine action. Outputs: true on success (state refreshed), false after alerting.
    @discardableResult
    func act(_ work: () async throws -> Void) async -> Bool {
        do {
            try await work()
            await refresh()
            return true
        } catch {
            self.error = error.localizedDescription
            return false
        }
    }

    public func toggleFunnel() async {
        await act { try await engine.run(status.running ? "stop_funnel" : "start_funnel") }
    }

    /// Inputs: server, enabled flag. Outputs: saved (stored secrets are kept: empty values are skipped).
    func setEnabled(_ s: Server, _ enabled: Bool) async {
        await act { try await engine.run("upsert_server", ServerParams(id: s.id, name: s.name, enabled: enabled, transport: s.transport, secrets: Secrets())) }
    }

    func save(id: String?, name: String, enabled: Bool, transport: Transport, secrets: Secrets) async -> Bool {
        await act { try await engine.run("upsert_server", ServerParams(id: id, name: name, enabled: enabled, transport: transport, secrets: secrets)) }
    }

    func remove(_ s: Server) async { await act { try await engine.run("remove_server", IdParams(id: s.id)) } }

    /// Inputs: saved server or an unsaved draft. Outputs: "connected (N tools)" or the error text.
    func test(_ s: Server) async -> String { await message { try await self.engine.call("test_server", IdParams(id: s.id)) } }
    func testDraft(name: String, transport: Transport, secrets: Secrets) async -> String {
        await message { try await self.engine.call("test_draft", DraftParams(name: name, transport: transport, secrets: secrets)) }
    }

    func probe(url: String, id: String?) async -> AuthProbe? { try? await engine.call("probe_mcp_auth", ProbeParams(url: url, id: id)) }

    /// Inputs: MCP URL, server id, optional client credentials. Outputs: nil on success, else the error.
    func signIn(url: String, id: String, clientId: String? = nil, clientSecret: String? = nil) async -> String? {
        do {
            try await engine.run("start_mcp_oauth", OauthParams(url: url, id: id, clientId: clientId, clientSecret: clientSecret))
            return nil
        } catch { return error.localizedDescription }
    }

    /// Inputs: catalog entry, account name, API key (key-based providers only).
    /// Outputs: nil when installed (after OAuth, or a successful test connect with the key), else the error.
    func install(_ entry: MarketplaceEntry, name: String, apiKey: String = "") async -> String? {
        if nameTaken(servers, name) { return "Another MCP is already named “\(name)”. Pick a different name." }
        installing = entry.id
        defer { installing = nil }
        let id = UUID().uuidString.lowercased()
        let transport = Transport.http(url: entry.url, headerKeys: [], hasBearer: true)
        let secrets = entry.apiKeyUrl == nil ? Secrets() : Secrets(bearer: apiKey.trimmingCharacters(in: .whitespaces))
        if entry.apiKeyUrl != nil {
            do { try await engine.run("test_draft", DraftParams(name: name, transport: transport, secrets: secrets)) } catch { return "Couldn't connect with that API key: \(error.localizedDescription)" }
        } else {
            guard let p = await probe(url: entry.url, id: id), p.oauth, p.supportsDcr else {
                return "This server no longer supports automatic registration. Add it from MCPs instead."
            }
            if let err = await signIn(url: entry.url, id: id) { return err }
        }
        let ok = await save(id: id, name: name, enabled: true, transport: transport, secrets: secrets)
        return ok ? nil : error
    }

    func saveKey(id: String?, name: String, mode: KeyMode, mcpIds: [String]) async -> Bool {
        await act { try await engine.run("upsert_key", KeyParams(id: id, name: name, mode: mode, mcpIds: mcpIds)) }
    }
    func removeKey(_ k: ApiKey) async { await act { try await engine.run("remove_key", IdParams(id: k.id)) } }
    func rotateKey(_ k: ApiKey) async { await act { try await engine.run("rotate_key", IdParams(id: k.id)) } }

    /// Inputs: a call returning a message. Outputs: the message, or the error text.
    private func message(_ work: () async throws -> String) async -> String {
        do { return try await work() } catch { return error.localizedDescription }
    }

    // MARK: - Native helpers (no engine round-trip)

    public func copy(_ text: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
    }

    public func open(_ link: String) {
        if let url = URL(string: link) { NSWorkspace.shared.open(url) }
    }

    /// Login item: a LaunchAgent bundled in the app that starts it with `--hidden`.
    static let loginAgent = SMAppService.agent(plistName: "com.sumeru.app.login.plist")
    public var openAtLogin: Bool {
        get { access(keyPath: \.openAtLogin); return Self.loginAgent.status == .enabled }
        set {
            withMutation(keyPath: \.openAtLogin) {
                do {
                    if newValue { try Self.loginAgent.register() } else { try Self.loginAgent.unregister() }
                } catch { self.error = error.localizedDescription }
            }
        }
    }
}
