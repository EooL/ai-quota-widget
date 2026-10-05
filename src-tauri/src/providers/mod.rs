pub mod chatgpt;
pub mod claude;
pub mod claude_usage;
pub mod gemini;

use serde_json::Value;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::AppHandle;
use tauri_plugin_shell::ShellExt;

#[derive(Debug, Clone, Serialize)]
pub struct AvailableModel {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelCatalog {
    pub chatgpt: Vec<AvailableModel>,
    pub gemini: Vec<AvailableModel>,
    pub claude: Vec<AvailableModel>,
}

pub async fn detect_models(app: &AppHandle) -> ModelCatalog {
    let (codex, agy) = tokio::join!(
        run_cli(app, "codex", &["debug", "models"]),
        run_cli(app, "agy", &["models"]),
    );
    ModelCatalog {
        chatgpt: codex.map(|raw| parse_codex_models(&raw)).unwrap_or_default(),
        gemini: agy.map(|raw| parse_agy_models(&raw)).unwrap_or_default(),
        // Claude Code has no public catalog command: fixed list + the configured default model.
        claude: claude::models(),
    }
}

fn parse_codex_models(raw: &str) -> Vec<AvailableModel> {
    let Ok(json) = serde_json::from_str::<Value>(raw) else { return Vec::new() };
    let Some(models) = json.get("models").and_then(Value::as_array) else { return Vec::new() };
    let mut found = Vec::new();
    for model in models {
        if model.get("visibility").and_then(Value::as_str) != Some("list") { continue; }
        let Some(id) = model.get("slug").and_then(Value::as_str) else { continue };
        let name = model.get("display_name").and_then(Value::as_str).unwrap_or(id);
        found.push(AvailableModel { id: id.to_string(), name: name.to_string() });
    }
    found
}

fn parse_agy_models(raw: &str) -> Vec<AvailableModel> {
    let mut found = Vec::new();
    for line in raw.lines() {
        let Some((id, name)) = line.trim().split_once('\t') else { continue };
        // The app tracks the Gemini quota account. agy can also list Claude and other
        // providers, whose quotas belong to different accounts and must not be conflated.
        if id.starts_with("gemini-") {
            found.push(AvailableModel { id: id.to_string(), name: name.to_string() });
        }
    }
    found
}

pub async fn run_cli(app: &AppHandle, program: &str, args: &[&str]) -> Result<String, String> {
    augment_process_path();
    let mut last_err = format!("{program} was not found in PATH");
    for candidate in command_candidates(program) {
        match app.shell().command(&candidate).args(args).output().await {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                if output.status.success() {
                    let text = stdout.trim().to_string();
                    if text.is_empty() {
                        return Err(if stderr.trim().is_empty() {
                            format!("{candidate} returned no output")
                        } else {
                            stderr.trim().to_string()
                        });
                    }
                    return Ok(text);
                }
                last_err = if !stderr.trim().is_empty() {
                    stderr.trim().to_string()
                } else if !stdout.trim().is_empty() {
                    stdout.trim().to_string()
                } else {
                    format!("{candidate} exited with an error")
                };
            }
            Err(err) => last_err = err.to_string(),
        }
    }
    Err(last_err)
}

fn command_candidates(program: &str) -> Vec<String> {
    let mut out = vec![program.to_string()];
    if cfg!(windows) {
        if !program.ends_with(".cmd") && !program.ends_with(".exe") {
            out.push(format!("{program}.cmd"));
            out.push(format!("{program}.exe"));
        }
    }
    out
}

/// Aplicațiile GUI de pe macOS (pornite din Finder) nu moștenesc PATH-ul din terminal, deci
/// `claude`/`codex`/`agy` instalate cu npm/Homebrew nu s-ar găsi. Completăm PATH o singură dată.
fn augment_process_path() {
    if cfg!(windows) {
        augment_windows_path();
        return;
    }
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let existing: Vec<PathBuf> = std::env::var_os("PATH")
            .map(|value| std::env::split_paths(&value).collect())
            .unwrap_or_default();
        let mut extra: Vec<PathBuf> = Vec::new();
        if let Some(shell_path) = login_shell_path() {
            extra.extend(std::env::split_paths(&shell_path));
        }
        extra.extend(known_unix_dirs());
        let merged = merge_paths(existing, extra);
        if let Ok(path) = std::env::join_paths(merged) {
            std::env::set_var("PATH", path);
        }
    });
}

/// Păstrează ordinea, elimină duplicatele și directoarele inexistente din completări.
fn merge_paths(existing: Vec<PathBuf>, extra: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut out = existing;
    for dir in extra {
        if dir.is_dir() && !out.contains(&dir) {
            out.push(dir);
        }
    }
    out
}

