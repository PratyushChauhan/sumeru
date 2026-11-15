import Foundation
import Testing
@testable import SumeruKit

let url = "http://127.0.0.1:7341/mcp"
let token = "tok+/="
func recipe(_ id: String, _ list: [AgentRecipe] = agentInstalls(url: url, token: token)) -> AgentRecipe {
    list.first { $0.id == id }!
}
func key(_ id: String, _ name: String, _ mode: KeyMode = .exclude, _ ids: [String] = []) -> ApiKey {
    ApiKey(id: id, name: name, mode: mode, mcpIds: ids, token: "")
}
let servers = ["a", "b", "c"].map { Server(id: $0, name: $0, enabled: true, transport: .http(url: "https://\($0)", headerKeys: [], hasBearer: false)) }

// Mirrors src/lib/agents.test.js — keep the two in sync.
@Suite struct AgentRecipes {
    @Test func coversEachAgentOnce() {
        #expect(agentInstalls(url: url, token: token).map(\.id) == ["claude-code", "codex", "cursor", "vscode", "gemini", "windsurf"])
    }

    @Test func embedsEndpointAndToken() {
        for r in agentInstalls(url: url, token: token) {
            #expect(r.text.contains(url) && r.text.contains("Bearer \(token)"), "\(r.id)")
        }
    }

    @Test func cursorDeeplinkHasBase64Config() throws {
        let link = try #require(URLComponents(string: recipe("cursor").link!))
        #expect(link.scheme == "cursor")
        #expect(link.queryItems?.first { $0.name == "name" }?.value == "sumeru")
        let b64 = try #require(link.queryItems?.first { $0.name == "config" }?.value)
        #expect(String(decoding: Data(base64Encoded: b64)!, as: UTF8.self)
            == #"{"url":"http://127.0.0.1:7341/mcp","headers":{"Authorization":"Bearer tok+/="}}"#)
    }

    @Test func vscodeDeeplinkIsEncodedJson() {
        let link = recipe("vscode").link!
        #expect(link.hasPrefix("vscode:mcp/install?"))
        #expect(link.dropFirst("vscode:mcp/install?".count).removingPercentEncoding
            == #"{"name":"sumeru","type":"http","url":"http://127.0.0.1:7341/mcp","headers":{"Authorization":"Bearer tok+/="}}"#)
    }

