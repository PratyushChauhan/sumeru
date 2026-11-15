import Foundation

/// Funnel state from `get_status`.
public struct Status: Codable, Equatable, Sendable {
    public var running: Bool
    public var endpoint: String
    public init(running: Bool, endpoint: String) { self.running = running; self.endpoint = endpoint }
}

/// How Sumeru reaches an upstream MCP (Rust `McpTransport`, tagged by `kind`).
public enum Transport: Codable, Equatable, Sendable {
    case stdio(command: String, args: [String], envKeys: [String])
    case http(url: String, headerKeys: [String], hasBearer: Bool)

    private enum K: String, CodingKey {
        case kind, command, args, url
        case envKeys = "env_keys", headerKeys = "header_keys", hasBearer = "has_bearer"
    }

    /// Inputs: decoder over `{kind, …}`. Outputs: transport; missing lists default to empty.
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: K.self)
        switch try c.decode(String.self, forKey: .kind) {
        case "stdio":
            self = .stdio(
                command: try c.decode(String.self, forKey: .command),
                args: try c.decodeIfPresent([String].self, forKey: .args) ?? [],
                envKeys: try c.decodeIfPresent([String].self, forKey: .envKeys) ?? [])
        default:
            self = .http(
                url: try c.decode(String.self, forKey: .url),
                headerKeys: try c.decodeIfPresent([String].self, forKey: .headerKeys) ?? [],
                hasBearer: try c.decodeIfPresent(Bool.self, forKey: .hasBearer) ?? false)
        }
    }

    /// Inputs: encoder. Outputs: the snake_case JSON shape the engine expects.
    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: K.self)
        switch self {
        case let .stdio(command, args, envKeys):
            try c.encode("stdio", forKey: .kind)
            try c.encode(command, forKey: .command)
            try c.encode(args, forKey: .args)
            try c.encode(envKeys, forKey: .envKeys)
        case let .http(url, headerKeys, hasBearer):
            try c.encode("http", forKey: .kind)
            try c.encode(url, forKey: .url)
            try c.encode(headerKeys, forKey: .headerKeys)
            try c.encode(hasBearer, forKey: .hasBearer)
        }
    }

    public var url: String? { if case let .http(url, _, _) = self { url } else { nil } }
}

/// A configured upstream MCP (`list_servers`).
public struct Server: Codable, Equatable, Identifiable, Sendable {
    public var id: String
    public var name: String
    public var enabled: Bool
    public var transport: Transport
    public init(id: String, name: String, enabled: Bool, transport: Transport) {
        self.id = id; self.name = name; self.enabled = enabled; self.transport = transport
    }
}

/// Secret values sent with `upsert_server` / `test_draft` (stored in the keychain, never listed).
public struct Secrets: Codable, Equatable, Sendable {
    public var env: [String: String] = [:]
    public var headers: [String: String] = [:]
    public var bearer: String?
    public init(env: [String: String] = [:], headers: [String: String] = [:], bearer: String? = nil) {
        self.env = env; self.headers = headers; self.bearer = bearer
    }
}

/// Whether a key lists the MCPs it may reach (`include`) or the ones it may not (`exclude`).
public enum KeyMode: String, Codable, Sendable { case include, exclude }

/// An endpoint API key with its bearer token (`list_keys`).
public struct ApiKey: Codable, Equatable, Identifiable, Sendable {
    public var id: String
    public var name: String
    public var mode: KeyMode
    public var mcpIds: [String]
    public var token: String
    enum CodingKeys: String, CodingKey { case id, name, mode, token, mcpIds = "mcp_ids" }
    public init(id: String, name: String, mode: KeyMode, mcpIds: [String], token: String) {
        self.id = id; self.name = name; self.mode = mode; self.mcpIds = mcpIds; self.token = token
    }
    public var isDefault: Bool { id == "default" }
}

/// OAuth discovery result for an MCP URL (`probe_mcp_auth`).
public struct AuthProbe: Codable, Equatable, Sendable {
    public var oauth: Bool
    public var label: String
    public var supportsDcr: Bool
    public var needsClientId: Bool
    public var hasSavedClient: Bool
    public var connected: Bool
    enum CodingKeys: String, CodingKey {
        case oauth, label, connected
        case supportsDcr = "supports_dcr", needsClientId = "needs_client_id"
        case hasSavedClient = "has_saved_client"
    }
}

/// A curated Marketplace provider (`src/lib/marketplace.json`).
public struct MarketplaceEntry: Codable, Equatable, Identifiable, Sendable {
    public var id: String
    public var name: String
    public var description: String
    public var url: String
}

/// Inputs: marketplace.json data. Outputs: its provider entries.
public func decodeMarketplace(_ data: Data) throws -> [MarketplaceEntry] {
    struct File: Decodable { var entries: [MarketplaceEntry] }
    return try JSONDecoder().decode(File.self, from: data).entries
}
