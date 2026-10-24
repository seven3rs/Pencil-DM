#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use pdm_engine::{
    resolve_engine, system_engine_is_compatible, DownloadInfo, EngineManifest, EngineProcess,
    EngineSource, LaunchOptions, RpcClient,
};
use serde_json::json;
mod browser;
mod scheduler;
mod store;
use chrono::{Datelike, Utc};
use rusqlite::Connection;
use sha2::Digest;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tauri_plugin_notification::NotificationExt;
use tokio::io::AsyncReadExt;
use tokio::sync::{Mutex, RwLock};

const APP_DIR_NAME: &str = "strs.pencil.downloadmanager";
const NOT_READY: &str = "The download engine is still starting. Try again in a moment.";

#[derive(Default)]
struct AppState {
    /// Cheap-to-clone handle that commands use; set once the engine is up.
    rpc: RwLock<Option<RpcClient>>,
    /// Owns the child process so it can be shut down on exit.
    process: Mutex<Option<EngineProcess>>,
    db: Mutex<Option<Connection>>,
    browser: std::sync::Mutex<Option<browser::BrowserState>>,
    shutdown_cancel: Arc<AtomicBool>,
}

impl AppState {
    async fn rpc(&self) -> Result<RpcClient, String> {
        self.rpc
            .read()
            .await
            .clone()
            .ok_or_else(|| NOT_READY.to_string())
    }
}

fn data_root() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(APP_DIR_NAME)
}

fn engine_root() -> PathBuf {
    data_root().join("engine")
}

fn custom_engine_path() -> Option<PathBuf> {
    std::fs::read_to_string(data_root().join("engine-path.txt"))
        .ok()
        .map(|value| PathBuf::from(value.trim()))
        .filter(|path| !path.as_os_str().is_empty())
}

async fn resolve_app_engine<P>(
    manifest: &EngineManifest,
    progress: P,
) -> pdm_engine::Result<EngineSource>
where
    P: Fn(u64, Option<u64>) + Send + Sync,
{
    if manifest.entry_for_current().is_some() {
        return resolve_engine(&engine_root(), manifest, progress).await;
    }
    let mut dynamic_manifest = manifest.clone();
    dynamic_manifest.refresh_system_minimum().await;
    pdm_engine::resolve_system_engine(&dynamic_manifest, custom_engine_path().as_deref())
}

fn log_line(msg: &str) {
    use std::io::Write;
    let _ = std::fs::create_dir_all(data_root());
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(data_root().join("setup.log"))
    {
        let _ = writeln!(f, "{msg}");
    }
}

/// `pencil-download-manager --install-engine`, run by the installer after copying files.
fn install_engine_cli() -> i32 {
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            log_line(&format!("install-engine: runtime error: {e}"));
            return 1;
        }
    };
    let result = rt.block_on(async {
        let manifest = EngineManifest::embedded()?;
        resolve_app_engine(&manifest, |_, _| {}).await
    });
    match result {
        Ok(src) => {
            log_line(&format!("install-engine: ok ({})", src.path().display()));
            0
        }
        Err(e) => {
            log_line(&format!("install-engine: failed: {e}"));
            1
        }
    }
}

fn missing_engine_instructions() -> (String, String) {
    #[cfg(target_os = "macos")]
    {
        return (
            "Install the download engine with Homebrew".into(),
            "brew install aria2".into(),
        );
    }
    #[cfg(target_os = "linux")]
    {
        let os_release = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
        let fedora_like = os_release.lines().any(|line| {
            (line.starts_with("ID=")
                && (line.contains("fedora") || line.contains("rhel") || line.contains("centos")))
                || (line.starts_with("ID_LIKE=")
                    && (line.contains("fedora") || line.contains("rhel")))
        });
        if fedora_like {
            return (
                "Install the download engine with your system package manager".into(),
                "sudo dnf install aria2".into(),
            );
        }
        return (
            "Install the download engine with your system package manager".into(),
            "sudo apt install aria2".into(),
        );
    }
    #[allow(unreachable_code)]
    (
        "The download engine is unavailable on this platform".into(),
        String::new(),
    )
}

