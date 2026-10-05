use super::run_cli;
use crate::models::{collect_quotas, now_ms, PlatformMetric, WindowQuota};
use serde_json::Value;
use tauri::AppHandle;

const ID: &str = "gemini";
const LABEL: &str = "Gemini Models";

pub async fn fetch(app: &AppHandle) -> PlatformMetric {
    let mut errors = Vec::new();

    match try_agy(app).await {
        Ok(metric) => return metric,
        Err(err) => errors.push(format!("cli:agy — {err}")),
    }

    match try_antigravity(app, "antigravity").await {
        Ok(metric) => return metric,
        Err(err) => errors.push(format!("cli:antigravity — {err}")),
    }

    match try_gcloud(app).await {
        Ok(metric) => return metric,
        Err(err) => errors.push(format!("cli:gcloud — {err}")),
    }

    PlatformMetric::unavailable(ID, LABEL, errors.join(" | "))
}

async fn try_agy(app: &AppHandle) -> Result<PlatformMetric, String> {
    let raw = run_cli(app, "agy", &["-p", "/quota", "--output-format", "json"]).await?;
    let json: Value = serde_json::from_str(&raw).map_err(|e| format!("JSON invalid: {e}"))?;
    metric_from_agy(&json)
}

fn metric_from_agy(json: &Value) -> Result<PlatformMetric, String> {
    let groups = json
        .get("command")
        .and_then(|command| command.get("data"))
        .and_then(|data| data.get("groups"))
        .and_then(Value::as_array)
        .ok_or_else(|| "agy output has no command.data.groups".to_string())?;
    let gemini = groups
        .iter()
        .find(|group| group.get("name").and_then(Value::as_str) == Some("Gemini Models"))
        .ok_or_else(|| "agy output has no Gemini Models group".to_string())?;
    let buckets = gemini
        .get("buckets")
        .and_then(Value::as_array)
        .ok_or_else(|| "Gemini group has no buckets".to_string())?;

    let mut short = None;
    let mut weekly = None;
    for bucket in buckets {
        let fraction = bucket
            .get("remaining_fraction")
            .and_then(Value::as_f64)
            .ok_or_else(|| "Gemini bucket without remaining_fraction".to_string())?;
        let reset_at_ms = bucket
            .get("reset_time")
            .and_then(Value::as_str)
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
            .map(|value| value.timestamp_millis())
            .unwrap_or_else(|| now_ms() + 5 * 3600 * 1000);
        let quota = WindowQuota {
            reset_at_ms,
            remaining: (fraction * 100.0).clamp(0.0, 100.0),
            limit: 100.0,
        };
        match bucket.get("window").and_then(Value::as_str) {
            Some("weekly") => weekly = Some(quota),
            Some("5h") => short = Some(quota),
            _ => {}
        }
    }
    if short.is_none() && weekly.is_none() {
        return Err("agy output has no Gemini windows".into());
    }
    Ok(PlatformMetric::with_windows(
        ID, LABEL, "cli:agy", short, weekly,
    ))
}

async fn try_antigravity(app: &AppHandle, command: &str) -> Result<PlatformMetric, String> {
    let raw = run_cli(app, command, &["quota", "--json"]).await?;
    metric_from_json(&raw, "cli:antigravity")
}

async fn try_gcloud(app: &AppHandle) -> Result<PlatformMetric, String> {
    let raw = run_cli(app, "gcloud", &["ai", "quota", "list", "--format=json"]).await?;
    metric_from_json(&raw, "cli:gcloud")
}

fn metric_from_json(raw: &str, source: &str) -> Result<PlatformMetric, String> {
    let json: Value = serde_json::from_str(raw).map_err(|e| format!("JSON invalid: {e}"))?;
    let (short, weekly) = collect_quotas(&json);
    if short.is_none() && weekly.is_none() {
        return Err("response without remaining/limit".into());
    }
    Ok(PlatformMetric::with_windows(
        ID, LABEL, source, short, weekly,
    ))
}