    @Test func usesEachAgentsShape() {
        #expect(recipe("codex").text.hasPrefix("[mcp_servers.sumeru]\nurl = "))
        #expect(recipe("claude-code").text.hasPrefix("claude mcp remove --scope user sumeru 2>/dev/null; claude mcp add --transport http --scope user sumeru http"))
        #expect(recipe("gemini").text.hasPrefix("gemini mcp add --scope user --transport http sumeru http"))
        // Exactly JSON.stringify(v, null, 2).
        #expect(recipe("windsurf").text == """
        {
          "mcpServers": {
            "sumeru": {
              "serverUrl": "http://127.0.0.1:7341/mcp",
              "headers": {
                "Authorization": "Bearer tok+/="
              }
            }
          }
        }
        """)
    }

    @Test func installsUnderPerKeyName() {
        let work = agentInstalls(url: url, token: token, server: "sumeru-work")
        for r in work { #expect(r.text.contains("sumeru-work"), "\(r.id)") }
        #expect(recipe("codex", work).text.hasPrefix("[mcp_servers.sumeru-work]"))
        #expect(recipe("cursor", work).link!.contains("name=sumeru-work&"))
    }

    @Test func repoRecipesNeverHoldTheToken() {
        let repo = agentInstalls(url: url, token: token, server: "sumeru-work", env: "SUMERU_WORK_TOKEN")
        #expect(repo.map(\.id) == ["claude-code", "codex", "cursor", "vscode", "gemini"])
        for r in repo {
            #expect(!r.text.contains(token) && r.text.contains(url) && r.link == nil, "\(r.id)")
        }
        #expect(recipe("claude-code", repo).text.hasSuffix("--scope project sumeru-work http://127.0.0.1:7341/mcp --header 'Authorization: Bearer ${SUMERU_WORK_TOKEN}'"))
        #expect(recipe("codex", repo).text.contains(#"bearer_token_env_var = "SUMERU_WORK_TOKEN""#))
        #expect(recipe("cursor", repo).text.contains(#""Authorization": "Bearer ${env:SUMERU_WORK_TOKEN}""#))
        #expect(recipe("vscode", repo).text.contains(#""password": true"#))
        #expect(recipe("vscode", repo).text.contains(#""Authorization": "Bearer ${input:sumeru-work-token}""#))
    }

    @Test func pasteInPrompt() {
        #expect(agentPrompt(recipe("codex")) == "Install the Sumeru MCP server in Codex. Paste into ~/.codex/config.toml (replace any existing [mcp_servers.sumeru]):\n\n```\n\(recipe("codex").text)\n```\n\nKeep my other MCP servers and settings. Then tell me if Codex needs a restart to load it.")
        #expect(agentPrompt(recipe("cursor")).contains("in Cursor. Paste into ~/.cursor/mcp.json:\n"))
        let repo = agentInstalls(url: url, token: token, env: "SUMERU_TOKEN")[0]
        let p = agentPrompt(repo, env: "SUMERU_TOKEN")
        #expect(p.contains(repo.text) && !p.contains(token))
        #expect(p.contains("reads the token from $SUMERU_TOKEN; never write the token into the repo."))
        let vscode = agentPrompt(agentInstalls(url: url, token: token, env: "SUMERU_TOKEN")[3], env: "SUMERU_TOKEN")
        #expect(!vscode.contains("$SUMERU_TOKEN") && vscode.contains(" Never write the token into the repo."))
    }
}

// Mirrors src/lib/keys.test.js.
@Suite struct Keys {
    @Test func scopeRules() {
        #expect(keyAllows(key("k", "K"), "a"))
        #expect(!keyAllows(key("k", "K", .exclude, ["a"]), "a"))
        #expect(keyAllows(key("k", "K", .include, ["a"]), "a"))
        #expect(!keyAllows(key("k", "K", .include, ["a"]), "b"))
        #expect(scopeIds(.include, allowed: ["a"], servers: servers) == ["a"])
        #expect(scopeIds(.exclude, allowed: ["a"], servers: servers) == ["b", "c"])
    }

    @Test func labelsAndNames() {
        #expect(scopeLabel(key("k", "K"), servers) == "All MCPs")
        #expect(scopeLabel(key("k", "K", .exclude, ["a"]), servers) == "All except 1")
        #expect(scopeLabel(key("k", "K", .include, ["a", "b"]), servers) == "Only 2 of 3 MCPs")
        #expect(scopeLabel(key("k", "K", .include), servers) == "No MCPs")
        #expect(agentServerName(key("default", "Default")) == "sumeru")
        #expect(agentServerName(key("x", "Work · Acme!")) == "sumeru-work-acme")
        #expect(agentServerName(key("1234567890", "···")) == "sumeru-12345678")
        #expect(tokenEnvVar(key("default", "Default")) == "SUMERU_TOKEN")
        #expect(tokenEnvVar(key("x", "Work repo")) == "SUMERU_WORK_REPO_TOKEN")
    }
}

@Suite struct MarketplaceAndDrafts {
    let fathom = MarketplaceEntry(id: "fathom", name: "Fathom", description: "", url: "https://api.fathom.ai/mcp")

    @Test func accounts() {
        let one = [Server(id: "1", name: "Fathom", enabled: true, transport: .http(url: fathom.url, headerKeys: [], hasBearer: true))]
        #expect(installedServers(fathom, one).count == 1)
        #expect(nextAccountName(fathom, one) == "Fathom 2")
        #expect(nameTaken(one, " fathom "))
    }

    // Mirrors the serverGroups test in marketplace.test.js.
    @Test func groupsByApp() throws {
        let file = URL(filePath: #filePath).deletingLastPathComponent().appending(path: "../../../src/lib/marketplace.json")
        let catalog = try decodeMarketplace(Data(contentsOf: file))
        let http = { (id: String, name: String, url: String, on: Bool) in
            Server(id: id, name: name, enabled: on, transport: .http(url: url, headerKeys: [], hasBearer: true))
        }
        let servers = [
            http("a", "doppl", "https://mcp.render.com/mcp", true),
            Server(id: "b", name: "fs", enabled: true, transport: .stdio(command: "npx", args: [], envKeys: [])),
            http("c", "vinci", "https://mcp.render.com/mcp", false),
            http("d", "Sentry-vinci", "https://mcp.sentry.dev/other", true),
            http("e", "Custom", "https://example.com/mcp", true),
            http("f", "Local box", "https://local/mcp", true),
        ]
        #expect(serverGroups(servers, catalog).map { [$0.id, $0.name] + $0.servers.map(\.id) } == [
            ["render", "Render", "a", "c"], ["local", "Local commands", "b"],
            ["sentry", "Sentry", "d"], ["host:example.com", "example.com", "e"], ["host:local", "local", "f"],
        ])
        #expect(groupSummary(Array(servers.prefix(3))) == "3 MCPs · 1 off")
        #expect(groupSummary(Array(servers.prefix(1))) == "1 MCP")
    }

    @Test func bundledCatalogDecodes() throws {
        let file = URL(filePath: #filePath).deletingLastPathComponent().appending(path: "../../../src/lib/marketplace.json")
        let entries = try decodeMarketplace(Data(contentsOf: file))
        #expect(entries.contains { $0.id == "fathom" } && entries.allSatisfy { $0.url.hasPrefix("https://") })
        #expect(entries.filter { $0.apiKeyUrl != nil }.map(\.id) == ["render"])
    }

    @Test func drafts() throws {
        #expect(splitCommandLine(#"npx -y "my pkg" x"#).args == ["-y", "my pkg", "x"])
        let (t, s) = try readDraft(source: "npx server", headersText: "", bearer: "", envText: "A=1\nB=x=y", argsText: "--flag", oauthConnected: false)
        #expect(t == .stdio(command: "npx", args: ["server", "--flag"], envKeys: ["A", "B"]))
        #expect(s.env == ["A": "1", "B": "x=y"])
        #expect(throws: EngineError.self) { try parsePairs("ok=1\n=bad") }
        let (h, _) = try readDraft(source: " https://x/mcp ", headersText: "", bearer: "", envText: "", argsText: "", oauthConnected: true)
        #expect(h == .http(url: "https://x/mcp", headerKeys: [], hasBearer: true))
    }

    @Test func transportJsonMatchesRust() throws {
        let json = #"{"kind":"stdio","command":"npx","args":["a"]}"#
        let t = try JSONDecoder().decode(Transport.self, from: Data(json.utf8))
        #expect(t == .stdio(command: "npx", args: ["a"], envKeys: []))
        let back = try JSONSerialization.jsonObject(with: JSONEncoder().encode(t)) as! [String: Any]
        #expect(back["kind"] as? String == "stdio" && back["env_keys"] as? [String] == [])
    }
}

@Suite struct EngineFraming {
    /// A fake `sumeru engine`: echoes each request id, replies with an error for method "fail".
    @Test func repliesMatchById() async throws {
        let script = FileManager.default.temporaryDirectory.appending(path: "fake-engine-\(UUID()).sh")
        try """
        #!/bin/sh
        while read -r line; do
          id=$(echo "$line" | sed -E 's/.*"id":([0-9]+).*/\\1/')
          case "$line" in *'"fail"'*) echo "{\\"id\\":$id,\\"error\\":\\"nope\\"}";;
          *) echo "{\\"id\\":$id,\\"result\\":{\\"running\\":true,\\"endpoint\\":\\"e$id\\"}}";; esac
        done
        """.write(to: script, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: script.path)
        let engine = EngineProcess(executable: script)
        async let a: Status = engine.call("get_status")
        async let b: Status = engine.call("get_status")
        let (sa, sb) = try await (a, b)
        #expect(Set([sa.endpoint, sb.endpoint]) == ["e1", "e2"])
        await #expect(throws: EngineError("nope")) { try await engine.run("fail") }
        await engine.stop()
    }
}
