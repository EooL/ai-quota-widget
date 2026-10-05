# AI Quota Widget — install notes

A small always-on-top widget that shows the **5-hour** and **weekly** quota of Claude, Codex (ChatGPT) and Antigravity (Gemini) on your own machine.

## What you need first

- The CLIs you want to track, installed **and signed in**: `claude`, `codex`, `agy`. The widget reads the same local sign-in these tools already use; it never asks for a password and sends nothing anywhere except to each provider's own API.
- Windows 10/11 (x64) or macOS 11+ (Apple Silicon or Intel).

## Windows

1. Run `AI Quota Widget_<version>_x64-setup.exe` (installs for your user only, no admin needed).
2. **SmartScreen** may say "Windows protected your PC" because the installer is not code-signed: click **More info → Run anyway**.

## macOS

1. Open the `.dmg` and drag **AI Quota Widget** to Applications.
2. The app is not notarized, so the first launch is blocked by Gatekeeper. Either **right-click the app → Open → Open**, or run once:
   ```sh
   xattr -dr com.apple.quarantine "/Applications/AI Quota Widget.app"
   ```
3. On first run macOS may ask to let the app read the **"Claude Code-credentials"** Keychain item (that is where Claude Code keeps its sign-in). Choose **Always Allow**.
4. The widget lives in the menu bar (no Dock icon). Use the tray icon to show, hide or quit.

## Using it

- Hover the widget to reveal the buttons: refresh, settings, move to another corner, and sizes **S / M / L**.
- Global shortcuts: `Alt+Shift+Q` show/hide, `Alt+Shift+W` cycle size, `Alt+Shift+E` cycle corner (on macOS `Alt` is `Option`).
- If a platform shows "unavailable", sign in to its CLI again (`claude auth login`, `codex login`) and press refresh.

## Notes

- Claude quota comes from the same endpoint Claude Code uses for `/usage`. It is undocumented and could change; the widget caches and backs off when rate limited.
- Cost figures are *API-equivalent* estimates at public prices, not charges on your subscription.
