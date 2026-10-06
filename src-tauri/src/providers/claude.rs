//! Cotele Claude (Pro/Max) din endpoint-ul OAuth folosit de Claude Code pentru `/usage`.
//!
//! Ordinea surselor:
//!   1. `GET https://api.anthropic.com/api/oauth/usage` cu tokenul din `~/.claude/.credentials.json`
//!      (sau `CLAUDE_CODE_OAUTH_TOKEN`, sau Keychain pe macOS). Cu cache de 2 minute și backoff pe 429.
//!   2. Limita observată de `cli-orchestrator` (`~/.cli-orchestrator/limits.json`), doar când (1) nu
//!      are date proaspete.
//!   3. Ultimele date reușite (max 1h), marcate ca vechi în `error`.
//!
//! Widget-ul nu reîmprospătează singur tokenul OAuth: refresh token-ul se rotește la fiecare folosire
//! și l-am putea invalida pe cel al Claude Code. Dacă tokenul e expirat, rulăm `claude auth status`
//! (cel mult o dată la 10 minute) ca Claude Code să-l reîmprospăteze, apoi recitim fișierul.

use super::claude_usage::{
    ledger_block, ledger_windows, parse_credentials, parse_extras, parse_usage, parse_version, CachedView,
    Credentials, LedgerBlock, UsageState, Windows,
};
use super::{http_client, read_json_file, run_cli};
use crate::models::{now_ms, ExtraWindow, PlatformMetric};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use tauri::AppHandle;

const ID: &str = "claude";
const LABEL: &str = "Claude";
const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const SOURCE_HTTP: &str = "http:api.anthropic.com/oauth/usage";
const SOURCE_LEDGER: &str = "ledger:cli-orchestrator";
const FALLBACK_VERSION: &str = "2.0.0";

static STATE: Mutex<UsageState> = Mutex::new(UsageState::new());
static VERSION: Mutex<Option<String>> = Mutex::new(None);
/// Ultimele ferestre pe model primite de la API (se afișează și din cache).
static EXTRAS: Mutex<Vec<ExtraWindow>> = Mutex::new(Vec::new());

fn with_extras(mut metric: PlatformMetric) -> PlatformMetric {
    if metric.source.starts_with(SOURCE_HTTP) {
        if let Ok(extras) = EXTRAS.lock() {
            metric.extra_windows = extras.clone();
        }
    }
    metric
}

/// Starea e doar cache; un panic în alt thread nu trebuie să blocheze widget-ul.
fn lock_state() -> MutexGuard<'static, UsageState> {
    STATE.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn configured_models() -> Vec<super::AvailableModel> {
    let mut candidates = Vec::new();
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join(".claude").join("settings.json"));
        candidates.push(home.join(".claude").join("settings.local.json"));
    }
    if let Ok(appdata) = std::env::var("APPDATA") {
        candidates.push(PathBuf::from(appdata).join("claude").join("settings.json"));
    }
    let from_environment = std::env::var("ANTHROPIC_MODEL").ok();
    let model = from_environment.or_else(|| {
        candidates.iter().find_map(|path| {
            let raw = std::fs::read_to_string(path).ok()?;
            let json: Value = serde_json::from_str(&raw).ok()?;
            json.get("model").and_then(Value::as_str).map(str::to_string)
        })
    });
    model
        .filter(|name| !name.trim().is_empty())
        .map(|id| {
            vec![super::AvailableModel {
                name: id.clone(),
                id,
            }]
        })
        .unwrap_or_default()
}

/// Claude Code nu are o comandă de catalog; lista e fixă (id-urile API publice).
/// Modelul configurat în `settings.json` / `ANTHROPIC_MODEL` apare primul dacă nu e deja în listă.
pub fn models() -> Vec<super::AvailableModel> {
    const KNOWN: [(&str, &str); 4] = [
        ("claude-haiku-4-5-20251001", "Claude Haiku 4.5"),
        ("claude-sonnet-5-5", "Claude Sonnet 5.5"),
        ("claude-opus-5-5", "Claude Opus 5.5"),
        ("claude-fable-5-1", "Claude Fable 5.1"),
    ];
    let mut out: Vec<super::AvailableModel> = KNOWN
        .iter()
        .map(|(id, name)| super::AvailableModel {
            id: id.to_string(),
            name: name.to_string(),
        })
        .collect();
    for configured in configured_models().into_iter().rev() {
        let known = out
            .iter()
            .any(|m| m.id == configured.id || m.name.eq_ignore_ascii_case(&configured.name));
        if !known {
            out.insert(0, configured);
        }
    }
    out
}

