# CLI

Install the npm launcher (downloads the matching portable binary on first run):

```bash
npm i -g sumeru-mcp
sumeru doctor
sumeru mcp-stdio   # or just: sumeru
sumeru mcp-stdio --key "work repo"  # scope to an API key (default: Default)
sumeru gui         # desktop UI (on macOS: opens Sumeru.app)
sumeru gui --hidden  # tray-only start
sumeru engine      # internal: gateway + JSON API on stdin/stdout (used by the macOS app)
sumeru servers …   # manage MCPs from scripts (below)
sumeru keys …      # manage API keys from scripts (below)
```

The macOS app bundles the same binary at `/Applications/Sumeru.app/Contents/Helpers/sumeru`.

## Manage MCPs and keys

Agents and scripts can add MCPs and scope API keys without the GUI:

```bash
sumeru servers list
sumeru servers add --name Granola --url https://mcp.granola.ai/mcp
sumeru servers add --name Linear --url https://mcp.linear.app/mcp --header X-Team=eng
printf %s "$TOKEN" | sumeru servers add --name Internal --url https://mcp.example.com/mcp --bearer-stdin
sumeru servers add --name Files --command /usr/local/bin/files-mcp --arg --root --arg ~/notes --env API_KEY=…
sumeru servers login Granola          # OAuth: prints the sign-in URL, opens the browser, waits
sumeru servers login Granola --no-browser  # only print the URL
sumeru servers disable Granola        # or: enable, remove
sumeru keys list                      # every key and the MCPs it reaches (never tokens)
sumeru keys add "work repo"           # new key that reaches nothing until granted
sumeru keys grant "work repo" Granola # or: revoke
sumeru keys token "work repo"         # bearer token on stdout, nothing else
```

`SERVER` and `KEY` are a name (any case) or an id. Add `--json` to any command for one JSON value on stdout (failures print `{"error": "…"}`). Failures exit with status 1. Header, env, and bearer values go to the OS keychain the app uses, never to `servers.json`. Use `--bearer-stdin`, `--client-secret-stdin`, or a value of `-` (`--header X-Api-Key=-`) to read a secret from stdin and keep it off the command line (one stdin value per command).

The app stays the only writer of its config. While Sumeru runs, the CLI sends each command to the engine over `engine.sock` in the config dir. The socket has mode `0600` and accepts only your user. The macOS app shows changes within a few seconds. The Linux app shows them when its window regains focus. When Sumeru is not running, the CLI edits the store itself. It holds the same `engine.lock` the app takes at startup, so the two never write at once. If Sumeru starts during a CLI command, it holds its own edits until that command finishes. Each change, such as a grant or an enable, is one atomic edit in the engine, so parallel commands never overwrite each other. Interrupting `servers login` (Ctrl-C) cancels that sign-in in the engine.

Use a key's token in a Claude Code `headersHelper`:

```json
{
  "mcpServers": {
    "sumeru": {
      "type": "http",
      "url": "http://127.0.0.1:7341/mcp",
      "headersHelper": "t=$(sumeru keys token 'work repo') && printf '{\"Authorization\": \"Bearer %s\"}' \"$t\""
    }
  }
}
```

Testing: `SUMERU_CONFIG_DIR=<dir>` points the CLI and engine at another config dir. `SUMERU_SECRETS=memory` keeps secrets in process memory instead of the keychain. Keep that dir path short: Unix socket paths are limited to about 100 bytes. That memory store only lasts as long as the engine process, so run the CLI against a running `sumeru engine`.

On Linux, the portable binary needs system WebKitGTK. For a casual desktop install, prefer the AppImage or `.deb` from [GitHub Releases](https://github.com/PratyushChauhan/sumeru/releases).

## Stdio MCP host config

```json
{
  "mcpServers": {
    "sumeru": {
      "command": "sumeru",
      "args": ["mcp-stdio"]
    }
  }
}
```

## Overrides

- `SUMERU_BINARY` — local binary path
- `SUMERU_VERSION` — release version to download
- `SUMERU_CACHE_DIR` — cache directory for binaries

## Doctor

`sumeru doctor` reports CLI version, platform, cached binary, config dir writability, and whether something answers on `:7341/mcp`.
