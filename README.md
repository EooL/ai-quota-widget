# AI Quota Widget

Widget desktop plutitor (Tauri v2 + React 18) care urmărește cotele celor trei abonamente AI de $20/lună: **ChatGPT Plus**, **Google Gemini Advanced** și **Claude Pro**.

Fereastra are 420×560, fără decorațiuni, transparentă, always-on-top, fără taskbar. În interior, fiecare platformă are două instrumente: fereastra rolling de 5 ore și fereastra rolling de 7 zile.

## Cerințe

- Node.js 18+
- Yarn
- Rust (stable) + [Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/)
- Opțional, CLI-urile de mai jos în `PATH`, plus fișiere locale de auth pentru fallback HTTP

## Rulare

```bash
yarn install
yarn tauri dev
```

Build de producție:

```bash
yarn tauri build
```

Frontend-ul poate fi previzualizat și în browser (`yarn dev`) cu date mock, detectate prin absența lui `__TAURI_INTERNALS__`.

## Surse de date (first-success-wins)

| Platformă | Sursă 1 | Fallback | `source` rezultat |
| --- | --- | --- | --- |
| ChatGPT Plus | `GET https://chatgpt.com/backend-api/wham/usage` cu tokenul Codex din `~/.codex/auth.json` | `codex status --json`, apoi `GET https://chatgpt.com/backend-api/conversation_limit` | `http:chatgpt.com/wham` / `cli:codex` / `http:chatgpt.com` |
| Gemini Models | `agy -p "/quota" --output-format json` | `antigravity quota --json`, apoi `gcloud ai quota list --format=json` | `cli:agy` / `cli:antigravity` / `cli:gcloud` |
| Claude | `GET https://api.anthropic.com/api/oauth/usage` cu tokenul OAuth din `~/.claude/.credentials.json` | limita observată de `cli-orchestrator` (`~/.cli-orchestrator/limits.json`), apoi ultimele date reușite (max 1h, marcate vechi) | `http:api.anthropic.com/oauth/usage` / `ledger:cli-orchestrator` |

> Notă Gemini: installerul `https://antigravity.google/cli/install.ps1` instalează `agy.exe`. În versiunea 1.2.7, quota-ul se obține headless cu `agy -p "/quota" --output-format json`; widgetul parsează grupul `Gemini Models` și bucket-urile `5h`/`weekly`.

### Claude: cum funcționează

Endpoint-ul `/api/oauth/usage` este cel pe care îl folosește Claude Code pentru `/usage`; returnează `five_hour` și `seven_day` (`utilization` în procente consumate + `resets_at`). **Nu este documentat oficial**: poate dispărea sau schimba formatul, iar folosirea tokenului OAuth al abonamentului în afara Claude Code este o zonă gri în termenii Anthropic. Widget-ul face doar citiri și nu trimite nicio cerere de inferență.

- **Credențiale**: `CLAUDE_CODE_OAUTH_TOKEN`, apoi `$CLAUDE_CONFIG_DIR/.credentials.json`, apoi `~/.claude/.credentials.json`, apoi Keychain (macOS). Tokenul nu este logat și nu apare în `Debug`.
- **Token expirat** (durează ~1h): widget-ul nu îl reîmprospătează singur (rotația refresh token-ului ar putea invalida sesiunea Claude Code). Rulează `claude auth status` (cel mult o dată la 10 min) și recitește fișierul; dacă e tot expirat, afișează eroarea.
- **Rate limit**: cel mult o cerere la 2 minute, indiferent de intervalul de polling; la `429` backoff exponențial 5→30 min (sau `Retry-After`), cu date din cache între timp.
- **`User-Agent`**: endpoint-ul limitează agresiv cererile fără `claude-code/<versiune>`; widget-ul trimite `claude-code/<versiunea din claude --version>`. Suprascrie cu `AQW_CLAUDE_UA`.
- Fereastra `seven_day_opus`/`seven_day_sonnet` nu este afișată încă.

### Integrare cu cli-orchestrator

După fiecare rundă de polling, widget-ul scrie atomic `~/.cli-orchestrator/quota-snapshot.json` (`$ORCH_HOME` dacă e setat): pentru fiecare platformă `cli`, `status`, `source`, `fetched_at_ms` și ferestrele `short`/`weekly` (`remaining_pct`, `reset_at_ms`). Orchestratorul îl folosește la rutare (sare peste un CLI epuizat, pune la coadă unul aproape de limită) și ignoră snapshot-ul mai vechi de 15 minute. În sens invers, widget-ul citește `limits.json` ca fallback pentru Claude.

Dacă toate metodele eșuează, UI-ul arată **Nedisponibil** (`source: none`) și un buton **Retry**.