enum Plan {
    UseCache(Option<CachedView>, Option<String>),
    Fetch,
}

pub async fn fetch(app: &AppHandle) -> PlatformMetric {
    with_extras(fetch_inner(app).await)
}

async fn fetch_inner(app: &AppHandle) -> PlatformMetric {
    let now = now_ms();
    {
        // După „token expirat”, dacă fișierul are acum un token valid (ex. Claude Code l-a reînnoit),
        // nu mai așteptăm backoff-ul.
        let mut st = lock_state();
        let expired_err = st.last_error.as_deref().map_or(false, |e| e.contains("expired"));
        if expired_err && load_credentials().map_or(false, |c| !c.expired(now)) {
            st.next_attempt_ms = 0;
        }
    }
    let plan = {
        let st = lock_state();
        if st.should_fetch(now) {
            Plan::Fetch
        } else {
            Plan::UseCache(st.view(now), st.last_error.clone())
        }
    };
    if let Plan::UseCache(view, error) = plan {
        return resolve(now, view, error);
    }

    match try_http(app, now).await {
        Ok((windows, subscription)) => {
            lock_state().on_success(windows.clone(), now);
            let source = match subscription {
                Some(plan) => format!("{SOURCE_HTTP} ({plan})"),
                None => SOURCE_HTTP.to_string(),
            };
            PlatformMetric::with_windows(ID, LABEL, &source, windows.0, windows.1)
        }
        Err(failure) => {
            let view = {
                let mut st = lock_state();
                st.on_failure(now, failure.message.clone(), failure.retry_after.as_deref());
                st.view(now)
            };
            resolve(now, view, Some(failure.message))
        }
    }
}

/// Alege ce afișăm când nu tocmai am primit date noi de la API.
fn resolve(now: i64, view: Option<CachedView>, error: Option<String>) -> PlatformMetric {
    if let Some(v) = &view {
        if !v.stale {
            return metric_from_view(v, None);
        }
    }
    if let Some(block) = read_ledger_block(now) {
        let (short, weekly) = ledger_windows(&block);
        let mut metric = PlatformMetric::with_windows(ID, LABEL, SOURCE_LEDGER, short, weekly);
        metric.error =
            error.map(|e| format!("API unavailable ({e}); limit observed by the orchestrator"));
        return metric;
    }
    if let Some(v) = view {
        let reason = error.unwrap_or_else(|| "unknown error".to_string());
        let note = format!("data is {} min old: {reason}", v.age_ms / 60_000);
        return metric_from_view(&v, Some(note));
    }
    PlatformMetric::unavailable(
        ID,
        LABEL,
        error.unwrap_or_else(|| "Claude quota unavailable".to_string()),
    )
}

fn metric_from_view(v: &CachedView, note: Option<String>) -> PlatformMetric {
    let source = if v.stale {
        format!("{SOURCE_HTTP} (cache)")
    } else {
        SOURCE_HTTP.to_string()
    };
    let mut metric =
        PlatformMetric::with_windows(ID, LABEL, &source, v.short.clone(), v.weekly.clone());
    // `with_windows` pune ora curentă; datele din cache trebuie să-și arate vârsta reală.
    metric.fetched_at_ms = now_ms() - v.age_ms;
    metric.error = note;
    metric
}

struct Failure {
    message: String,
    retry_after: Option<String>,
}

impl Failure {
    fn msg(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            retry_after: None,
        }
    }
}

