//! Download-engine management for Pencil Download Manager:
//! verified install (Scoop-style manifest), launch, and a small RPC client.

pub mod error;
pub mod install;
pub mod launcher;
pub mod manifest;
pub mod rpc;

use rand::{distributions::Alphanumeric, Rng};
use std::path::{Path, PathBuf};

pub use error::{EngineError, Result};
pub use launcher::{EngineProcess, LaunchOptions};
pub use manifest::EngineManifest;
pub use rpc::{DownloadInfo, RpcClient};

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
    /// Found on the system PATH (platforms without a manifest entry, e.g. Linux
    /// where the .deb/.rpm package depends on it).
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
    which::which("aria2c")
        .map(EngineSource::System)
        .map_err(|_| EngineError::NotFound)
}
