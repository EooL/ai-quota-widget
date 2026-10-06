use super::{extract_string, first_existing, home_config_paths, http_client};
use crate::models::{collect_quotas, PlatformMetric};
use serde_json::Value;
use std::path::PathBuf;
use tauri::AppHandle;

const ID: &str = "chatgpt";
const LABEL: &str = "ChatGPT Plus";

pub async fn fetch(_app: &AppHandle) -> PlatformMetric {
    let mut errors = Vec::new();

    match try_wham_usage().await {
        Ok(metric) => return metric,
        Err(err) => errors.push(format!("http:chatgpt.com/wham — {err}")),
    }

    match try_http().await {
        Ok(metric) => return metric,
        Err(err) => errors.push(format!("http:chatgpt.com — {err}")),
    }

    PlatformMetric::unavailable(ID, LABEL, errors.join(" | "))
}

async fn try_wham_usage() -> Result<PlatformMetric, String> {
    let token = load_openai_token()?;
    let account = load_account_id();
    let client = http_client()?;
    let mut request = client
        .get("https://chatgpt.com/backend-api/wham/usage")
        .bearer_auth(token)
        .header("User-Agent", "codex_cli_rs/0.160.1")
        .header("Accept", "application/json");
    if let Some(id) = account {
        request = request.header("ChatGPT-Account-Id", id);
    }
    let response = request
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    let json: Value = response.json().await.map_err(|e| e.to_string())?;
    let rate_limit = json
        .get("rate_limit")
        .ok_or_else(|| "response has no rate_limit".to_string())?;
    let short = quota_from_window(rate_limit.get("primary_window"))?;
    let weekly = quota_from_window(rate_limit.get("secondary_window"))?;
    Ok(PlatformMetric::with_windows(
        ID,
        LABEL,
        "http:chatgpt.com/wham",
        short,
        weekly,
    ))
}

fn quota_from_window(window: Option<&Value>) -> Result<Option<crate::models::WindowQuota>, String> {
    // `secondary_window: null` este un răspuns valid (cont fără fereastră săptămânală).
    let Some(window) = window.filter(|w| !w.is_null()) else {
        return Ok(None);
    };
    let used = window
        .get("used_percent")
        .and_then(Value::as_f64)
        .ok_or_else(|| "window without used_percent".to_string())?;
    let reset_at_ms = window
        .get("reset_at")
        .and_then(Value::as_i64)
        .map(|seconds| seconds * 1000)
        .ok_or_else(|| "window without reset_at".to_string())?;
    Ok(Some(crate::models::WindowQuota {
        reset_at_ms,
        remaining: (100.0 - used).clamp(0.0, 100.0),
        limit: 100.0,
    }))
}

async fn try_http() -> Result<PlatformMetric, String> {
    let token = load_openai_token()?;
    let client = http_client()?;
    let response = client
        .get("https://chatgpt.com/backend-api/conversation_limit")
        .bearer_auth(token)
        .header("User-Agent", "ai-quota-widget/0.1")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    let json: Value = response.json().await.map_err(|e| e.to_string())?;
    let (short, weekly) = collect_quotas(&json);
    if short.is_none() && weekly.is_none() {
        return Err("response has no quota windows".into());
    }
    Ok(PlatformMetric::with_windows(
        ID,
        LABEL,
        "http:chatgpt.com",
        short,
        weekly,
    ))
}

fn auth_paths() -> Vec<PathBuf> {
    let mut paths = home_config_paths(&["openai", "auth.json"]);
    if let Some(home) = dirs::home_dir() {
        paths.insert(0, home.join(".codex").join("auth.json"));
    }
    paths
}

/// `ChatGPT-Account-Id` îl trimite și Codex CLI; fără el unele conturi primesc 403.
fn load_account_id() -> Option<String> {
    let path = first_existing(&auth_paths())?;
    let json = super::read_json_file(&path).ok()?;
    extract_string(&json, &["account_id", "chatgpt_account_id"])
}

fn load_openai_token() -> Result<String, String> {
    let mut paths = home_config_paths(&["openai", "auth.json"]);
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".codex").join("auth.json"));
        paths.push(home.join(".config/openai/auth.json"));
        paths.push(home.join(".openai/auth.json"));
    }
    if let Some(config) = dirs::config_dir() {
        paths.push(config.join("openai").join("auth.json"));
    }
    if let Ok(appdata) = std::env::var("APPDATA") {
        paths.push(PathBuf::from(appdata).join("openai").join("auth.json"));
    }

    let path = first_existing(&paths).ok_or_else(|| {
        "~/.config/openai/auth.json is missing (ChatGPT token unavailable)".to_string()
    })?;
    let json = super::read_json_file(&path)?;
    extract_string(
        &json,
        &[
            "access_token",
            "accessToken",
            "token",
            "apiKey",
            "api_key",
            "session_token",
        ],
    )
    .ok_or_else(|| "auth.json has no access_token".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn null_secondary_window_is_not_an_error() {
        assert!(quota_from_window(Some(&Value::Null)).unwrap().is_none());
        assert!(quota_from_window(None).unwrap().is_none());
    }

    #[test]
    fn primary_window_is_parsed() {
        let w = json!({"used_percent": 7.0, "reset_at": 1_800_000_000i64});
        let q = quota_from_window(Some(&w)).unwrap().unwrap();
        assert_eq!(q.remaining, 93.0);
        assert_eq!(q.reset_at_ms, 1_800_000_000_000);
    }
}