async fn try_http(app: &AppHandle, now: i64) -> Result<(Windows, Option<String>), Failure> {
    let mut creds = load_credentials().map_err(Failure::msg)?;
    if creds.expired(now) {
        let may_nudge = lock_state().may_nudge(now);
        if may_nudge {
            // Efect secundar dorit: Claude Code își reîmprospătează tokenul când e folosit.
            let _ = run_cli(app, "claude", &["auth", "status"]).await;
            creds = load_credentials().map_err(Failure::msg)?;
            if creds.expired(now_ms()) {
                // `auth status` nu atinge mereu API-ul; un prompt minim forțează refresh-ul tokenului.
                let _ = tokio::time::timeout(
                    std::time::Duration::from_secs(90),
                    run_cli(app, "claude", &["-p", "ok", "--max-turns", "1"]),
                )
                .await;
                creds = load_credentials().map_err(Failure::msg)?;
            }
        }
        if creds.expired(now_ms()) {
            return Err(Failure::msg(
                "Claude OAuth token expired; open Claude Code once to refresh it",
            ));
        }
    }

    let user_agent = user_agent(app).await;
    let client = http_client().map_err(Failure::msg)?;
    let response = client
        .get(USAGE_URL)
        .bearer_auth(&creds.access_token)
        .header("anthropic-beta", "oauth-2025-04-20")
        .header("User-Agent", user_agent)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| Failure::msg(e.to_string()))?;

    let status = response.status();
    let retry_after = response
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    if !status.is_success() {
        let hint = if status.as_u16() == 401 || status.as_u16() == 403 {
            " (run `claude auth login`)"
        } else {
            ""
        };
        return Err(Failure {
            message: format!("HTTP {status}{hint}"),
            retry_after,
        });
    }
    let json: Value = response
        .json()
        .await
        .map_err(|e| Failure::msg(format!("JSON invalid: {e}")))?;
    let windows = parse_usage(&json, now_ms()).map_err(Failure::msg)?;
    if let Ok(mut extras) = EXTRAS.lock() {
        *extras = parse_extras(&json, now_ms());
    }
    Ok((windows, creds.subscription))
}

/// Endpoint-ul tratează mult mai sever cererile fără `User-Agent: claude-code/<versiune>`.
/// Suprascris cu `AQW_CLAUDE_UA`.
async fn user_agent(app: &AppHandle) -> String {
    if let Ok(custom) = std::env::var("AQW_CLAUDE_UA") {
        if !custom.trim().is_empty() {
            return custom;
        }
    }
    let cached = VERSION.lock().ok().and_then(|guard| guard.clone());
    let version = match cached {
        Some(v) => v,
        None => {
            let detected = run_cli(app, "claude", &["--version"])
                .await
                .ok()
                .and_then(|text| parse_version(&text))
                .unwrap_or_else(|| FALLBACK_VERSION.to_string());
            if let Ok(mut guard) = VERSION.lock() {
                *guard = Some(detected.clone());
            }
            detected
        }
    };
    format!("claude-code/{version}")
}

fn credential_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(dir) = std::env::var("CLAUDE_CONFIG_DIR") {
        if !dir.trim().is_empty() {
            paths.push(PathBuf::from(dir).join(".credentials.json"));
        }
    }
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".claude").join(".credentials.json"));
    }
    paths
}

fn load_credentials() -> Result<Credentials, String> {
    if let Ok(token) = std::env::var("CLAUDE_CODE_OAUTH_TOKEN") {
        let token = token.trim();
        if !token.is_empty() {
            return Ok(Credentials {
                access_token: token.to_string(),
                expires_at_ms: None,
                subscription: None,
            });
        }
    }
    let mut errors = Vec::new();
    for path in credential_paths() {
        if !path.exists() {
            continue;
        }
        match read_json_file(&path).and_then(|json| parse_credentials(&json)) {
            Ok(creds) => return Ok(creds),
            Err(e) => errors.push(e),
        }
    }
    if cfg!(target_os = "macos") {
        if let Some(raw) = keychain_credentials() {
            match serde_json::from_str::<Value>(&raw)
                .map_err(|e| e.to_string())
                .and_then(|json| parse_credentials(&json))
            {
                Ok(creds) => return Ok(creds),
                Err(e) => errors.push(e),
            }
        }
    }
    if errors.is_empty() {
        Err("missing ~/.claude/.credentials.json (run `claude auth login`)".to_string())
    } else {
        Err(errors.join("; "))
    }
}

fn keychain_credentials() -> Option<String> {
    let output = std::process::Command::new("security")
        .args(["find-generic-password", "-s", "Claude Code-credentials", "-w"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn read_ledger_block(now: i64) -> Option<LedgerBlock> {
    let path = crate::snapshot::orch_home()?.join("limits.json");
    let json = read_json_file(&path).ok()?;
    ledger_block(&json, "claude", now)
}
