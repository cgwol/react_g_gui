// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{Emitter, Manager};

static LAST_TOGGLE: AtomicU64 = AtomicU64::new(0);

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn system_action(action: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let mut cmd = match action.as_str() {
            "sleep" => {
                let mut c = std::process::Command::new("rundll32.exe");
                c.args(["powrprof.dll,SetSuspendState", "0,1,0"]);
                c
            }
            "shutdown" => {
                let mut c = std::process::Command::new("shutdown");
                c.args(["/s", "/t", "0"]);
                c
            }
            _ => return Err("unknown action".into()),
        };
        cmd.spawn().map_err(|e| e.to_string())?;
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        let script = match action.as_str() {
            "sleep" => "tell application \"System Events\" to sleep",
            "shutdown" => "tell application \"System Events\" to shut down",
            _ => return Err("unknown action".into()),
        };
        std::process::Command::new("osascript")
            .arg("-e")
            .arg(script)
            .spawn()
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = action;
        Err("unsupported platform".into())
    }
}

#[tauri::command]
fn launch_path(path: String) -> Result<(), String> {
    let file = std::path::Path::new(&path);
    let is_bat = file
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("bat"))
        .unwrap_or(false);
    let mut cmd = if is_bat {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", &path]);
        c
    } else {
        std::process::Command::new(file)
    };
    if let Some(dir) = file.parent() {
        cmd.current_dir(dir);
    }
    cmd.spawn().map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(serde::Serialize)]
struct SteamGame {
    app_id: String,
    name: String,
}

fn steam_root() -> Option<std::path::PathBuf> {
    let output = std::process::Command::new("reg")
        .args(["query", r"HKCU\Software\Valve\Steam", "/v", "SteamPath"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let path = text.lines().find_map(|line| {
        let line = line.trim();
        let rest = line.strip_prefix("SteamPath")?.trim();
        let rest = rest.strip_prefix("REG_SZ")?.trim();
        Some(std::path::PathBuf::from(rest))
    })?;
    if path.join("steamapps").exists() {
        Some(path)
    } else {
        None
    }
}

fn field(body: &str, key: &str) -> String {
    let needle = format!("\"{key}\"");
    for line in body.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(&needle) {
            return rest.trim().trim_matches('"').to_string();
        }
    }
    String::new()
}

#[tauri::command]
fn list_steam_games() -> Result<Vec<SteamGame>, String> {
    let steam = steam_root().ok_or("Steam install not found")?;
    let vdf = steam.join("steamapps").join("libraryfolders.vdf");
    let text = std::fs::read_to_string(&vdf).map_err(|e| e.to_string())?;

    let mut libs = vec![steam.join("steamapps")];
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("\"path\"") {
            let path = rest.trim().trim_matches('"').replace("\\\\", "\\");
            let lib = std::path::PathBuf::from(path).join("steamapps");
            if !libs.contains(&lib) {
                libs.push(lib);
            }
        }
    }

    let mut games = Vec::new();
    for lib in libs {
        let Ok(entries) = std::fs::read_dir(&lib) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with("appmanifest_") || !name.ends_with(".acf") {
                continue;
            }
            let Ok(body) = std::fs::read_to_string(entry.path()) else {
                continue;
            };
            let app_id = field(&body, "appid");
            let title = field(&body, "name");
            if !app_id.is_empty() && !title.is_empty() {
                games.push(SteamGame {
                    app_id,
                    name: title,
                });
            }
        }
    }
    games.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(games)
}

#[tauri::command]
fn launch_steam(app_id: String) -> Result<(), String> {
    let steam = steam_root().ok_or("Steam install not found")?;
    std::process::Command::new(steam.join("steam.exe"))
        .arg("-applaunch")
        .arg(&app_id)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn tiles_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("tiles");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn clear_tile(dir: &std::path::Path, id: &str) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let prefix = format!("{id}.");
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with(&prefix) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[tauri::command]
fn save_tile_image(app: tauri::AppHandle, id: String, path: String) -> Result<String, String> {
    let dir = tiles_dir(&app)?;
    clear_tile(&dir, &id);
    let src = std::path::PathBuf::from(&path);
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_lowercase();
    let dest = dir.join(format!("{id}.{ext}"));
    std::fs::copy(&src, &dest).map_err(|e| e.to_string())?;
    Ok(dest.to_string_lossy().to_string())
}

#[tauri::command]
fn save_data_url(app: tauri::AppHandle, id: String, data_url: String) -> Result<String, String> {
    let (mime, b64) = data_url
        .strip_prefix("data:")
        .and_then(|rest| rest.split_once(";base64,"))
        .ok_or("bad data url")?;
    let ext = match mime {
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => "png",
    };
    let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64)
        .map_err(|e| e.to_string())?;
    let dir = tiles_dir(&app)?;
    clear_tile(&dir, &id);
    let dest = dir.join(format!("{id}.{ext}"));
    std::fs::write(&dest, bytes).map_err(|e| e.to_string())?;
    Ok(dest.to_string_lossy().to_string())
}

