//! Download-engine management for Pencil Download Manager:
//! verified install (Scoop-style manifest), launch, and a small RPC client.

pub mod error;
pub mod install;
pub mod launcher;
pub mod manifest;
pub mod rpc;

use rand::{distributions::Alphanumeric, Rng};
use std::path::{Path, PathBuf};
use std::process::Command;

pub use error::{EngineError, Result};
pub use launcher::{EngineProcess, LaunchOptions};
pub use manifest::{parse_version, version_at_least, EngineManifest};
pub use rpc::{DownloadInfo, DownloadOptions, RpcClient};

pub(crate) fn random_token(len: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(len)
        .map(char::from)
        .collect()
}

/// Where the engine binary comes from on this platform.
#[derive(Debug, Clone)]
pub enum EngineSource {
    /// Installed and hash-verified under the app data directory.
    Managed(PathBuf),
    /// Found on PATH or a common system location (platforms without a bundled
    /// manifest entry, e.g. macOS and Linux).
    System(PathBuf),
}

impl EngineSource {
    pub fn path(&self) -> &Path {
        match self {
            EngineSource::Managed(p) | EngineSource::System(p) => p,
        }
    }
}

/// Make sure an engine is available: install/repair the verified copy when the
/// manifest has an entry for this platform, otherwise look on the PATH.
pub async fn resolve_engine<P>(
    root: &Path,
    manifest: &EngineManifest,
    progress: P,
) -> Result<EngineSource>
where
    P: Fn(u64, Option<u64>) + Send + Sync,
{
    if let Some(entry) = manifest.entry_for_current() {
        let bin = install::ensure_installed(root, manifest, entry, progress).await?;
        return Ok(EngineSource::Managed(bin));
    }
    resolve_system_engine(manifest, None)
}

pub fn resolve_system_engine(
    manifest: &EngineManifest,
    custom_path: Option<&Path>,
) -> Result<EngineSource> {
    if let Some(path) = custom_path {
        if system_engine_is_compatible(path, manifest) {
            return Ok(EngineSource::System(path.to_path_buf()));
        }
    }
    find_system_engine(manifest)
        .map(EngineSource::System)
        .ok_or(EngineError::NotFound)
}

pub fn system_engine_is_compatible(path: &Path, manifest: &EngineManifest) -> bool {
    if !path.is_file() {
        return false;
    }
    let Some(minimum) = parse_version(&manifest.min_system_version) else {
        return false;
    };
    let output = match Command::new(path).arg("--version").output() {
        Ok(output) if output.status.success() => output,
        _ => return false,
    };
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    parse_version(&text).is_some_and(|found| version_at_least(&found, &minimum))
}

fn find_system_engine(manifest: &EngineManifest) -> Option<PathBuf> {
    parse_version(&manifest.min_system_version)?;
    system_engine_candidates()
        .into_iter()
        .find(|candidate| system_engine_is_compatible(candidate, manifest))
}

fn system_engine_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(path) = which::which("aria2c") {
        candidates.push(path);
    }
    if let Some(path) = std::env::var_os("PATH") {
        candidates.extend(std::env::split_paths(&path).map(|dir| dir.join("aria2c")));
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        candidates.push(home.join(".local/bin/aria2c"));
    }
    candidates.extend([
        PathBuf::from("/opt/homebrew/bin/aria2c"),
        PathBuf::from("/usr/local/bin/aria2c"),
        PathBuf::from("/usr/bin/aria2c"),
    ]);
    let mut unique = std::collections::HashSet::new();
    candidates.retain(|path| unique.insert(path.clone()));
    candidates
}

#[cfg(test)]
mod system_detection_tests {
    use super::*;

    #[test]
    fn candidates_include_path_and_common_locations() {
        let candidates = system_engine_candidates();
        assert!(candidates.iter().any(|p| p.ends_with("aria2c")));
        #[cfg(target_os = "macos")]
        assert!(candidates.contains(&PathBuf::from("/opt/homebrew/bin/aria2c")));
        #[cfg(target_os = "linux")]
        assert!(candidates.contains(&PathBuf::from("/usr/bin/aria2c")));
    }
}
