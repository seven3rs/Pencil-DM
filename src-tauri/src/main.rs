#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use pdm_engine::{
    resolve_engine, system_engine_is_compatible, DownloadInfo, EngineManifest, EngineProcess,
    EngineSource, LaunchOptions, RpcClient,
};
use serde_json::json;
mod store;
use rusqlite::Connection;
use std::path::PathBuf;
use tauri::{Emitter, Manager};
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
            if let Ok(db) = store::open(&data_root().join("pencil.db")) { *state.db.lock().await = Some(db); }
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
    if let Some(db) = state.db.lock().await.as_ref() { let _ = store::save_snapshot(db, &gid, &url, "", "", "Other", "waiting", 0, 0, None); }
    Ok(gid)
}

#[tauri::command]
async fn list_downloads(state: tauri::State<'_, AppState>) -> Result<Vec<DownloadInfo>, String> {
    let rpc = state.rpc().await?;
    let items = rpc.list().await.map_err(|e| e.to_string())?;
    if let Some(db) = state.db.lock().await.as_ref() { for item in &items { let _ = store::save_snapshot(db, &item.gid, "", &item.name, &item.path, "Other", &item.status, item.total, item.completed, item.error.as_deref()); } }
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
async fn list_history(state: tauri::State<'_, AppState>) -> Result<Vec<store::HistoryItem>, String> {
    let db = state.db.lock().await;
    let conn = db.as_ref().ok_or_else(|| "History database is not ready.".to_string())?;
    store::history(conn)
}

#[tauri::command]
async fn set_setting(state: tauri::State<'_, AppState>, key: String, value: String) -> Result<(), String> {
    let db = state.db.lock().await;
    store::set_setting(db.as_ref().ok_or_else(|| "Settings database is not ready.".to_string())?, &key, &value)
}

#[tauri::command]
async fn get_settings(state: tauri::State<'_, AppState>) -> Result<Vec<(String, String)>, String> {
    let db = state.db.lock().await;
    store::settings(db.as_ref().ok_or_else(|| "Settings database is not ready.".to_string())?)
}

#[tauri::command]
async fn list_categories(state: tauri::State<'_, AppState>) -> Result<Vec<(String, String, String, String)>, String> {
    let db = state.db.lock().await;
    let conn = db.as_ref().ok_or_else(|| "Categories database is not ready.".to_string())?;
    let mut stmt = conn.prepare("SELECT id,name,extensions,default_dir FROM categories ORDER BY name").map_err(|e| e.to_string())?;
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).map_err(|e| e.to_string())?.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

#[tauri::command]
async fn update_category(state: tauri::State<'_, AppState>, id: String, name: String, extensions: String, default_dir: String) -> Result<(), String> {
    if name.trim().is_empty() || name.len() > 80 || extensions.len() > 2048 || default_dir.trim().is_empty() || default_dir.len() > 1024 { return Err("Invalid category fields.".into()); }
    let db = state.db.lock().await;
    let conn = db.as_ref().ok_or_else(|| "Categories database is not ready.".to_string())?;
    conn.execute("UPDATE categories SET name=?1,extensions=?2,default_dir=?3 WHERE id=?4", rusqlite::params![name.trim(), extensions.to_ascii_lowercase(), default_dir.trim(), id]).map_err(|e| e.to_string())?; Ok(())
}

fn main() {
    if std::env::args().any(|a| a == "--install-engine") {
        std::process::exit(install_engine_cli());
    }

    tauri::Builder::default()
        .setup(|app| {
            let db = store::open(&data_root().join("pencil.db")).ok();
            app.manage(AppState { db: Mutex::new(db), ..AppState::default() });
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move { start_engine(handle).await });
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
            list_history, set_setting, get_settings, list_categories, update_category
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