#[tauri::command]
fn delete_tile_image(path: String) -> Result<(), String> {
    if path.is_empty() || path.starts_with("data:") || path.starts_with("http") {
        return Ok(());
    }
    let file = std::path::Path::new(&path);
    if file.exists() {
        std::fs::remove_file(file).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn clear_tiles(app: tauri::AppHandle) -> Result<(), String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("tiles");
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn place_overlay(app: &tauri::AppHandle, win: &tauri::WebviewWindow) {
    let cursor = app.cursor_position().ok();
    let monitors = app.available_monitors().unwrap_or_default();
    let monitor = monitors.iter().find(|monitor| {
        let Some(cursor) = cursor else {
            return false;
        };
        let pos = monitor.position();
        let size = monitor.size();
        let x = cursor.x as i32;
        let y = cursor.y as i32;
        x >= pos.x
            && y >= pos.y
            && x < pos.x + size.width as i32
            && y < pos.y + size.height as i32
    });
    let monitor = monitor.or_else(|| monitors.first());
    if let Some(monitor) = monitor {
        let pos = monitor.position();
        let size = monitor.size();
        let _ = win.set_position(tauri::PhysicalPosition::new(pos.x, pos.y));
        let _ = win.set_size(tauri::PhysicalSize::new(size.width, size.height));
    }
}

fn set_overlay(app: &tauri::AppHandle, show: bool) {
    let Some(win) = app.get_webview_window("overlay") else {
        return;
    };
    if show {
        place_overlay(app, &win);
        let _ = win.set_always_on_top(true);
        let _ = win.show();
        let _ = win.set_focus();
        let _ = win.emit("overlay-open", ());
    } else {
        let _ = win.hide();
        let _ = win.emit("overlay-close", ());
    }
}

fn toggle_overlay(app: &tauri::AppHandle) {
    let now = now_ms();
    let prev = LAST_TOGGLE.load(Ordering::Relaxed);
    if now.saturating_sub(prev) < 350 {
        return;
    }
    LAST_TOGGLE.store(now, Ordering::Relaxed);

    let app = app.clone();
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let visible = handle
            .get_webview_window("overlay")
            .and_then(|win| win.is_visible().ok())
            .unwrap_or(false);
        set_overlay(&handle, !visible);
    });
}

fn create_overlay(app: &tauri::AppHandle) -> Result<(), String> {
    if app.get_webview_window("overlay").is_some() {
        return Ok(());
    }
    tauri::WebviewWindowBuilder::new(app, "overlay", tauri::WebviewUrl::App("index.html".into()))
        .title("Menu")
        .transparent(true)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .focused(false)
        .shadow(false)
        .resizable(false)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn guide_down(gilrs: &gilrs::Gilrs) -> bool {
    use gilrs::Button;
    gilrs.gamepads().any(|(_, pad)| {
        if !pad.is_connected() {
            return false;
        }
        let mode = pad.button_code(Button::Mode).is_some() && pad.is_pressed(Button::Mode);
        let chord = pad.is_pressed(Button::Select) && pad.is_pressed(Button::Start);
        mode || chord
    })
}

fn spawn_guide_watcher(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let mut gilrs = match gilrs::Gilrs::new() {
            Ok(gilrs) => gilrs,
            Err(_) => return,
        };
        let mut prev = false;
        loop {
            while gilrs.next_event().is_some() {}
            let down = guide_down(&gilrs);
            if down && !prev {
                toggle_overlay(&app);
            }
            prev = down;
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
    });
}

#[tauri::command]
fn hide_overlay(app: tauri::AppHandle) {
    set_overlay(&app, false);
}

#[tauri::command]
fn open_shell(app: tauri::AppHandle, route: String) {
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.unminimize();
        let _ = main.show();
        let _ = main.set_focus();
        let _ = main.emit("shell-navigate", route);
    }
    set_overlay(&app, false);
}

#[tauri::command]
fn minimize_shell(app: tauri::AppHandle) {
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.minimize();
    }
    set_overlay(&app, false);
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            if let Err(err) = create_overlay(app.handle()) {
                eprintln!("overlay window: {err}");
            }

            #[cfg(desktop)]
            {
                use tauri_plugin_global_shortcut::ShortcutState;
                app.handle().plugin(
                    tauri_plugin_global_shortcut::Builder::new()
                        .with_shortcuts(["ctrl+alt+g"])?
                        .with_handler(|app, _shortcut, event| {
                            if event.state == ShortcutState::Pressed {
                                toggle_overlay(app);
                            }
                        })
                        .build(),
                )?;
                spawn_guide_watcher(app.handle().clone());
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            system_action,
            launch_path,
            list_steam_games,
            launch_steam,
            save_tile_image,
            save_data_url,
            delete_tile_image,
            clear_tiles,
            hide_overlay,
            open_shell,
            minimize_shell,
            quit_app
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
