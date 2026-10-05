//! Logică pură (fără Tauri) pentru cotele abonamentului Claude.
//!
//! Sursa reală este endpoint-ul folosit de Claude Code pentru `/usage`:
//! `GET https://api.anthropic.com/api/oauth/usage` cu tokenul OAuth din
//! `~/.claude/.credentials.json` (`claudeAiOauth.accessToken`). Răspunsul are
//! `five_hour` și `seven_day`, fiecare cu `utilization` (0–100, procent consumat)
//! și `resets_at` (ISO 8601). Endpoint-ul nu e documentat oficial și poate da 429
//! persistent, deci totul aici e construit în jurul cache-ului și al backoff-ului.
//!
//! Tokenul nu este niciodată logat sau serializat (vezi `Debug` pentru `Credentials`).

use crate::models::{as_f64, WindowQuota};
use serde_json::Value;
use std::fmt;

pub const SHORT_WINDOW_MS: i64 = 5 * 3600 * 1000;
pub const WEEK_WINDOW_MS: i64 = 168 * 3600 * 1000;
/// Interval minim între două cereri reale către API, oricât de des face polling UI-ul.
pub const MIN_FETCH_INTERVAL_MS: i64 = 120_000;
pub const BACKOFF_BASE_MS: i64 = 300_000;
pub const BACKOFF_MAX_MS: i64 = 1_800_000;
pub const RETRY_AFTER_MIN_MS: i64 = 60_000;
pub const RETRY_AFTER_MAX_MS: i64 = 3_600_000;
/// Cât timp mai afișăm date vechi după ce cererile eșuează.
pub const MAX_STALE_MS: i64 = 3_600_000;
pub const EXPIRY_MARGIN_MS: i64 = 60_000;
pub const NUDGE_INTERVAL_MS: i64 = 600_000;

pub type Windows = (Option<WindowQuota>, Option<WindowQuota>);

// ------------------------------------------------------------------ răspuns API
pub fn parse_usage(json: &Value, now: i64) -> Result<Windows, String> {
    let obj = json
        .as_object()
        .ok_or_else(|| "response is not a JSON object".to_string())?;
    if !obj.contains_key("five_hour") && !obj.contains_key("seven_day") {
        return Err("response has no five_hour/seven_day".to_string());
    }
    let short = obj
        .get("five_hour")
        .map(|v| window_from(v, SHORT_WINDOW_MS, now))
        .transpose()?;
    let weekly = obj
        .get("seven_day")
        .map(|v| window_from(v, WEEK_WINDOW_MS, now))
        .transpose()?;
    Ok((short, weekly))
}

/// Ferestre weekly pe model (`seven_day_opus`, `seven_day_sonnet`). `null` = fereastra nu a pornit, se omite.
pub fn parse_extras(json: &Value, now: i64) -> Vec<crate::models::ExtraWindow> {
    const KNOWN: [(&str, &str); 2] = [("seven_day_opus", "Opus"), ("seven_day_sonnet", "Sonnet")];
    KNOWN
        .iter()
        .filter_map(|(key, label)| {
            let v = json.get(*key)?;
            if v.is_null() {
                return None;
            }
            let window = window_from(v, WEEK_WINDOW_MS, now).ok()?;
            Some(crate::models::ExtraWindow {
                key: key.to_string(),
                label: label.to_string(),
                window,
            })
        })
        .collect()
}

fn window_from(v: &Value, default_len_ms: i64, now: i64) -> Result<WindowQuota, String> {
    // `null` = fereastra nu a început încă: cota e întreagă.
    if v.is_null() {
        return Ok(WindowQuota {
            reset_at_ms: now + default_len_ms,
            remaining: 100.0,
            limit: 100.0,
        });
    }
    let used = v
        .get("utilization")
        .and_then(as_f64)
        .ok_or_else(|| "window without utilization".to_string())?;
    let reset_at_ms = v
        .get("resets_at")
        .and_then(Value::as_str)
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.timestamp_millis())
        .unwrap_or(now + default_len_ms);
    Ok(WindowQuota {
        reset_at_ms,
        remaining: (100.0 - used).clamp(0.0, 100.0),
        limit: 100.0,
    })
}

