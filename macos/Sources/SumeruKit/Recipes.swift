import Foundation

// Swift port of src/lib/{agents,keys,marketplace,draft}.js. Output must match the JS
// byte-for-byte (tests mirror agents.test.js / keys.test.js), so both apps hand agents the same config.

/// One way to install Sumeru into a coding agent: a command or config snippet, maybe a deeplink.
public struct AgentRecipe: Equatable, Identifiable, Sendable {
    public let id: String
    public let name: String
    public let hint: String
    public let text: String
    public let link: String?
}

/// Ordered JSON value, printed like `JSON.stringify(v, null, 2)` (Foundation reorders keys).
indirect enum J {
    case s(String), b(Bool), a([J]), o([(String, J)])

    /// Inputs: indent width (0 = compact). Outputs: JSON text.
    func render(_ indent: Int = 2, _ depth: Int = 0) -> String {
        let pad = indent > 0 ? "\n" + String(repeating: " ", count: indent * (depth + 1)) : ""
        let end = indent > 0 ? "\n" + String(repeating: " ", count: indent * depth) : ""
        let colon = indent > 0 ? ": " : ":"
        switch self {
        case let .s(v): return J.quote(v)
        case let .b(v): return v ? "true" : "false"
        case let .a(v): return v.isEmpty ? "[]" : "[" + v.map { pad + $0.render(indent, depth + 1) }.joined(separator: ",") + end + "]"
        case let .o(v): return v.isEmpty ? "{}" : "{" + v.map { pad + J.quote($0.0) + colon + $0.1.render(indent, depth + 1) }.joined(separator: ",") + end + "}"
        }
    }

    /// Inputs: string. Outputs: JSON string literal (JSON.stringify escaping).
    static func quote(_ s: String) -> String {
        var out = "\""
        for u in s.unicodeScalars {
            switch u {
            case "\"": out += "\\\""
            case "\\": out += "\\\\"
            case "\n": out += "\\n"
            case "\r": out += "\\r"
            case "\t": out += "\\t"
            case _ where u.value < 0x20: out += String(format: "\\u%04x", u.value)
            default: out.unicodeScalars.append(u)
            }
        }
        return out + "\""
    }
}

/// Inputs: string. Outputs: JS `encodeURIComponent(s)`.
func encodeURIComponent(_ s: String) -> String {
    var allowed = CharacterSet.alphanumerics.intersection(.init(charactersIn: Unicode.Scalar(0)...Unicode.Scalar(127)))
    allowed.insert(charactersIn: "-_.!~*'()")
    return s.addingPercentEncoding(withAllowedCharacters: allowed) ?? s
}

/// Inputs: endpoint URL, bearer token (or placeholder), MCP server name agents install it under,
/// and `env`: when set, repo-level recipes that read the token from that env var instead.
/// Outputs: one recipe per supported agent (`agentInstalls` in agents.js).
public func agentInstalls(url: String, token: String, server: String = "sumeru", env: String = "") -> [AgentRecipe] {
    if !env.isEmpty { return repoInstalls(url: url, server: server, env: env) }
    let auth = "Bearer \(token)"
    let headers = J.o([("Authorization", .s(auth))])
    let cursor = J.o([("url", .s(url)), ("headers", headers)])
    let vscode = J.o([("type", .s("http")), ("url", .s(url)), ("headers", headers)])
    let vscodeLink = J.o([("name", .s(server)), ("type", .s("http")), ("url", .s(url)), ("headers", headers)])
    let cursorConfig = Data(cursor.render(0).utf8).base64EncodedString()
    return [
        .init(id: "claude-code", name: "Claude Code", hint: "Run in a terminal",
              text: "claude mcp remove --scope user \(server) 2>/dev/null; claude mcp add --transport http --scope user \(server) \(url) --header \"Authorization: \(auth)\"",
              link: nil),
        .init(id: "codex", name: "Codex",
              hint: "Paste into ~/.codex/config.toml (replace any existing [mcp_servers.\(server)])",
              text: "[mcp_servers.\(server)]\nurl = \"\(url)\"\nhttp_headers = { Authorization = \"\(auth)\" }",
              link: nil),
        .init(id: "cursor", name: "Cursor", hint: "Click Add to Cursor, or paste into ~/.cursor/mcp.json",
              text: J.o([("mcpServers", .o([(server, cursor)]))]).render(),
              link: "cursor://anysphere.cursor-deeplink/mcp/install?name=\(encodeURIComponent(server))&config=\(encodeURIComponent(cursorConfig))"),
        .init(id: "vscode", name: "VS Code", hint: "Click Add to VS Code, or paste into your user mcp.json",
              text: J.o([("servers", .o([(server, vscode)]))]).render(),
              link: "vscode:mcp/install?\(encodeURIComponent(vscodeLink.render(0)))"),
        .init(id: "gemini", name: "Gemini CLI", hint: "Run in a terminal",
              text: "gemini mcp add --scope user --transport http \(server) \(url) --header \"Authorization: \(auth)\"",
              link: nil),
        .init(id: "windsurf", name: "Windsurf", hint: "Paste into ~/.codeium/windsurf/mcp_config.json",
              text: J.o([("mcpServers", .o([(server, .o([("serverUrl", .s(url)), ("headers", headers)]))]))]).render(),
              link: nil),
    ]
}