async fn start_engine(app: tauri::AppHandle) {
    let emit_status = |state: &str, detail: &str| {
        let _ = app.emit("engine-status", json!({ "state": state, "detail": detail }));
    };
    emit_status("starting", "");

    let manifest = match EngineManifest::embedded() {
        Ok(m) => m,
        Err(e) => return emit_status("error", &e.to_string()),
    };

    // Normally a no-op: the installer already set the engine up and this just
    // re-checks the binary's hash. If it is missing or modified it is repaired.
    let source = resolve_app_engine(&manifest, |done, total| {
        let _ = app.emit("engine-progress", json!({ "done": done, "total": total }));
    })
    .await;
    let bin = match source {
        Ok(EngineSource::Managed(p)) | Ok(EngineSource::System(p)) => p,
        Err(pdm_engine::EngineError::NotFound) => {
            let (detail, command) = missing_engine_instructions();
            let _ = app.emit(
                "engine-status",
                json!({
                    "state": "missing",
                    "detail": detail,
                    "command": command
                }),
            );
            return;
        }
        Err(e) => return emit_status("error", &e.to_string()),
    };

    let opts = LaunchOptions {
        bin,
        download_dir: dirs::download_dir().unwrap_or_else(|| data_root().join("downloads")),
        session_file: data_root().join("session.dat"),
    };
    match EngineProcess::spawn(opts).await {
        Ok(process) => {
            let state = app.state::<AppState>();
            let rpc = process.rpc.clone();
            if let Ok(db) = store::open(&data_root().join("pencil.db")) {
                *state.db.lock().await = Some(db);
            }
            *state.process.lock().await = Some(process);
            *state.rpc.write().await = Some(rpc);
            emit_status("ready", "");
        }
        Err(e) => emit_status("error", &e.to_string()),
    }
}

