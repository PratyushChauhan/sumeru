# Run at startup

1. Prefer an **installed / release** build (not `tauri dev`)
2. Open Funnelit → **Configure**
3. Enable **Run at system startup**
4. Log out and back in (or reboot)

Funnelit should start hidden in the tray with `--hidden`. Open from the tray to configure.

## Linux note

Autostart writes `~/.config/autostart/*.desktop` pointing at the current binary. If you enabled it from a debug build under `target/debug/`, re-enable from the installed binary so the path stays stable.