// ------------------------------------------------------------------ credențiale
#[derive(Clone)]
pub struct Credentials {
    pub access_token: String,
    pub expires_at_ms: Option<i64>,
    pub subscription: Option<String>,
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("access_token", &"<redacted>")
            .field("expires_at_ms", &self.expires_at_ms)
            .field("subscription", &self.subscription)
            .finish()
    }
}

impl Credentials {
    pub fn expired(&self, now: i64) -> bool {
        self.expires_at_ms.map_or(false, |e| e <= now + EXPIRY_MARGIN_MS)
    }
}

pub fn parse_credentials(json: &Value) -> Result<Credentials, String> {
    let root = json.get("claudeAiOauth").unwrap_or(json);
    let token = root
        .get("accessToken")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Claude credentials have no accessToken".to_string())?;
    let expires_at_ms = root.get("expiresAt").and_then(as_f64).map(|n| {
        let n = n as i64;
        // unele versiuni scriu secunde în loc de milisecunde
        if n < 10_000_000_000 { n * 1000 } else { n }
    });
    let subscription = root
        .get("subscriptionType")
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok(Credentials {
        access_token: token.to_string(),
        expires_at_ms,
        subscription,
    })
}

/// "2.0.31 (Claude Code)" -> "2.0.31"
pub fn parse_version(text: &str) -> Option<String> {
    text.split_whitespace()
        .find(|t| {
            t.matches('.').count() == 2
                && t.chars().all(|c| c.is_ascii_digit() || c == '.')
                && !t.starts_with('.')
                && !t.ends_with('.')
        })
        .map(str::to_string)
}

// ---------------------------------------------------------------------- backoff
pub fn backoff_ms(retry_after: Option<&str>, failures: u32) -> i64 {
    if let Some(secs) = retry_after
        .and_then(|s| s.trim().parse::<i64>().ok())
        .filter(|s| *s > 0)
    {
        return (secs * 1000).clamp(RETRY_AFTER_MIN_MS, RETRY_AFTER_MAX_MS);
    }
    // `retry-after: 0` (observat în practică la 429) nu înseamnă „reîncearcă acum”.
    let exp = failures.saturating_sub(1).min(6);
    (BACKOFF_BASE_MS << exp).min(BACKOFF_MAX_MS)
}

#[derive(Debug, Clone)]
pub struct CachedUsage {
    pub short: Option<WindowQuota>,
    pub weekly: Option<WindowQuota>,
    pub fetched_ms: i64,
}

#[derive(Debug, Clone)]
pub struct CachedView {
    pub short: Option<WindowQuota>,
    pub weekly: Option<WindowQuota>,
    pub age_ms: i64,
    /// `true` dacă ultima încercare a eșuat și datele sunt cele vechi.
    pub stale: bool,
}

#[derive(Debug)]
pub struct UsageState {
    pub last_ok: Option<CachedUsage>,
    pub next_attempt_ms: i64,
    pub failures: u32,
    pub last_error: Option<String>,
    pub last_nudge_ms: i64,
}

impl UsageState {
    pub const fn new() -> Self {
        Self {
            last_ok: None,
            next_attempt_ms: 0,
            failures: 0,
            last_error: None,
            last_nudge_ms: 0,
        }
    }

    pub fn should_fetch(&self, now: i64) -> bool {
        now >= self.next_attempt_ms
    }

    pub fn on_success(&mut self, windows: Windows, now: i64) {
        self.last_ok = Some(CachedUsage {
            short: windows.0,
            weekly: windows.1,
            fetched_ms: now,
        });
        self.failures = 0;
        self.last_error = None;
        self.next_attempt_ms = now + MIN_FETCH_INTERVAL_MS;
    }

    pub fn on_failure(&mut self, now: i64, error: String, retry_after: Option<&str>) {
        self.failures = self.failures.saturating_add(1);
        self.next_attempt_ms = now + backoff_ms(retry_after, self.failures);
        self.last_error = Some(error);
    }

