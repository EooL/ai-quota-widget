//! Publică ultimele cote într-un fișier JSON citit de `cli-orchestrator`.
//!
//! Locație: `$ORCH_HOME/quota-snapshot.json` (implicit `~/.cli-orchestrator/`), aceeași
//! rădăcină ca `limits.json`. Scrierea e atomică (fișier temporar + rename), iar orice
//! eroare e ignorată: widget-ul nu trebuie să cadă pentru că nu poate scrie snapshot-ul.
//!
//! Formatul (versiunea 1) este contractul cu orchestratorul; vezi `orch/quota.py`.

use crate::models::{MetricsPayload, PlatformMetric, PlatformStatus, WindowQuota};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const FILE_NAME: &str = "quota-snapshot.json";
pub const VERSION: i64 = 1;

/// Rădăcina de stare a orchestratorului (`$ORCH_HOME` sau `~/.cli-orchestrator`).
pub fn orch_home() -> Option<PathBuf> {
    match std::env::var_os("ORCH_HOME") {
        Some(dir) if !dir.is_empty() => Some(PathBuf::from(dir)),
        _ => Some(dirs::home_dir()?.join(".cli-orchestrator")),
    }
}

pub fn snapshot_path() -> Option<PathBuf> {
    Some(orch_home()?.join(FILE_NAME))
}

fn status_name(s: &PlatformStatus) -> &'static str {
    match s {
        PlatformStatus::Available => "available",
        PlatformStatus::Warning => "warning",
        PlatformStatus::Blocked => "blocked",
        PlatformStatus::Unknown => "unknown",
    }
}

fn window_json(w: &Option<WindowQuota>) -> Value {
    match w {
        Some(q) if q.limit > 0.0 => json!({
            "remaining_pct": ((q.remaining / q.limit) * 100.0).clamp(0.0, 100.0),
            "reset_at_ms": q.reset_at_ms,
        }),
        _ => Value::Null,
    }
}

fn platform_json(cli: &str, m: &PlatformMetric) -> Value {
    json!({
        "cli": cli,
        "label": m.label,
        "status": status_name(&m.status),
        "source": m.source,
        "fetched_at_ms": m.fetched_at_ms,
        "error": m.error,
        "short": window_json(&m.short_window),
        "weekly": window_json(&m.weekly_window),
    })
}

pub fn snapshot_json(payload: &MetricsPayload, now_ms: i64) -> Value {
    json!({
        "version": VERSION,
        "written_at_ms": now_ms,
        "platforms": {
            "claude": platform_json("claude", &payload.claude),
            "chatgpt": platform_json("codex", &payload.chatgpt),
            "gemini": platform_json("agy", &payload.gemini),
        }
    })
}

pub fn write_atomic(path: &Path, value: &Value) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!("json.tmp{}", std::process::id()));
    let body = serde_json::to_vec_pretty(value).map_err(std::io::Error::other)?;
    if let Err(e) = std::fs::write(&tmp, body).and_then(|_| std::fs::rename(&tmp, path)) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

/// Best-effort: apelat după fiecare rundă de polling.
pub fn write_snapshot(payload: &MetricsPayload) {
    if let Some(path) = snapshot_path() {
        let _ = write_atomic(&path, &snapshot_json(payload, crate::models::now_ms()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metric(id: &str, short: Option<(f64, f64)>, weekly: Option<(f64, f64)>) -> PlatformMetric {
        let w = |p: Option<(f64, f64)>| {
            p.map(|(remaining, limit)| WindowQuota { reset_at_ms: 1_000, remaining, limit })
        };
        PlatformMetric::with_windows(id, id, "test", w(short), w(weekly))
    }

    fn payload() -> MetricsPayload {
        MetricsPayload {
            chatgpt: metric("chatgpt", Some((28.0, 40.0)), Some((0.0, 100.0))),
            gemini: PlatformMetric::unavailable("gemini", "Gemini", "agy lipsește"),
            claude: metric("claude", Some((62.5, 100.0)), None),
        }
    }

    #[test]
    fn maps_platforms_to_cli_names_and_percentages() {
        let v = snapshot_json(&payload(), 42);
        assert_eq!(v["version"], 1);
        assert_eq!(v["written_at_ms"], 42);
        assert_eq!(v["platforms"]["claude"]["cli"], "claude");
        assert_eq!(v["platforms"]["chatgpt"]["cli"], "codex");
        assert_eq!(v["platforms"]["gemini"]["cli"], "agy");
        assert_eq!(v["platforms"]["claude"]["short"]["remaining_pct"], 62.5);
        assert_eq!(v["platforms"]["claude"]["weekly"], Value::Null);
        // 28/40 -> 70%, nu 28: ferestrele cu limite absolute se normalizează
        assert_eq!(v["platforms"]["chatgpt"]["short"]["remaining_pct"], 70.0);
        assert_eq!(v["platforms"]["chatgpt"]["status"], "blocked"); // weekly 0 rămas
        assert_eq!(v["platforms"]["gemini"]["status"], "unknown");
        assert_eq!(v["platforms"]["gemini"]["error"], "agy lipsește");
    }

    #[test]
    fn atomic_write_replaces_and_leaves_no_temp_files() {
        let dir = std::env::temp_dir().join(format!("aqw-snap-{}", std::process::id()));
        let path = dir.join("nested").join(FILE_NAME);
        write_atomic(&path, &json!({"a": 1})).unwrap();
        write_atomic(&path, &snapshot_json(&payload(), 7)).unwrap(); // peste existent
        let back: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(back["written_at_ms"], 7);
        let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp"))
            .collect();
        assert!(leftovers.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
