//! Consumul real de tokeni din Claude Code, agregat din jurnalele locale
//! (`~/.claude/projects/**/*.jsonl`). Doar citire; nimic nu pleacă din calculator.
//! Costul în dolari îl calculează frontend-ul (prețurile modelelor trăiesc într-un singur loc).

use chrono::{DateTime, FixedOffset};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

const DAYS: u64 = 31;
const CACHE_TTL_MS: i64 = 5 * 60_000;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageRow {
    pub day: String,
    pub model: String,
    pub input: u64,
    pub output: u64,
    pub cache_write: u64,
    pub cache_read: u64,
}

pub type Acc = BTreeMap<(String, String), UsageRow>;

static CACHE: Mutex<Option<(i64, Vec<UsageRow>)>> = Mutex::new(None);

fn tokens(usage: &Value, key: &str) -> u64 {
    usage.get(key).and_then(Value::as_u64).unwrap_or(0)
}

/// Adaugă o linie de jurnal în agregat. Liniile fără `message.usage`/`model`/`timestamp` se ignoră.
pub fn ingest_line(line: &str, offset_secs: i32, seen: &mut HashSet<String>, acc: &mut Acc) {
    if !line.contains("\"usage\"") {
        return;
    }
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return;
    };
    let Some(message) = v.get("message") else {
        return;
    };
    let Some(usage) = message.get("usage") else {
        return;
    };
    let Some(model) = message.get("model").and_then(Value::as_str) else {
        return;
    };
    if model.starts_with('<') {
        return; // "<synthetic>"
    }
    let Some(ts) = v.get("timestamp").and_then(Value::as_str) else {
        return;
    };
    let Ok(dt) = DateTime::parse_from_rfc3339(ts) else {
        return;
    };
    let Some(tz) = FixedOffset::east_opt(offset_secs) else {
        return;
    };
    // Același mesaj e scris de mai multe ori (streaming, sesiuni reluate): îl numărăm o dată.
    if let (Some(mid), Some(rid)) = (
        message.get("id").and_then(Value::as_str),
        v.get("requestId").and_then(Value::as_str),
    ) {
        if !seen.insert(format!("{mid}:{rid}")) {
            return;
        }
    }
    let day = dt.with_timezone(&tz).format("%Y-%m-%d").to_string();
    let row = acc
        .entry((day.clone(), model.to_string()))
        .or_insert_with(|| UsageRow {
            day,
            model: model.to_string(),
            input: 0,
            output: 0,
            cache_write: 0,
            cache_read: 0,
        });
    row.input += tokens(usage, "input_tokens");
    row.output += tokens(usage, "output_tokens");
    row.cache_write += tokens(usage, "cache_creation_input_tokens");
    row.cache_read += tokens(usage, "cache_read_input_tokens");
}

fn collect(dir: &Path, cutoff: SystemTime, out: &mut Vec<PathBuf>, depth: u32) {
    if depth > 6 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if meta.is_dir() {
            collect(&path, cutoff, out, depth + 1);
        } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl")
            && meta.modified().map(|m| m >= cutoff).unwrap_or(true)
        {
            out.push(path);
        }
    }
}

fn roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(dir) = std::env::var("CLAUDE_CONFIG_DIR") {
        if !dir.trim().is_empty() {
            roots.push(PathBuf::from(dir).join("projects"));
        }
    }
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join(".claude").join("projects"));
        roots.push(home.join(".config").join("claude").join("projects"));
    }
    roots.dedup();
    roots
}

/// Agregă toate jurnalele din `roots` modificate în ultimele `days` zile.
pub fn scan(roots: &[PathBuf], days: u64, offset_secs: i32) -> Vec<UsageRow> {
    let cutoff = SystemTime::now()
        .checked_sub(Duration::from_secs(days * 86_400))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let mut files = Vec::new();
    for root in roots {
        collect(root, cutoff, &mut files, 0);
    }
    files.sort();
    files.dedup();
    let mut seen = HashSet::new();
    let mut acc = Acc::new();
    for path in files {
        let Ok(file) = std::fs::File::open(&path) else {
            continue;
        };
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            ingest_line(&line, offset_secs, &mut seen, &mut acc);
        }
    }
    let oldest_day = {
        let tz = FixedOffset::east_opt(offset_secs);
        let t = chrono::Utc::now() - chrono::Duration::days(days as i64);
        tz.map(|tz| t.with_timezone(&tz).format("%Y-%m-%d").to_string())
            .unwrap_or_default()
    };
    acc.into_values().filter(|r| r.day >= oldest_day).collect()
}

/// Rânduri (zi, model) pe ultimele 31 de zile; rezultatul se reține 5 minute.
pub fn totals() -> Vec<UsageRow> {
    let now = crate::models::now_ms();
    if let Ok(guard) = CACHE.lock() {
        if let Some((at, rows)) = guard.as_ref() {
            if now - at < CACHE_TTL_MS {
                return rows.clone();
            }
        }
    }
    let offset = chrono::Local::now().offset().local_minus_utc();
    let rows = scan(&roots(), DAYS, offset);
    if let Ok(mut guard) = CACHE.lock() {
        *guard = Some((now, rows.clone()));
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(id: &str, req: &str, model: &str, ts: &str, input: u64, output: u64) -> String {
        format!(
            r#"{{"type":"assistant","timestamp":"{ts}","requestId":"{req}","message":{{"id":"{id}","model":"{model}","usage":{{"input_tokens":{input},"output_tokens":{output},"cache_creation_input_tokens":10,"cache_read_input_tokens":100}}}}}}"#
        )
    }

    #[test]
    fn aggregates_per_day_and_model_and_counts_duplicates_once() {
        let mut seen = HashSet::new();
        let mut acc = Acc::new();
        let a = line("m1", "r1", "claude-opus-5-5", "2026-10-05T10:00:00Z", 5, 50);
        ingest_line(&a, 0, &mut seen, &mut acc);
        ingest_line(&a, 0, &mut seen, &mut acc); // duplicat
        ingest_line(&line("m2", "r2", "claude-opus-5-5", "2026-10-05T11:00:00Z", 7, 70), 0, &mut seen, &mut acc);
        ingest_line(&line("m3", "r3", "claude-haiku-4-5-20251001", "2026-10-05T11:00:00Z", 1, 2), 0, &mut seen, &mut acc);
        assert_eq!(acc.len(), 2);
        let opus = &acc[&("2026-10-05".to_string(), "claude-opus-5-5".to_string())];
        assert_eq!((opus.input, opus.output, opus.cache_write, opus.cache_read), (12, 120, 20, 200));
    }

    #[test]
    fn day_follows_the_local_offset() {
        let mut seen = HashSet::new();
        let mut acc = Acc::new();
        // 23:30 UTC = 02:30 a doua zi la UTC+3
        ingest_line(&line("m1", "r1", "claude-sonnet-5-5", "2026-10-05T23:30:00Z", 1, 1), 3 * 3600, &mut seen, &mut acc);
        assert!(acc.keys().any(|(day, _)| day == "2026-10-06"));
    }

    #[test]
    fn ignores_lines_without_usage_synthetic_models_and_garbage() {
        let mut seen = HashSet::new();
        let mut acc = Acc::new();
        ingest_line("not json with \"usage\"", 0, &mut seen, &mut acc);
        ingest_line(r#"{"type":"user","message":{"content":"hi"}}"#, 0, &mut seen, &mut acc);
        ingest_line(&line("m1", "r1", "<synthetic>", "2026-10-05T10:00:00Z", 1, 1), 0, &mut seen, &mut acc);
        assert!(acc.is_empty());
    }
}