    /// Datele din cache. Când sunt vechi (după eșec), ferestrele deja resetate
    /// sunt eliminate, iar după `MAX_STALE_MS` nu se mai afișează nimic.
    pub fn view(&self, now: i64) -> Option<CachedView> {
        let c = self.last_ok.as_ref()?;
        let stale = self.last_error.is_some();
        let age_ms = (now - c.fetched_ms).max(0);
        if !stale {
            return Some(CachedView {
                short: c.short.clone(),
                weekly: c.weekly.clone(),
                age_ms,
                stale,
            });
        }
        if age_ms > MAX_STALE_MS {
            return None;
        }
        let live = |w: &Option<WindowQuota>| w.clone().filter(|q| q.reset_at_ms > now);
        let (short, weekly) = (live(&c.short), live(&c.weekly));
        if short.is_none() && weekly.is_none() {
            return None;
        }
        Some(CachedView { short, weekly, age_ms, stale })
    }

    /// Cel mult o dată la `NUDGE_INTERVAL_MS`.
    pub fn may_nudge(&mut self, now: i64) -> bool {
        if now - self.last_nudge_ms >= NUDGE_INTERVAL_MS {
            self.last_nudge_ms = now;
            true
        } else {
            false
        }
    }
}

// ----------------------------------------------- ledger cli-orchestrator (fallback)
#[derive(Debug, Clone, PartialEq)]
pub struct LedgerBlock {
    pub kind: String,
    pub until_ms: i64,
}

/// `limits.json` din `~/.cli-orchestrator`: `{ "claude": {"kind": "5h", "blocked_until": "..."} }`.
pub fn ledger_block(json: &Value, cli: &str, now: i64) -> Option<LedgerBlock> {
    let entry = json.get(cli)?;
    let until = entry.get("blocked_until")?.as_str()?;
    let until_ms = chrono::DateTime::parse_from_rfc3339(until).ok()?.timestamp_millis();
    if until_ms <= now {
        return None;
    }
    let kind = entry
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    Some(LedgerBlock { kind, until_ms })
}

pub fn ledger_windows(block: &LedgerBlock) -> Windows {
    let exhausted = WindowQuota {
        reset_at_ms: block.until_ms,
        remaining: 0.0,
        limit: 100.0,
    };
    if block.kind == "weekly" {
        (None, Some(exhausted))
    } else {
        (Some(exhausted), None)
    }
}

