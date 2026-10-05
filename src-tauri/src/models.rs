use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowQuota {
    pub reset_at_ms: i64,
    pub remaining: f64,
    pub limit: f64,
}

/// Fereastră suplimentară (ex. weekly doar pentru Opus sau Sonnet).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtraWindow {
    pub key: String,
    pub label: String,
    pub window: WindowQuota,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PlatformStatus {
    Available,
    Warning,
    Blocked,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformMetric {
    pub id: String,
    pub label: String,
    pub short_window: Option<WindowQuota>,
    pub weekly_window: Option<WindowQuota>,
    pub status: PlatformStatus,
    pub error: Option<String>,
    pub source: String,
    pub fetched_at_ms: i64,
    #[serde(default)]
    pub extra_windows: Vec<ExtraWindow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsPayload {
    pub chatgpt: PlatformMetric,
    pub gemini: PlatformMetric,
    pub claude: PlatformMetric,
}

impl PlatformMetric {
    pub fn unavailable(id: &str, label: &str, error: impl Into<String>) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            short_window: None,
            weekly_window: None,
            status: PlatformStatus::Unknown,
            error: Some(error.into()),
            source: "none".into(),
            fetched_at_ms: now_ms(),
            extra_windows: Vec::new(),
        }
    }

    pub fn with_windows(
        id: &str,
        label: &str,
        source: &str,
        short_window: Option<WindowQuota>,
        weekly_window: Option<WindowQuota>,
    ) -> Self {
        let status = status_from_windows(&short_window, &weekly_window);
        Self {
            id: id.to_string(),
            label: label.to_string(),
            short_window,
            weekly_window,
            status,
            error: None,
            source: source.to_string(),
            fetched_at_ms: now_ms(),
            extra_windows: Vec::new(),
        }
    }
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub fn status_from_windows(
    short_window: &Option<WindowQuota>,
    weekly_window: &Option<WindowQuota>,
) -> PlatformStatus {
    let windows: Vec<&WindowQuota> = [short_window.as_ref(), weekly_window.as_ref()]
        .into_iter()
        .flatten()
        .collect();
    if windows.is_empty() {
        return PlatformStatus::Unknown;
    }
    if windows.iter().any(|w| w.remaining <= 0.0) {
        return PlatformStatus::Blocked;
    }
    if windows
        .iter()
        .any(|w| w.limit > 0.0 && w.remaining / w.limit <= 0.2)
    {
        return PlatformStatus::Warning;
    }
    PlatformStatus::Available
}

pub fn number_from(obj: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<f64> {
    for key in keys {
        if let Some(v) = obj.get(*key) {
            if let Some(n) = as_f64(v) {
                return Some(n);
            }
        }
    }
    None
}

pub fn as_f64(value: &serde_json::Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_i64().map(|n| n as f64))
        .or_else(|| value.as_u64().map(|n| n as f64))
        .or_else(|| value.as_str()?.parse().ok())
}

pub fn quota_from_object(obj: &serde_json::Map<String, serde_json::Value>) -> Option<WindowQuota> {
    let limit = number_from(
        obj,
        &[
            "limit",
            "cap",
            "max",
            "quota",
            "total",
            "message_cap",
            "max_tokens",
            "allowed",
        ],
    );
    let used = number_from(obj, &["used", "usage", "consumed", "current"]);
    let remaining = number_from(
        obj,
        &[
            "remaining",
            "remain",
            "left",
            "available",
            "num_tokens_remaining",
            "messages_remaining",
        ],
    )
    .or_else(|| match (limit, used) {
        (Some(limit), Some(used)) => Some((limit - used).max(0.0)),
        _ => None,
    });
    let reset_at_ms = parse_reset(obj).unwrap_or_else(|| now_ms() + 4 * 3600 * 1000);
    match (remaining, limit) {
        (Some(remaining), Some(limit)) if limit > 0.0 => Some(WindowQuota {
            reset_at_ms,
            remaining,
            limit,
        }),
        _ => None,
    }
}

fn parse_reset(obj: &serde_json::Map<String, serde_json::Value>) -> Option<i64> {
    for key in [
        "reset_at_ms",
        "resetAtMs",
        "resets_at_ms",
        "resetsAtMs",
        "reset_ms",
    ] {
        if let Some(n) = obj.get(key).and_then(as_f64) {
            return Some(n as i64);
        }
    }
    for key in ["resets_at", "reset_at", "resetAt", "resetsAt", "reset"] {
        if let Some(v) = obj.get(key) {
            if let Some(n) = as_f64(v) {
                if n > 10_000_000_000.0 {
                    return Some(n as i64);
                }
                if n > 10_000_000.0 {
                    return Some((n as i64) * 1000);
                }
                return Some(now_ms() + (n * 1000.0) as i64);
            }
            if let Some(s) = v.as_str() {
                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
                    return Some(dt.timestamp_millis());
                }
            }
        }
    }
    for key in [
        "resets_in_minutes",
        "reset_in_minutes",
        "minutes_until_reset",
    ] {
        if let Some(n) = obj.get(key).and_then(as_f64) {
            return Some(now_ms() + (n * 60_000.0) as i64);
        }
    }
    None
}

pub fn collect_quotas(value: &serde_json::Value) -> (Option<WindowQuota>, Option<WindowQuota>) {
    let mut found = Vec::new();
    walk_quotas(value, &mut found);
    let short = found.first().cloned();
    let weekly = found.get(1).cloned();
    (short, weekly)
}

fn walk_quotas(value: &serde_json::Value, out: &mut Vec<WindowQuota>) {
    if out.len() >= 2 {
        return;
    }
    match value {
        serde_json::Value::Object(map) => {
            if let Some(q) = quota_from_object(map) {
                out.push(q);
            }
            // Prefer well-known nested keys first.
            for key in [
                "primary",
                "five_hour",
                "fiveHour",
                "short",
                "rate_limit",
                "rateLimit",
                "secondary",
                "weekly",
                "week",
            ] {
                if let Some(child) = map.get(key) {
                    walk_quotas(child, out);
                }
            }
            for (key, child) in map {
                if [
                    "primary",
                    "five_hour",
                    "fiveHour",
                    "short",
                    "rate_limit",
                    "rateLimit",
                    "secondary",
                    "weekly",
                    "week",
                ]
                .contains(&key.as_str())
                {
                    continue;
                }
                walk_quotas(child, out);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                walk_quotas(item, out);
            }
        }
        _ => {}
    }
}
