# Getting started

1. Install and launch Sumeru. The funnel starts automatically and keeps running in the tray when you close the window.
   - **macOS** (Apple Silicon and Intel): download `sumeru_<version>_universal.dmg` from the [latest release](https://github.com/PratyushChauhan/sumeru/releases/latest), open it, and drag **Sumeru** to **Applications**. It's a native menu-bar app: the Sumeru icon in the menu bar shows status, copies the endpoint, and opens the main window. Sumeru isn't notarized yet, so macOS blocks the first launch: open **System Settings → Privacy & Security** and click **Open Anyway**. If it still won't open, run `xattr -dr com.apple.quarantine /Applications/Sumeru.app` in Terminal. On first launch and after updates, macOS may ask to let **sumeru** use its keychain item; enter your password and click **Always Allow** (once: all secrets share one item; the first launch after upgrading from 0.4.4 or earlier asks once per old item while it moves them over) (the endpoint starts once you answer).
   - **Linux**: AppImage, `.deb` or `.rpm` from the same release.
   - **npm**: `npm i -g sumeru-mcp && sumeru gui` ([CLI](./cli.md)).
2. Open **Configure**. The endpoint URL and the **Default** API key are there; add more keys to scope agents to specific MCPs ([Scoped API keys](./cookbook/scoped-keys.md)).

![Configure tab with running badge, endpoint URL, and bearer token](/images/configure.png)

3. Add MCPs via **Marketplace** (curated DCR HTTP MCPs) or **Configure → Add MCP** (stdio commands or HTTP MCP URLs — remote URLs need HTTPS; plain HTTP is loopback-only — see [OAuth](./oauth.md)). The MCP list groups them by app — expand a row to see that app's accounts.

![Marketplace tab with curated providers and Install buttons](/images/marketplace.png)

4. Connect your agent from **Configure → Connect an agent** (Claude Code, Codex, Cursor, VS Code, Gemini CLI, Windsurf) — see [Connect your agent](./cookbook/connect-agent.md).

## Tray and pause

- Close the window → UI hides; endpoint keeps serving
- Tray → **Open** to show the UI again
- Tray → **Quit** to stop the endpoint
- Header **Pause** / **Resume** toggles the local funnel without quitting

## Autostart

Enable **Run at system startup** on the Configure tab so Sumeru launches hidden in the tray after login. Prefer enabling this from an installed/release build so the OS entry points at a stable binary path.