// ------------------------------------------------------------------------ teste
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const NOW: i64 = 1_790_000_000_000;

    #[test]
    fn parses_typical_usage() {
        let j = json!({
            "five_hour": {"utilization": 37.5, "resets_at": "2026-10-05T20:00:00.123456+00:00"},
            "seven_day": {"utilization": 12, "resets_at": "2026-10-09T10:00:00Z"},
            "seven_day_opus": null,
            "extra_usage": {"is_enabled": false}
        });
        let (s, w) = parse_usage(&j, NOW).unwrap();
        let (s, w) = (s.unwrap(), w.unwrap());
        assert_eq!(s.remaining, 62.5);
        assert_eq!(s.limit, 100.0);
        assert_eq!(w.remaining, 88.0);
        assert_eq!(
            s.reset_at_ms,
            chrono::DateTime::parse_from_rfc3339("2026-10-05T20:00:00.123456+00:00")
                .unwrap()
                .timestamp_millis()
        );
    }

    #[test]
    fn null_window_means_full_quota() {
        let j = json!({"five_hour": null, "seven_day": {"utilization": 90.0, "resets_at": null}});
        let (s, w) = parse_usage(&j, NOW).unwrap();
        let s = s.unwrap();
        assert_eq!(s.remaining, 100.0);
        assert_eq!(s.reset_at_ms, NOW + SHORT_WINDOW_MS);
        let w = w.unwrap();
        assert_eq!(w.remaining, 10.0);
        assert_eq!(w.reset_at_ms, NOW + WEEK_WINDOW_MS); // resets_at lipsă -> implicit
    }

    #[test]
    fn utilization_is_clamped_and_accepts_strings() {
        let j = json!({"five_hour": {"utilization": 140, "resets_at": null},
                       "seven_day": {"utilization": "25", "resets_at": null}});
        let (s, w) = parse_usage(&j, NOW).unwrap();
        assert_eq!(s.unwrap().remaining, 0.0);
        assert_eq!(w.unwrap().remaining, 75.0);
    }

    #[test]
    fn rejects_unexpected_shapes() {
        assert!(parse_usage(&json!({"error": {"type": "rate_limit_error"}}), NOW).is_err());
        assert!(parse_usage(&json!([1, 2]), NOW).is_err());
        assert!(parse_usage(&json!({"five_hour": {"resets_at": null}}), NOW).is_err());
    }

    #[test]
    fn credentials_nested_flat_and_missing() {
        let nested = json!({"claudeAiOauth": {"accessToken": "tok", "expiresAt": 1790000600000i64,
                                              "subscriptionType": "max"}});
        let c = parse_credentials(&nested).unwrap();
        assert_eq!(c.access_token, "tok");
        assert_eq!(c.subscription.as_deref(), Some("max"));
        assert!(!c.expired(NOW));
        assert!(c.expired(NOW + 600_000)); // în marja de 60s înainte de expirare

        let flat = json!({"accessToken": "t2", "expiresAt": 1_790_000_000i64}); // secunde
        assert_eq!(parse_credentials(&flat).unwrap().expires_at_ms, Some(1_790_000_000_000));

        assert!(parse_credentials(&json!({"claudeAiOauth": {}})).is_err());
        assert!(parse_credentials(&json!({"accessToken": ""})).is_err());
        // fără expiresAt nu putem ști: nu îl declarăm expirat
        assert!(!parse_credentials(&json!({"accessToken": "x"})).unwrap().expired(NOW));
    }

    #[test]
    fn credentials_debug_never_leaks_token() {
        let c = parse_credentials(&json!({"accessToken": "sk-ant-oat01-SECRET"})).unwrap();
        let shown = format!("{c:?}");
        assert!(!shown.contains("SECRET"));
        assert!(shown.contains("redacted"));
    }

    #[test]
    fn version_parsing() {
        assert_eq!(parse_version("2.0.31 (Claude Code)").as_deref(), Some("2.0.31"));
        assert_eq!(parse_version("claude 1.2.3\n").as_deref(), Some("1.2.3"));
        assert_eq!(parse_version("no version here"), None);
        assert_eq!(parse_version("1.2"), None);
    }

    #[test]
    fn backoff_rules() {
        // retry-after: 0 sau lipsă -> backoff exponențial, plafonat
        assert_eq!(backoff_ms(Some("0"), 1), 300_000);
        assert_eq!(backoff_ms(None, 1), 300_000);
        assert_eq!(backoff_ms(None, 2), 600_000);
        assert_eq!(backoff_ms(None, 3), 1_200_000);
        assert_eq!(backoff_ms(None, 4), 1_800_000);
        assert_eq!(backoff_ms(None, 50), 1_800_000);
        // retry-after explicit, cu podea și plafon
        assert_eq!(backoff_ms(Some("5"), 1), 60_000);
        assert_eq!(backoff_ms(Some("600"), 1), 600_000);
        assert_eq!(backoff_ms(Some("999999"), 1), 3_600_000);
        assert_eq!(backoff_ms(Some("abc"), 1), 300_000);
    }

    fn win(remaining: f64, reset: i64) -> Option<WindowQuota> {
        Some(WindowQuota { reset_at_ms: reset, remaining, limit: 100.0 })
    }

    #[test]
    fn state_throttles_real_requests_after_success() {
        let mut st = UsageState::new();
        assert!(st.should_fetch(NOW));
        st.on_success((win(60.0, NOW + 1000), None), NOW);
        assert!(!st.should_fetch(NOW + 60_000));
        assert!(st.should_fetch(NOW + MIN_FETCH_INTERVAL_MS));
        let v = st.view(NOW + 60_000).unwrap();
        assert!(!v.stale);
        assert_eq!(v.age_ms, 60_000);
    }

    #[test]
    fn state_backs_off_after_failure_and_serves_stale() {
        let mut st = UsageState::new();
        st.on_success((win(60.0, NOW + 3_600_000), win(40.0, NOW + 100_000)), NOW);
        st.on_failure(NOW + 200_000, "HTTP 429".into(), Some("0"));
        assert!(!st.should_fetch(NOW + 200_000 + 299_000));
        assert!(st.should_fetch(NOW + 200_000 + 300_000));
        // fereastra weekly a expirat între timp -> dispare, short rămâne
        let v = st.view(NOW + 200_000).unwrap();
        assert!(v.stale);
        assert!(v.short.is_some());
        assert!(v.weekly.is_none());
        // eșecuri repetate -> backoff crește
        st.on_failure(NOW + 600_000, "HTTP 429".into(), None);
        assert_eq!(st.failures, 2);
        // succes -> resetează contorul și eroarea
        st.on_success((win(55.0, NOW + 3_600_000), None), NOW + 900_000);
        assert_eq!(st.failures, 0);
        assert!(st.last_error.is_none());
    }

    #[test]
    fn stale_data_expires() {
        let mut st = UsageState::new();
        st.on_success((win(60.0, NOW + 10 * 3_600_000), None), NOW);
        st.on_failure(NOW + 1, "timeout".into(), None);
        assert!(st.view(NOW + MAX_STALE_MS).is_some());
        assert!(st.view(NOW + MAX_STALE_MS + 1).is_none());
        // fără nicio reușită anterioară nu există ce afișa
        assert!(UsageState::new().view(NOW).is_none());
    }

    #[test]
    fn nudge_is_rate_limited() {
        let mut st = UsageState::new();
        assert!(st.may_nudge(NOW));
        assert!(!st.may_nudge(NOW + 1000));
        assert!(st.may_nudge(NOW + NUDGE_INTERVAL_MS));
    }

    #[test]
    fn ledger_active_expired_and_missing() {
        let until = chrono::DateTime::parse_from_rfc3339("2026-10-05T21:00:00+03:00")
            .unwrap()
            .timestamp_millis();
        let j = json!({
            "claude": {"kind": "weekly", "blocked_until": "2026-10-05T21:00:00+03:00"},
            "codex": {"kind": "5h", "blocked_until": "2020-01-01T00:00:00+00:00"}
        });
        let b = ledger_block(&j, "claude", until - 1000).unwrap();
        assert_eq!(b, LedgerBlock { kind: "weekly".into(), until_ms: until });
        assert!(ledger_block(&j, "claude", until).is_none()); // expirat
        assert!(ledger_block(&j, "codex", until - 1000).is_none());
        assert!(ledger_block(&j, "agy", until - 1000).is_none());
        assert!(ledger_block(&json!({"claude": {"kind": "5h"}}), "claude", NOW).is_none());
    }

    #[test]
    fn ledger_windows_follow_kind() {
        let weekly = ledger_windows(&LedgerBlock { kind: "weekly".into(), until_ms: NOW + 5 });
        assert!(weekly.0.is_none() && weekly.1.as_ref().unwrap().remaining == 0.0);
        let five = ledger_windows(&LedgerBlock { kind: "5h".into(), until_ms: NOW + 5 });
        assert!(five.1.is_none() && five.0.as_ref().unwrap().reset_at_ms == NOW + 5);
        let unknown = ledger_windows(&LedgerBlock { kind: "unknown".into(), until_ms: NOW + 5 });
        assert!(unknown.0.is_some());
    }

    #[test]
    fn extras_skip_null_and_parse_model_windows() {
        let json = serde_json::json!({
            "five_hour": {"utilization": 10.0, "resets_at": "2026-10-05T20:00:00Z"},
            "seven_day": {"utilization": 5.0, "resets_at": "2026-10-09T20:00:00Z"},
            "seven_day_opus": {"utilization": 60.0, "resets_at": "2026-10-09T20:00:00Z"},
            "seven_day_sonnet": null
        });
        let extras = parse_extras(&json, 0);
        assert_eq!(extras.len(), 1);
        assert_eq!(extras[0].label, "Opus");
        assert!((extras[0].window.remaining - 40.0).abs() < 1e-9);
    }
}