Ferestre urmărite: cota scurtă (4h ChatGPT / 5h Gemini & Claude) și cota săptămânală rolling (168h).

## Teste

```bash
cd src-tauri && cargo test
```

Acoperă parsarea răspunsului Claude, credențialele, backoff-ul, cache-ul și fallback-ul pe ledger (`providers/claude_usage.rs`), plus formatul snapshot-ului (`snapshot.rs`).

## Setări persistente

- `localStorage["aqw:settings:v1"]` — interval poll (10–300s), prag warning (5–50%), vizibilitate platforme, autostart
- `localStorage["aqw:history:v1"]` — maxim 288 puncte / platformă (24h @ 5 min) pentru sparkline

Autostart-ul apelează comenzile Rust `set_autostart` / `get_autostart` (plugin `tauri-plugin-autostart`, `MacosLauncher::LaunchAgent`).

## Structură

```
src/                 React + Tailwind
src-tauri/src        Rust: models, providers/{chatgpt,gemini,claude}, tray, comenzi
src-tauri/capabilities/default.json
```

### Claude: modele și cost API

Claude Code nu are comandă de catalog, deci lista de modele din planner e fixă (`providers/claude.rs::models()`: Haiku 4.5, Sonnet 5.5, Opus 5.5, Fable 5.1, plus modelul din `settings.json`/`ANTHROPIC_MODEL`). Costul API echivalent folosește prețurile publice per familie (`src/lib/autoEstimate.ts::claudeRates`); consumul în % per cerere e proporțional cu prețul (euristică). Fable/Mythos nu se adaugă automat, doar manual din planner.

## Mărimi, prognoză și scurtături

- **S / M / L** (butoanele de la hover, sau `Alt+Shift+W`): inele duble (exterior 5h, interior weekly), 5h mereu primar.
- **Prognoză**: din ultima oră de istoric se estimează când ajunge 5h la 0; avertisment dacă vine înainte de reset (`src/lib/forecast.ts`).
- **Notificări** (Setări): ritm prea mare, resetarea ferestrei.
- **Următorul CLI recomandat** (ușor / greu): aceleași reguli ca router-ul din cli-orchestrator (`src/lib/routeHint.ts`).
- **Mod discret** (Setări): aproape invizibil până scade o cotă sau treci cu mouse-ul.
- **Weekly pe model** (Opus/Sonnet): din `seven_day_opus` / `seven_day_sonnet` ale API-ului Claude, afișat ca etichete.
- **Istoric 7 zile** (weekly rămas), păstrat rărit în localStorage (`src/lib/history.ts`).
- **Cost real Claude** (L): tokenii din `~/.claude/projects/**/*.jsonl` (`src-tauri/src/claude_logs.rs`), cost la tarife publice; valoare API echivalentă, nu taxă.
- **Scurtături globale**: `Alt+Shift+Q` arată/ascunde, `Alt+Shift+W` schimbă mărimea, `Alt+Shift+E` schimbă colțul (⌖). Poziția se ține minte.


## Distribuție (Windows + macOS)

**Recomandat: build automat în GitHub Actions** (`.github/workflows/release.yml`). Un singur tag produce installerul Windows (`.exe` NSIS, per-utilizator, fără admin) și macOS (`.dmg` universal, Apple Silicon + Intel); înainte de ambalare rulează `tsc` și `cargo test`, deci un build rupt nu se publică.

```powershell
cd D:\work\ai-quota-widget
git add -A; git commit -m "release 1.0.0"
gh repo create ai-quota-widget --private --source . --push      # sau: git remote add origin ... && git push -u origin master
git tag v1.0.0; git push --tags
```

După ~10–15 minute apare un **draft release** în GitHub (Releases) cu `.exe` și `.dmg`; îl publici și trimiți linkul. Destinatarii citesc `INSTALL.md` (SmartScreen / Gatekeeper, Keychain, CLI-uri autentificate).

**Build local (doar Windows):** `yarn install; yarn tauri build` → `src-tauri\target\release\bundle\nsis\*.exe`. Build-ul macOS se face doar pe un Mac sau în CI.

**Semnare:** fără certificate, Windows afișează SmartScreen, iar macOS blochează prima pornire (Gatekeeper) — ocolibile după `INSTALL.md`. Pentru instalare fără avertismente: Apple Developer (99 USD/an) pentru semnare + notarizare (secretele sunt pregătite, comentate, în workflow) și un certificat de code-signing pentru Windows.

**Pe macOS** widget-ul își completează singur `PATH` (shell de login + Homebrew/npm/nvm) ca să găsească `claude`/`codex`/`agy`, citește tokenul Claude din Keychain (cere o aprobare la prima rulare) și rulează fără icoană în Dock.