/// Inputs: a recipe from `agentInstalls`, and `env` (repo scope's token env var, or "").
/// Outputs: a prompt to paste into that agent so it installs Sumeru itself (`agentPrompt` in agents.js).
public func agentPrompt(_ r: AgentRecipe, env: String = "") -> String {
    let how = r.hint.replacingOccurrences(of: "^Click Add to [^,]+, or p", with: "P", options: .regularExpression)
    let secret = env.isEmpty ? "" : " It reads the token from $\(env); never write the token into the repo."
    return "Install the Sumeru MCP server in \(r.name). \(how):\n\n```\n\(r.text)\n```\n\nKeep my other MCP servers and settings.\(secret) Then tell me if \(r.name) needs a restart to load it."
}

/// Inputs: endpoint URL, server name, env var holding the key's token.
/// Outputs: repo-level recipes safe to commit (env var or VS Code password prompt, never the token).
func repoInstalls(url: String, server: String, env: String) -> [AgentRecipe] {
    let auth = { (ref: String) in J.o([("Authorization", .s("Bearer \(ref)"))]) }
    let input = "\(server)-token"
    return [
        .init(id: "claude-code", name: "Claude Code",
              hint: "Run in the repo root (writes .mcp.json; Claude Code reads $\(env))",
              text: "claude mcp remove --scope project \(server) 2>/dev/null; claude mcp add --transport http --scope project \(server) \(url) --header 'Authorization: Bearer ${\(env)}'",
              link: nil),
        .init(id: "codex", name: "Codex",
              hint: "Paste into .codex/config.toml in the repo (trusted projects; Codex reads $\(env))",
              text: "[mcp_servers.\(server)]\nurl = \"\(url)\"\nbearer_token_env_var = \"\(env)\"",
              link: nil),
        .init(id: "cursor", name: "Cursor", hint: "Paste into .cursor/mcp.json in the repo (Cursor reads $\(env))",
              text: J.o([("mcpServers", .o([(server, .o([("url", .s(url)), ("headers", auth("${env:\(env)}"))]))]))]).render(),
              link: nil),
        .init(id: "vscode", name: "VS Code",
              hint: "Paste into .vscode/mcp.json — VS Code asks for the token once and stores it securely",
              text: J.o([
                  ("inputs", .a([.o([("type", .s("promptString")), ("id", .s(input)),
                                     ("description", .s("Sumeru token (\(server))")), ("password", .b(true))])])),
                  ("servers", .o([(server, .o([("type", .s("http")), ("url", .s(url)), ("headers", auth("${input:\(input)}"))]))])),
              ]).render(),
              link: nil),
        .init(id: "gemini", name: "Gemini CLI",
              hint: "Run in the repo root (writes .gemini/settings.json; Gemini reads $\(env))",
              text: "gemini mcp add --scope project --transport http \(server) \(url) --header 'Authorization: Bearer ${\(env)}'",
              link: nil),
    ]
}

// MARK: - Keys (keys.js)

/// Inputs: key, mcp id. Outputs: whether the key may reach the MCP (Rust `ApiKey::allows`).
public func keyAllows(_ key: ApiKey, _ id: String) -> Bool {
    key.mcpIds.contains(id) == (key.mode == .include)
}

/// Inputs: mode, ids switched on (reachable), all servers. Outputs: `mcp_ids` to store.
public func scopeIds(_ mode: KeyMode, allowed: Set<String>, servers: [Server]) -> [String] {
    servers.map(\.id).filter { allowed.contains($0) == (mode == .include) }
}

/// Inputs: key, servers. Outputs: short scope summary ("All MCPs", "Only 2 of 3 MCPs", …).
public func scopeLabel(_ key: ApiKey, _ servers: [Server]) -> String {
    let n = servers.filter { keyAllows(key, $0.id) }.count
    if key.mode == .include { return n > 0 ? "Only \(n) of \(servers.count) MCPs" : "No MCPs" }
    return n == servers.count ? "All MCPs" : "All except \(servers.count - n)"
}

