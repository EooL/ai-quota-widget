mod claude_logs;
mod models;
mod providers;
mod snapshot;

use models::{MetricsPayload, PlatformMetric};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tauri_plugin_autostart::ManagerExt as AutostartExt;
use tauri_plugin_notification::NotificationExt;

#[tauri::command]
async fn get_platform_metrics(app: AppHandle) -> Result<MetricsPayload, String> {
    let chatgpt_app = app.clone();
    let gemini_app = app.clone();
    let claude_app = app.clone();
    let (chatgpt, gemini, claude) = tokio::join!(
        providers::chatgpt::fetch(&chatgpt_app),
        providers::gemini::fetch(&gemini_app),
        providers::claude::fetch(&claude_app),
    );
    let payload = MetricsPayload {
        chatgpt,
        gemini,
        claude,
    };
    // Orchestratorul citește cotele reale de aici (best-effort, erorile se ignoră).
    snapshot::write_snapshot(&payload);
    Ok(payload)
}

#[tauri::command]
async fn get_available_models(app: AppHandle) -> providers::ModelCatalog {
    providers::detect_models(&app).await
}

#[tauri::command]
async fn refresh_platform(app: AppHandle, platform: String) -> Result<PlatformMetric, String> {
    let fetch = || async {
        match platform.as_str() {
            "chatgpt" => providers::chatgpt::fetch(&app).await,
            "gemini" => providers::gemini::fetch(&app).await,
            "claude" => providers::claude::fetch(&app).await,
            other => PlatformMetric::unavailable(other, other, "Unknown platform"),
        }
    };
    if !matches!(platform.as_str(), "chatgpt" | "gemini" | "claude") {
        return Err(format!("Unknown platform: {platform}"));
    }
    let first = fetch().await;
    if first.status == models::PlatformStatus::Unknown {
        tokio::time::sleep(std::time::Duration::from_millis(350)).await;
        Ok(fetch().await)
    } else {
        Ok(first)
    }
}

/// Consum real Claude Code (tokeni pe zi și model) din jurnalele locale; citirea rulează pe un fir separat.
#[tauri::command]
async fn claude_usage_totals() -> Result<Vec<claude_logs::UsageRow>, String> {
    let rows = tauri::async_runtime::spawn_blocking(claude_logs::totals)
        .await
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

#[tauri::command]
fn push_notification(app: AppHandle, title: String, body: String) -> Result<(), String> {
    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_autostart(app: AppHandle, enable: bool) -> Result<bool, String> {
    let mgr = app.autolaunch();
    if enable {
        mgr.enable().map_err(|e| e.to_string())?;
    } else {
        mgr.disable().map_err(|e| e.to_string())?;
    }
    mgr.is_enabled().map_err(|e| e.to_string())
}

/// Mută fereastra într-un colț al monitorului curent: "tl" | "tr" | "bl" | "br".
#[tauri::command]
fn snap_window(app: AppHandle, corner: String) -> Result<(), String> {
    let win = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    let monitor = win
        .current_monitor()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "unknown monitor".to_string())?;
    let size = win.outer_size().map_err(|e| e.to_string())?;
    let scale = monitor.scale_factor();
    let margin = (16.0 * scale) as i32;
    // sus lăsăm loc pentru bara de meniu (macOS), jos pentru bara de activități / Dock
    let top_margin = ((if cfg!(target_os = "macos") { 40.0 } else { 16.0 }) * scale) as i32;
    let bottom_margin = ((if cfg!(target_os = "macos") { 90.0 } else { 56.0 }) * scale) as i32;
    let mon_pos = monitor.position();
    let mon_size = monitor.size();
    let right = mon_pos.x + mon_size.width as i32 - size.width as i32 - margin;
    let left = mon_pos.x + margin;
    let top = mon_pos.y + top_margin;
    let bottom = mon_pos.y + mon_size.height as i32 - size.height as i32 - bottom_margin;
    let (x, y) = match corner.as_str() {
        "tl" => (left, top),
        "tr" => (right, top),
        "bl" => (left, bottom),
        "br" => (right, bottom),
        other => return Err(format!("unknown corner: {other}")),
    };
    win.set_position(tauri::PhysicalPosition::new(x, y))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_window_position(app: AppHandle, x: i32, y: i32) -> Result<(), String> {
    let win = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    win.set_position(tauri::PhysicalPosition::new(x, y))
        .map_err(|e| e.to_string())
}

fn is_hotkey(shortcut: &Shortcut, key: Code) -> bool {
    shortcut.matches(Modifiers::ALT | Modifiers::SHIFT, key)
}

#[tauri::command]
fn get_autostart(app: AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, "hide", "Hide", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "Refresh", true, None::<&str>)?;
    let chatgpt = CheckMenuItem::with_id(
        app,
        "plat-chatgpt",
        "ChatGPT Plus",
        true,
        true,
        None::<&str>,
    )?;
    let gemini = CheckMenuItem::with_id(
        app,
        "plat-gemini",
        "Gemini Advanced",
        true,
        true,
        None::<&str>,
    )?;
    let claude =
        CheckMenuItem::with_id(app, "plat-claude", "Claude Pro", true, true, None::<&str>)?;
    let platforms = Submenu::with_id_and_items(
        app,
        "platforms",
        "Platforme",
        true,
        &[&chatgpt, &gemini, &claude],
    )?;
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[&show, &hide, &refresh, &sep, &platforms, &settings, &quit],
    )?;

    let mut builder = TrayIconBuilder::with_id("aqw-tray")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
            "hide" => {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.hide();
                }
            }
            "refresh" => {
                let _ = app.emit("tray:refresh", ());
            }
            "plat-chatgpt" => {
                let _ = app.emit("tray:toggle-platform", "chatgpt");
            }
            "plat-gemini" => {
                let _ = app.emit("tray:toggle-platform", "gemini");
            }
            "plat-claude" => {
                let _ = app.emit("tray:toggle-platform", "claude");
            }
            "settings" => {
                let _ = app.emit("tray:open-settings", ());
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(win) = app.get_webview_window("main") {
                    if win.is_visible().unwrap_or(false) {
                        let _ = win.hide();
                    } else {
                        let _ = win.show();
                        let _ = win.set_focus();
                    }
                }
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    if is_hotkey(shortcut, Code::KeyQ) {
                        if let Some(win) = app.get_webview_window("main") {
                            if win.is_visible().unwrap_or(true) {
                                let _ = win.hide();
                            } else {
                                let _ = win.show();
                                let _ = win.set_focus();
                            }
                        }
                    } else if is_hotkey(shortcut, Code::KeyW) {
                        let _ = app.emit("shortcut:cycle", ());
                    } else if is_hotkey(shortcut, Code::KeyE) {
                        let _ = app.emit("shortcut:corner", ());
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            get_platform_metrics,
            get_available_models,
            refresh_platform,
            push_notification,
            set_autostart,
            get_autostart,
            snap_window,
            claude_usage_totals,
            set_window_position
        ])
        .setup(|app| {
            // Widget de tip „menu bar" pe macOS: fără icoană în Dock și fără Cmd+Tab.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            setup_tray(app.handle())?;
            // Alt+Shift+Q arată/ascunde, +W schimbă mărimea, +E schimbă colțul. Dacă o combinație e
            // deja luată de altă aplicație, o ignorăm (widget-ul rămâne utilizabil din tray).
            for key in [Code::KeyQ, Code::KeyW, Code::KeyE] {
                let _ = app
                    .global_shortcut()
                    .register(Shortcut::new(Some(Modifiers::ALT | Modifiers::SHIFT), key));
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running AI Quota Widget");
}