#[tauri::command]
async fn set_engine_path(path: String) -> Result<(), String> {
    let mut manifest = EngineManifest::embedded().map_err(|e| e.to_string())?;
    if manifest.entry_for_current().is_some() {
        return Err("Custom system engine paths are available on macOS and Linux.".into());
    }
    manifest.refresh_system_minimum().await;
    let candidate = PathBuf::from(path.trim());
    if !candidate.is_absolute() {
        return Err("Choose an absolute path to the download engine executable.".into());
    }
    if !system_engine_is_compatible(&candidate, &manifest) {
        return Err(
            "The selected executable is missing, invalid, or below the minimum supported version."
                .into(),
        );
    }
    std::fs::create_dir_all(data_root()).map_err(|e| e.to_string())?;
    std::fs::write(
        data_root().join("engine-path.txt"),
        candidate.to_string_lossy().as_bytes(),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn recheck_engine(app: tauri::AppHandle) -> Result<(), String> {
    start_engine(app).await;
    Ok(())
}

#[tauri::command]
async fn add_download(state: tauri::State<'_, AppState>, url: String) -> Result<String, String> {
    let rpc = state.rpc().await?;
    let gid = rpc.add_uri(&url).await.map_err(|e| e.to_string())?;
    if let Some(db) = state.db.lock().await.as_ref() {
        let _ = store::save_snapshot(
            db,
            store::DownloadSnapshot {
                gid: &gid,
                url: &url,
                file_name: "",
                save_dir: "",
                category: "Other",
                status: "waiting",
                total: 0,
                completed: 0,
                error: None,
            },
        );
    }
    Ok(gid)
}

#[rustfmt::skip]
async fn execute_post_action(app: &tauri::AppHandle, state: &AppState, gid: &str, action: &str, path: &str) {
    match action {
        "none" | "" => {},
        "open-folder" => {
            let downloaded_path = PathBuf::from(path);
            if let Some(parent) = downloaded_path.parent() {
                #[cfg(target_os = "windows")] let (program, args): (&str, Vec<String>) = ("explorer", vec![parent.to_string_lossy().into_owned()]);
                #[cfg(target_os = "macos")] let (program, args): (&str, Vec<String>) = ("open", vec![parent.to_string_lossy().into_owned()]);
                #[cfg(target_os = "linux")] let (program, args): (&str, Vec<String>) = ("xdg-open", vec![parent.to_string_lossy().into_owned()]);
                let _ = std::process::Command::new(program).args(args).spawn();
            }
        },
        "sound" => {
            #[cfg(target_os = "windows")] { let _ = std::process::Command::new("powershell").args(["-NoProfile", "-Command", "[console]::beep(880,400)"]).spawn(); }
            #[cfg(target_os = "macos")] { let _ = std::process::Command::new("afplay").arg("/System/Library/Sounds/Glass.aiff").spawn(); }
            #[cfg(target_os = "linux")] { let _ = std::process::Command::new("canberra-gtk-play").args(["-i", "complete"]).spawn(); }
        },
        "notification" => { let _ = app.notification().builder().title("Download complete").body("A queued download has finished.").show(); },
        "run-program" => {
            let program = {
                let db = state.db.lock().await;
                db.as_ref().and_then(|conn| store::settings(conn).ok()).and_then(|items| items.into_iter().find(|(k, _)| k == &format!("post_program:{gid}")).map(|(_, v)| v))
            };
            if let Some(program) = program {
                if let Ok(canonical) = PathBuf::from(program).canonicalize() {
                    let allowed = {
                        let db = state.db.lock().await;
                        db.as_ref().and_then(|conn| store::settings(conn).ok()).and_then(|items| items.into_iter().find(|(k, _)| k == "allowed_programs").map(|(_, v)| v)).and_then(|v| serde_json::from_str::<Vec<String>>(&v).ok()).unwrap_or_default()
                    };
                    if allowed.contains(&canonical.to_string_lossy().into_owned()) { let _ = std::process::Command::new(canonical).spawn(); }
                }
            }
        },
        "sleep" | "hibernate" | "shutdown" => {
            state.shutdown_cancel.store(false, Ordering::SeqCst);
            let cancel = state.shutdown_cancel.clone();
            let action = action.to_string();
            let handle = app.clone();
            tauri::async_runtime::spawn(async move {
                for remaining in (1..=60).rev() {
                    if cancel.load(Ordering::SeqCst) { let _ = handle.emit("power-countdown-cancelled", ()); return; }
                    let _ = handle.emit("power-countdown", json!({"action": action, "seconds": remaining}));
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
                if cancel.load(Ordering::SeqCst) { return; }
                #[cfg(target_os = "windows")] let (program, args): (&str, &[&str]) = match action.as_str() { "shutdown" => ("shutdown", &["/s", "/t", "0"]), "hibernate" => ("shutdown", &["/h"]), _ => ("rundll32", &["powrprof.dll,SetSuspendState", "0,1,0"]) };
                #[cfg(target_os = "macos")] let (program, args): (&str, &[&str]) = match action.as_str() { "shutdown" => ("/sbin/shutdown", &["-h", "now"]), "hibernate" => ("pmset", &["sleepnow"]), _ => ("pmset", &["sleepnow"]) };
                #[cfg(target_os = "linux")] let (program, args): (&str, &[&str]) = match action.as_str() { "shutdown" => ("systemctl", &["poweroff"]), "hibernate" => ("systemctl", &["hibernate"]), _ => ("systemctl", &["suspend"]) };
                let _ = std::process::Command::new(program).args(args).spawn();
            });
        },
        _ => { let _ = gid; }
    }
}

#[rustfmt::skip]
#[tauri::command]
async fn list_downloads(app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<Vec<DownloadInfo>, String> {
    let rpc = state.rpc().await?;
    let items = rpc.list().await.map_err(|e| e.to_string())?;
    let actions_to_run = {
        let db = state.db.lock().await;
        if let Some(conn) = db.as_ref() {
            for item in &items {
                let _ = store::save_snapshot(conn, store::DownloadSnapshot {
                    gid: &item.gid, url: "", file_name: &item.name, save_dir: &item.path,
                    category: "Other", status: &item.status, total: item.total,
                    completed: item.completed, error: item.error.as_deref(),
                });
            }
            let settings = store::settings(conn).unwrap_or_default();
            let mut actions = Vec::new();
            for item in &items {
                if item.status != "complete" { continue; }
                let action_key = format!("post_action:{}", item.gid);
                let done_key = format!("post_action_done:{}", item.gid);
                let action = settings.iter().find(|(k, _)| k == &action_key).map(|(_, v)| v.clone()).unwrap_or_else(|| "none".into());
                let done = settings.iter().any(|(k, v)| k == &done_key && v == "true");
                if !done && action != "none" {
                    let _ = store::set_setting(conn, &done_key, "true");
                    actions.push((item.gid.clone(), item.path.clone(), action));
                }
            }
            actions
        } else { Vec::new() }
    };
    for (gid, path, action) in actions_to_run {
        execute_post_action(&app, &state, &gid, &action, &path).await;
    }
    Ok(items)
}

#[tauri::command]
async fn pause_download(state: tauri::State<'_, AppState>, gid: String) -> Result<(), String> {
    let rpc = state.rpc().await?;
    rpc.pause(&gid).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn resume_download(state: tauri::State<'_, AppState>, gid: String) -> Result<(), String> {
    let rpc = state.rpc().await?;
    rpc.resume(&gid).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn remove_download(state: tauri::State<'_, AppState>, gid: String) -> Result<(), String> {
    let rpc = state.rpc().await?;
    rpc.remove(&gid).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_history(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<store::HistoryItem>, String> {
    let db = state.db.lock().await;
    let conn = db
        .as_ref()
        .ok_or_else(|| "History database is not ready.".to_string())?;
    store::history(conn)
}

#[tauri::command]
async fn set_setting(
    state: tauri::State<'_, AppState>,
    key: String,
    value: String,
) -> Result<(), String> {
    let db = state.db.lock().await;
    store::set_setting(
        db.as_ref()
            .ok_or_else(|| "Settings database is not ready.".to_string())?,
        &key,
        &value,
    )
}

#[tauri::command]
async fn get_settings(state: tauri::State<'_, AppState>) -> Result<Vec<(String, String)>, String> {
    let db = state.db.lock().await;
    store::settings(
        db.as_ref()
            .ok_or_else(|| "Settings database is not ready.".to_string())?,
    )
}

#[tauri::command]
async fn list_categories(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<(String, String, String, String)>, String> {
    let db = state.db.lock().await;
    let conn = db
        .as_ref()
        .ok_or_else(|| "Categories database is not ready.".to_string())?;
    let mut stmt = conn
        .prepare("SELECT id,name,extensions,default_dir FROM categories ORDER BY name")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn update_category(
    state: tauri::State<'_, AppState>,
    id: String,
    name: String,
    extensions: String,
    default_dir: String,
) -> Result<(), String> {
    if name.trim().is_empty()
        || name.len() > 80
        || extensions.len() > 2048
        || default_dir.trim().is_empty()
        || default_dir.len() > 1024
    {
        return Err("Invalid category fields.".into());
    }
    let db = state.db.lock().await;
    let conn = db
        .as_ref()
        .ok_or_else(|| "Categories database is not ready.".to_string())?;
    conn.execute(
        "UPDATE categories SET name=?1,extensions=?2,default_dir=?3 WHERE id=?4",
        rusqlite::params![
            name.trim(),
            extensions.to_ascii_lowercase(),
            default_dir.trim(),
            id
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[rustfmt::skip]
#[tauri::command]
async fn set_speed_limiter(state: tauri::State<'_, AppState>, enabled: bool, bytes_per_second: u64) -> Result<(), String> {
    if enabled && !(1024..=1_073_741_824).contains(&bytes_per_second) { return Err("Speed limit must be between 1 KiB/s and 1 GiB/s.".into()); }
    let rpc = state.rpc().await?;
    rpc.set_global_speed_limit(if enabled { Some(bytes_per_second) } else { None }).await.map_err(|e| e.to_string())?;
    let db = state.db.lock().await;
    let conn = db.as_ref().ok_or_else(|| "Settings database is not ready.".to_string())?;
    store::set_setting(conn, "speed_limiter_enabled", if enabled { "true" } else { "false" })?;
    store::set_setting(conn, "speed_limiter_bytes_per_second", &bytes_per_second.to_string())
}

#[rustfmt::skip]
#[tauri::command]
async fn set_download_speed_limit(state: tauri::State<'_, AppState>, gid: String, bytes_per_second: Option<u64>) -> Result<(), String> {
    if bytes_per_second.is_some_and(|v| !(1024..=1_073_741_824).contains(&v)) { return Err("Speed limit must be between 1 KiB/s and 1 GiB/s.".into()); }
    state.rpc().await?.set_download_speed_limit(&gid, bytes_per_second).await.map_err(|e| e.to_string())
}

#[rustfmt::skip]
#[tauri::command]
async fn save_schedule_rules(state: tauri::State<'_, AppState>, rules: Vec<scheduler::ScheduleRule>) -> Result<(), String> {
    if rules.len() > 100 { return Err("At most 100 schedule rules are allowed.".into()); }
    for rule in &rules {
        if rule.name.trim().is_empty() || rule.name.len() > 80 || rule.days.is_empty() || rule.days.iter().any(|d| !(1..=7).contains(d)) || rule.start_time == rule.stop_time || chrono::NaiveTime::parse_from_str(&rule.start_time, "%H:%M").is_err() || chrono::NaiveTime::parse_from_str(&rule.stop_time, "%H:%M").is_err() || rule.timezone.parse::<chrono_tz::Tz>().is_err() || rule.speed_limit.is_some_and(|v| !(1024..=1_073_741_824).contains(&v)) {
            return Err("A schedule rule contains invalid days, time, time zone, or speed limit.".into());
        }
    }
    let json = serde_json::to_string(&rules).map_err(|e| e.to_string())?;
    let db = state.db.lock().await;
    store::set_setting(db.as_ref().ok_or_else(|| "Settings database is not ready.".to_string())?, "schedule_rules", &json)
}

#[rustfmt::skip]
#[tauri::command]
async fn get_schedule_rules(state: tauri::State<'_, AppState>) -> Result<Vec<scheduler::ScheduleRule>, String> {
    let db = state.db.lock().await;
    let conn = db.as_ref().ok_or_else(|| "Settings database is not ready.".to_string())?;
    let value = store::settings(conn)?.into_iter().find(|(k, _)| k == "schedule_rules").map(|(_, v)| v).unwrap_or_else(|| "[]".into());
    serde_json::from_str(&value).map_err(|e| e.to_string())
}

#[rustfmt::skip]
#[tauri::command]
async fn set_download_checksum(state: tauri::State<'_, AppState>, gid: String, expected: String) -> Result<(), String> {
    let value = expected.trim().to_ascii_lowercase();
    if !value.is_empty() && !((value.len() == 32 || value.len() == 40 || value.len() == 64) && value.bytes().all(|b| b.is_ascii_hexdigit())) {
        return Err("Checksum must be a 32-character MD5, 40-character SHA-1, or 64-character SHA-256 hex digest.".into());
    }
    let db = state.db.lock().await;
    store::set_setting(db.as_ref().ok_or_else(|| "Settings database is not ready.".to_string())?, &format!("checksum:{gid}"), &value)
}

#[rustfmt::skip]
async fn hash_file<D: Digest + Default>(path: &str) -> Result<String, String> {
    let mut file = tokio::fs::File::open(path).await.map_err(|error| error.to_string())?;
    let mut hasher = D::default();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).await.map_err(|error| error.to_string())?;
        if count == 0 { break; }
        hasher.update(&buffer[..count]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(""))
}

#[rustfmt::skip]
#[tauri::command]
async fn verify_checksum(path: String, expected: String) -> Result<bool, String> {
    let expected = expected.trim().to_ascii_lowercase();
    let algorithm = match expected.len() {
        64 => "sha256",
        40 => "sha1",
        32 => "md5",
        _ => return Err("Paste a valid SHA-256, SHA-1, or MD5 checksum.".into()),
    };
    if !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Checksum must contain hexadecimal characters only.".into());
    }
    let actual = match algorithm {
        "sha256" => hash_file::<sha2::Sha256>(&path).await?,
        "sha1" => hash_file::<sha1::Sha1>(&path).await?,
        _ => hash_file::<md5::Md5>(&path).await?,
    };
    Ok(actual == expected)
}

#[rustfmt::skip]
#[tauri::command]
async fn set_post_download_action(state: tauri::State<'_, AppState>, gid: String, action: String) -> Result<(), String> {
    const ACTIONS: &[&str] = &["none", "open-folder", "sound", "notification", "run-program", "sleep", "hibernate", "shutdown"];
    if !ACTIONS.contains(&action.as_str()) { return Err("Unsupported post-download action.".into()); }
    let db = state.db.lock().await;
    store::set_setting(db.as_ref().ok_or_else(|| "Settings database is not ready.".to_string())?, &format!("post_action:{gid}"), &action)
}

#[rustfmt::skip]
#[tauri::command]
async fn set_post_download_program(state: tauri::State<'_, AppState>, gid: String, path: String, confirmed: bool) -> Result<(), String> {
    if !confirmed { return Err("Explicit confirmation is required before allowing a program.".into()); }
    let candidate = PathBuf::from(path.trim());
    if !candidate.is_absolute() || !candidate.is_file() { return Err("Choose an existing absolute program path.".into()); }
    let canonical = candidate.canonicalize().map_err(|e| e.to_string())?.to_string_lossy().into_owned();
    let db = state.db.lock().await;
    let conn = db.as_ref().ok_or_else(|| "Settings database is not ready.".to_string())?;
    let current = store::settings(conn)?.into_iter().find(|(k, _)| k == "allowed_programs").map(|(_, v)| v).unwrap_or_else(|| "[]".into());
    let mut paths: Vec<String> = serde_json::from_str(&current).unwrap_or_default();
    if !paths.contains(&canonical) { paths.push(canonical.clone()); }
    store::set_setting(conn, "allowed_programs", &serde_json::to_string(&paths).map_err(|e| e.to_string())?)?;
    store::set_setting(conn, &format!("post_program:{gid}"), &canonical)?;
    store::set_setting(conn, &format!("post_action:{gid}"), "run-program")
}

#[rustfmt::skip]
#[tauri::command]
async fn start_power_action(app: tauri::AppHandle, state: tauri::State<'_, AppState>, action: String) -> Result<(), String> {
    if !["sleep", "hibernate", "shutdown"].contains(&action.as_str()) { return Err("Unsupported power action.".into()); }
    state.shutdown_cancel.store(false, Ordering::SeqCst);
    let cancel = state.shutdown_cancel.clone();
    app.emit("power-countdown", json!({"action": action, "seconds": 60})).map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn(async move {
        for remaining in (1..=60).rev() {
            if cancel.load(Ordering::SeqCst) { let _ = app.emit("power-countdown-cancelled", ()); return; }
            let _ = app.emit("power-countdown", json!({"action": action, "seconds": remaining}));
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
        if cancel.load(Ordering::SeqCst) { return; }
        #[cfg(target_os = "windows")]
        let (program, args): (&str, &[&str]) = match action.as_str() { "shutdown" => ("shutdown", &["/s", "/t", "0"]), "hibernate" => ("shutdown", &["/h"]), _ => ("rundll32", &["powrprof.dll,SetSuspendState", "0,1,0"]) };
        #[cfg(target_os = "macos")]
        let (program, args): (&str, &[&str]) = match action.as_str() { "shutdown" => ("/sbin/shutdown", &["-h", "now"]), "hibernate" => ("pmset", &["sleepnow"]), _ => ("pmset", &["sleepnow"]) };
        #[cfg(target_os = "linux")]
        let (program, args): (&str, &[&str]) = match action.as_str() { "shutdown" => ("systemctl", &["poweroff"]), "hibernate" => ("systemctl", &["hibernate"]), _ => ("systemctl", &["suspend"]) };
        let _ = std::process::Command::new(program).args(args).spawn();
    });
    Ok(())
}

#[rustfmt::skip]
#[tauri::command]
fn cancel_power_action(state: tauri::State<'_, AppState>) { state.shutdown_cancel.store(true, Ordering::SeqCst); }

#[rustfmt::skip]
#[tauri::command]
async fn allow_program(state: tauri::State<'_, AppState>, path: String, confirmed: bool) -> Result<(), String> {
    if !confirmed { return Err("Explicit confirmation is required before allowing a program.".into()); }
    let candidate = PathBuf::from(path.trim());
    if !candidate.is_absolute() || !candidate.is_file() { return Err("Choose an existing absolute program path.".into()); }
    let canonical = candidate.canonicalize().map_err(|e| e.to_string())?;
    let db = state.db.lock().await;
    let conn = db.as_ref().ok_or_else(|| "Settings database is not ready.".to_string())?;
    let current = store::settings(conn)?.into_iter().find(|(k, _)| k == "allowed_programs").map(|(_, v)| v).unwrap_or_else(|| "[]".into());
    let mut paths: Vec<String> = serde_json::from_str(&current).unwrap_or_default();
    let value = canonical.to_string_lossy().into_owned();
    if !paths.contains(&value) { paths.push(value); }
    store::set_setting(conn, "allowed_programs", &serde_json::to_string(&paths).map_err(|e| e.to_string())?)
}

#[rustfmt::skip]
#[tauri::command]
async fn run_allowed_program(state: tauri::State<'_, AppState>, path: String, args: Vec<String>) -> Result<(), String> {
    if args.len() > 64 || args.iter().any(|a| a.len() > 2048 || a.contains('\0')) { return Err("Invalid program arguments.".into()); }
    let candidate = PathBuf::from(path.trim()).canonicalize().map_err(|e| e.to_string())?;
    let db = state.db.lock().await;
    let conn = db.as_ref().ok_or_else(|| "Settings database is not ready.".to_string())?;
    let current = store::settings(conn)?.into_iter().find(|(k, _)| k == "allowed_programs").map(|(_, v)| v).unwrap_or_else(|| "[]".into());
    let paths: Vec<String> = serde_json::from_str(&current).unwrap_or_default();
    if !paths.contains(&candidate.to_string_lossy().into_owned()) { return Err("This program has not been approved. Approve it explicitly first.".into()); }
    std::process::Command::new(candidate).args(args).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn create_browser_pairing_code(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let browser = state
        .browser
        .lock()
        .map_err(|_| "Browser integration lock failed".to_string())?;
    browser
        .as_ref()
        .ok_or_else(|| {
            "Browser integration is unavailable. Restart the app and try again.".to_string()
        })?
        .create_pairing_code()
}

#[rustfmt::skip]
fn schedule_rule_active(rule: &scheduler::ScheduleRule, now: chrono::DateTime<Utc>) -> bool {
    if !rule.enabled { return false; }
    let Ok(tz) = rule.timezone.parse::<chrono_tz::Tz>() else { return false; };
    let local = now.with_timezone(&tz);
    let today = local.weekday().number_from_monday();
    let yesterday = if today == 1 { 7 } else { today - 1 };
    let (Ok(start), Ok(stop)) = (rule.start_time.parse::<chrono::NaiveTime>(), rule.stop_time.parse::<chrono::NaiveTime>()) else { return false; };
    let time = local.time();
    if start <= stop { rule.days.contains(&today) && time >= start && time < stop }
    else { (rule.days.contains(&today) && time >= start) || (rule.days.contains(&yesterday) && time < stop) }
}

#[rustfmt::skip]
async fn scheduler_loop(app: tauri::AppHandle) {
    let mut was_active: Option<bool> = None;
    let mut last_limit: Option<Option<u64>> = None;
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let state = app.state::<AppState>();
        let rules = {
            let db = state.db.lock().await;
            db.as_ref()
                .and_then(|conn| store::settings(conn).ok())
                .and_then(|items| {
                    items.into_iter().find(|(key, _)| key == "schedule_rules").map(|(_, value)| value)
                })
                .and_then(|value| serde_json::from_str::<Vec<scheduler::ScheduleRule>>(&value).ok())
                .unwrap_or_default()
        };
        // The next transition is calculated by the same pure function covered by tests.
        if let Some(event) = scheduler::next_event(Utc::now(), &rules) {
            let _next_event = (event.at, event.rule_id, event.kind, event.speed_limit);
        }
        let now = Utc::now();
        let has_rules = rules.iter().any(|rule| rule.enabled);
        let active = rules.iter().any(|rule| schedule_rule_active(rule, now));

        if has_rules && was_active != Some(active) {
            if let Ok(rpc) = state.rpc().await {
                if let Ok(items) = rpc.list().await {
                    for item in items {
                        if active && item.status == "paused" {
                            let _ = rpc.resume(&item.gid).await;
                        } else if !active && (item.status == "active" || item.status == "waiting") {
                            let _ = rpc.pause(&item.gid).await;
                        }
                    }
                }
                let _ = app.emit("scheduler-state", json!({"active": active}));
                was_active = Some(active);
            }
        } else if !has_rules {
            was_active = None;
        }

        let base_limit = {
            let db = state.db.lock().await;
            db.as_ref()
                .and_then(|conn| store::settings(conn).ok())
                .and_then(|items| {
                    let enabled = items.iter().any(|(key, value)| key == "speed_limiter_enabled" && value == "true");
                    let value = items.iter()
                        .find(|(key, _)| key == "speed_limiter_bytes_per_second")
                        .and_then(|(_, value)| value.parse::<u64>().ok());
                    if enabled { value } else { None }
                })
        };
        let schedule_limit = if has_rules {
            rules.iter()
                .filter(|rule| schedule_rule_active(rule, now))
                .filter_map(|rule| rule.speed_limit)
                .min()
        } else {
            None
        };
        let effective = match (base_limit, schedule_limit) {
            (Some(base), Some(scheduled)) => Some(base.min(scheduled)),
            (Some(base), None) => Some(base),
            (None, Some(scheduled)) => Some(scheduled),
            (None, None) => None,
        };
        if last_limit != Some(effective) {
            if let Ok(rpc) = state.rpc().await {
                if rpc.set_global_speed_limit(effective).await.is_ok() {
                    last_limit = Some(effective);
                }
            }
        }
    }
}

fn main() {
    if std::env::args().any(|a| a == "--install-engine") {
        std::process::exit(install_engine_cli());
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let db = store::open(&data_root().join("pencil.db")).ok();
            app.manage(AppState {
                db: Mutex::new(db),
                browser: std::sync::Mutex::new(None),
                shutdown_cancel: Arc::new(AtomicBool::new(false)),
                ..AppState::default()
            });
            let browser = browser::start(app.handle().clone(), &data_root())
                .map_err(std::io::Error::other)?;
            *app.state::<AppState>()
                .browser
                .lock()
                .map_err(|_| std::io::Error::other("Browser integration lock failed"))? =
                Some(browser);
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move { start_engine(handle.clone()).await });
            let scheduler_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move { scheduler_loop(scheduler_handle).await });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            add_download,
            list_downloads,
            pause_download,
            resume_download,
            remove_download,
            recheck_engine,
            set_engine_path,
            list_history,
            set_setting,
            get_settings,
            list_categories,
            update_category,
            create_browser_pairing_code,
            set_speed_limiter,
            set_download_speed_limit,
            save_schedule_rules,
            get_schedule_rules,
            verify_checksum,
            set_download_checksum,
            set_post_download_action,
            start_power_action,
            cancel_power_action,
            allow_program,
            run_allowed_program,
            set_post_download_program
        ])
        .build(tauri::generate_context!())
        .expect("error while building Pencil Download Manager")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                let state = app.state::<AppState>();
                tauri::async_runtime::block_on(async {
                    if let Some(process) = state.process.lock().await.take() {
                        process.shutdown().await;
                    }
                });
            }
        });
}
