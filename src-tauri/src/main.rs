#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use pdm_engine::{
    resolve_engine, DownloadInfo, EngineManifest, EngineProcess, EngineSource, LaunchOptions,
    system_engine_is_compatible, RpcClient,
};
use serde_json::json;
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
                && (line.contains("fedora")
                    || line.contains("rhel")
                    || line.contains("centos")))
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
    ("The download engine is unavailable on this platform".into(), String::new())
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
            let _ = app.emit("engine-status", json!({
                "state": "missing",
                "detail": detail,
                "command": command
            }));
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
    rpc.add_uri(&url).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_downloads(state: tauri::State<'_, AppState>) -> Result<Vec<DownloadInfo>, String> {
    let rpc = state.rpc().await?;
    rpc.list().await.map_err(|e| e.to_string())
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

fn main() {
    if std::env::args().any(|a| a == "--install-engine") {
        std::process::exit(install_engine_cli());
    }

    tauri::Builder::default()
        .manage(AppState::default())
        .setup(|app| {
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
            set_engine_path
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