fn known_unix_dirs() -> Vec<PathBuf> {
    let mut dirs_out: Vec<PathBuf> = ["/opt/homebrew/bin", "/opt/homebrew/sbin", "/usr/local/bin"]
        .iter()
        .map(PathBuf::from)
        .collect();
    if let Some(home) = dirs::home_dir() {
        for rel in [
            ".local/bin",
            ".npm-global/bin",
            ".bun/bin",
            ".cargo/bin",
            ".volta/bin",
            ".deno/bin",
            ".claude/local",
            "Library/pnpm",
            ".local/share/pnpm",
        ] {
            dirs_out.push(home.join(rel));
        }
        // nvm: ~/.nvm/versions/node/<versiune>/bin
        if let Ok(entries) = std::fs::read_dir(home.join(".nvm").join("versions").join("node")) {
            for entry in entries.flatten() {
                dirs_out.push(entry.path().join("bin"));
            }
        }
    }
    dirs_out
}

/// PATH-ul unui shell de login interactiv (include ce setează `.zshrc`/nvm). Max 3 secunde.
fn login_shell_path() -> Option<String> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let mut child = Command::new(shell)
        .args(["-ilc", "printf '__AQW_PATH__%s__AQW_END__' \"$PATH\""])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if started.elapsed() > Duration::from_secs(3) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => return None,
        }
    }
    let mut out = String::new();
    child.stdout.take()?.read_to_string(&mut out).ok()?;
    extract_marked_path(&out)
}

fn extract_marked_path(output: &str) -> Option<String> {
    const START: &str = "__AQW_PATH__";
    const END: &str = "__AQW_END__";
    let from = output.find(START)? + START.len();
    let to = output[from..].find(END)? + from;
    let value = output[from..to].trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn augment_windows_path() {

    let mut paths: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect())
        .unwrap_or_default();

    let mut add = |path: PathBuf| {
        if path.is_dir() && !paths.iter().any(|existing| existing == &path) {
            paths.push(path);
        }
    };

    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        let local = PathBuf::from(local_app_data);
        add(local.join("agy").join("bin"));
        add(local
            .join("Programs")
            .join("OpenAI")
            .join("Codex")
            .join("bin"));
        add(local.join("Microsoft").join("WindowsApps"));
    }
    if let Ok(app_data) = std::env::var("APPDATA") {
        add(PathBuf::from(app_data).join("npm"));
    }
    if let Ok(user_profile) = std::env::var("USERPROFILE") {
        let user = PathBuf::from(user_profile);
        add(user.join(".cargo").join("bin"));
        add(user.join("AppData").join("Roaming").join("npm"));
    }

    if let Ok(path) = std::env::join_paths(paths) {
        std::env::set_var("PATH", path);
    }
}

pub fn read_json_file(path: &Path) -> Result<Value, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&raw).map_err(|e| format!("Invalid JSON in {}: {e}", path.display()))
}

pub fn first_existing(paths: &[PathBuf]) -> Option<PathBuf> {
    paths.iter().find(|p| p.exists()).cloned()
}

pub fn home_config_paths(segments: &[&str]) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(home) = dirs::home_dir() {
        let mut p = home.join(".config");
        for s in segments {
            p = p.join(s);
        }
        paths.push(p);

        let mut p = home;
        for s in segments {
            p = p.join(format!(".{s}"));
        }
        // also ~/.openai/auth.json style via joined hidden first segment
        paths.push(
            dirs::home_dir()
                .unwrap_or_default()
                .join(format!(".{}", segments.join("/"))),
        );
    }
    if let Some(config) = dirs::config_dir() {
        let mut p = config;
        for s in segments {
            p = p.join(s);
        }
        paths.push(p);
    }
    paths
}

pub fn extract_string(value: &Value, keys: &[&str]) -> Option<String> {
    if let Some(obj) = value.as_object() {
        for key in keys {
            if let Some(s) = obj.get(*key).and_then(|v| v.as_str()) {
                if !s.is_empty() {
                    return Some(s.to_string());
                }
            }
        }
        for child in obj.values() {
            if let Some(s) = extract_string(child, keys) {
                return Some(s);
            }
        }
    } else if let Some(arr) = value.as_array() {
        for child in arr {
            if let Some(s) = extract_string(child, keys) {
                return Some(s);
            }
        }
    }
    None
}

pub fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod path_tests {
    use super::*;

    #[test]
    fn extracts_path_between_markers_ignoring_shell_noise() {
        let noisy = "Last login: today\n__AQW_PATH__/usr/bin:/opt/homebrew/bin__AQW_END__\nbye";
        assert_eq!(extract_marked_path(noisy).as_deref(), Some("/usr/bin:/opt/homebrew/bin"));
        assert_eq!(extract_marked_path("no markers"), None);
        assert_eq!(extract_marked_path("__AQW_PATH____AQW_END__"), None);
    }

    #[test]
    fn merge_keeps_order_and_skips_duplicates_and_missing_dirs() {
        let tmp = std::env::temp_dir();
        let existing = vec![tmp.clone()];
        let merged = merge_paths(existing, vec![tmp.clone(), PathBuf::from("/definitely/not/here")]);
        assert_eq!(merged, vec![tmp]);
    }
}