/// Inputs: key. Outputs: server name agents install it under (`sumeru`, or `sumeru-<slug>`).
public func agentServerName(_ key: ApiKey?) -> String {
    guard let key, !key.isDefault else { return "sumeru" }
    let slug = key.name.lowercased()
        .replacingOccurrences(of: "[^a-z0-9]+", with: "-", options: .regularExpression)
        .trimmingCharacters(in: CharacterSet(charactersIn: "-"))
    return "sumeru-\(slug.isEmpty ? String(key.id.prefix(8)) : slug)"
}

/// Inputs: key. Outputs: env var repo configs read its token from (`SUMERU_<KEY>_TOKEN`).
public func tokenEnvVar(_ key: ApiKey?) -> String {
    agentServerName(key).uppercased().replacingOccurrences(of: "-", with: "_") + "_TOKEN"
}

// MARK: - Marketplace (marketplace.js)

/// Inputs: entry, servers. Outputs: every installed account of that provider (same HTTP URL).
public func installedServers(_ entry: MarketplaceEntry, _ servers: [Server]) -> [Server] {
    servers.filter { $0.transport.url == entry.url }
}

/// Inputs: servers, name. Outputs: whether another MCP already uses the name (case-insensitive).
public func nameTaken(_ servers: [Server], _ name: String) -> Bool {
    let n = name.trimmingCharacters(in: .whitespaces).lowercased()
    return servers.contains { $0.name.trimmingCharacters(in: .whitespaces).lowercased() == n }
}

/// Inputs: entry, servers. Outputs: next free "<Provider> N" account name.
public func nextAccountName(_ entry: MarketplaceEntry, _ servers: [Server]) -> String {
    var n = installedServers(entry, servers).count + 1
    while nameTaken(servers, "\(entry.name) \(n)") { n += 1 }
    return "\(entry.name) \(n)"
}

// MARK: - MCP drafts (draft.js)

/// Inputs: text. Outputs: true when it looks like an MCP URL.
public func isHttpSource(_ value: String) -> Bool {
    value.trimmingCharacters(in: .whitespaces).range(of: "^https?://", options: [.regularExpression, .caseInsensitive]) != nil
}

/// Inputs: `KEY=value` lines. Outputs: map, or throws naming the first malformed line.
public func parsePairs(_ text: String) throws -> [String: String] {
    var out: [String: String] = [:]
    for (i, line) in text.components(separatedBy: "\n").enumerated() {
        let t = line.trimmingCharacters(in: .whitespaces)
        if t.isEmpty { continue }
        guard let eq = t.firstIndex(of: "="), eq != t.startIndex else {
            throw EngineError("Invalid KEY=value entry on line \(i + 1)")
        }
        out[t[..<eq].trimmingCharacters(in: .whitespaces)] = String(t[t.index(after: eq)...])
    }
    return out
}

/// Inputs: command line. Outputs: command and args (quoted segments kept together).
public func splitCommandLine(_ text: String) -> (command: String, args: [String]) {
    let re = try! NSRegularExpression(pattern: #"(?:[^\s"]+|"[^"]*")+"#)
    let t = text.trimmingCharacters(in: .whitespaces)
    let tokens = re.matches(in: t, range: NSRange(t.startIndex..., in: t)).map {
        String(t[Range($0.range, in: t)!]).replacingOccurrences(of: #"^"|"$"#, with: "", options: .regularExpression)
    }
    return (tokens.first ?? "", Array(tokens.dropFirst()))
}

/// Inputs: editor fields. Outputs: transport + secrets to save or test (`readDraft`).
public func readDraft(source: String, headersText: String, bearer: String, envText: String,
                      argsText: String, oauthConnected: Bool) throws -> (Transport, Secrets) {
    let trimmed = source.trimmingCharacters(in: .whitespaces)
    if isHttpSource(trimmed) {
        let headers = try parsePairs(headersText)
        let b = bearer.trimmingCharacters(in: .whitespaces)
        return (.http(url: trimmed, headerKeys: headers.keys.sorted(), hasBearer: oauthConnected || !b.isEmpty),
                Secrets(headers: headers, bearer: b.isEmpty ? nil : b))
    }
    let env = try parsePairs(envText)
    let lines = argsText.components(separatedBy: "\n").map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
    let parsed = splitCommandLine(trimmed)
    return (.stdio(command: parsed.command, args: parsed.args + lines, envKeys: env.keys.sorted()), Secrets(env: env))
}
