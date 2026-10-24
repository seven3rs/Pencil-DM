//! Scoop-style verified install: download -> check SHA-256 -> safe extract ->
//! check the binary's SHA-256 -> atomic rename into place.

use crate::error::{EngineError, Result};
use crate::manifest::{EngineManifest, PlatformEntry};
use crate::random_token;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::AsyncWriteExt;

const DEFAULT_MAX_DOWNLOAD: u64 = 64 * 1024 * 1024;
const MAX_EXTRACTED_BYTES: u64 = 256 * 1024 * 1024;

pub fn engine_dir(root: &Path, version: &str) -> PathBuf {
    root.join("aria2").join(version)
}

pub fn binary_path(root: &Path, manifest: &EngineManifest, entry: &PlatformEntry) -> PathBuf {
    engine_dir(root, &manifest.version).join(&entry.bin)
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut f = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// True when the installed binary exists and matches the hash embedded in the app.
pub fn is_installed(root: &Path, manifest: &EngineManifest, entry: &PlatformEntry) -> bool {
    match sha256_file(&binary_path(root, manifest, entry)) {
        Ok(h) => h.eq_ignore_ascii_case(&entry.bin_sha256),
        Err(_) => false,
    }
}

/// Install (or repair) the engine for this platform. Returns the binary path.
pub async fn ensure_installed<P>(
    root: &Path,
    manifest: &EngineManifest,
    entry: &PlatformEntry,
    progress: P,
) -> Result<PathBuf>
where
    P: Fn(u64, Option<u64>) + Send + Sync,
{
    if is_installed(root, manifest, entry) {
        return Ok(binary_path(root, manifest, entry));
    }
    tokio::fs::create_dir_all(root).await?;
    let dl_dir = root.join(format!(".download-{}", random_token(10)));
    tokio::fs::create_dir_all(&dl_dir).await?;
    let archive = dl_dir.join("engine.zip");

    let result = async {
        download_verified(
            &entry.url,
            &entry.sha256,
            entry.max_bytes.unwrap_or(DEFAULT_MAX_DOWNLOAD),
            &archive,
            &progress,
        )
        .await?;
        let (root, manifest, entry, archive) = (
            root.to_path_buf(),
            manifest.clone(),
            entry.clone(),
            archive.clone(),
        );
        tokio::task::spawn_blocking(move || {
            install_from_archive(&archive, &root, &manifest, &entry)
        })
        .await
        .map_err(|e| EngineError::Io(io::Error::other(e)))?
    }
    .await;

    let _ = tokio::fs::remove_dir_all(&dl_dir).await;
    result
}

/// Stream a file to disk while hashing it; delete it if the hash does not match.
pub async fn download_verified<P>(
    url: &str,
    expected_sha256: &str,
    max_bytes: u64,
    dest: &Path,
    progress: &P,
) -> Result<()>
where
    P: Fn(u64, Option<u64>) + Send + Sync,
{
    if !url.starts_with("https://") {
        return Err(EngineError::InsecureUrl(url.to_string()));
    }
    let client = reqwest::Client::builder()
        .user_agent("pencil-download-manager")
        .connect_timeout(Duration::from_secs(20))
        .read_timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 {
                attempt.error("too many redirects")
            } else if attempt.url().scheme() != "https" {
                attempt.error("redirect to a non-https url")
            } else {
                attempt.follow()
            }
        }))
        .build()?;

    let resp = client.get(url).send().await?.error_for_status()?;
    let total = resp.content_length();
    if let Some(t) = total {
        if t > max_bytes {
            return Err(EngineError::TooLarge(max_bytes));
        }
    }

    let mut hasher = Sha256::new();
    let mut file = tokio::fs::File::create(dest).await?;
    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        downloaded += chunk.len() as u64;
        if downloaded > max_bytes {
            drop(file);
            let _ = tokio::fs::remove_file(dest).await;
            return Err(EngineError::TooLarge(max_bytes));
        }
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
        progress(downloaded, total);
    }
    file.flush().await?;
    drop(file);

    let actual = hex::encode(hasher.finalize());
    if !actual.eq_ignore_ascii_case(expected_sha256) {
        let _ = tokio::fs::remove_file(dest).await;
        return Err(EngineError::HashMismatch {
            what: "download".into(),
            expected: expected_sha256.to_string(),
            actual,
        });
    }
    Ok(())
}

/// Verify an archive on disk, extract it safely, verify the binary and move it
/// into `root/aria2/<version>`. Synchronous so it can be unit tested offline.
pub fn install_from_archive(
    archive_path: &Path,
    root: &Path,
    manifest: &EngineManifest,
    entry: &PlatformEntry,
) -> Result<PathBuf> {
    let actual = sha256_file(archive_path)?;
    if !actual.eq_ignore_ascii_case(&entry.sha256) {
        return Err(EngineError::HashMismatch {
            what: "archive".into(),
            expected: entry.sha256.clone(),
            actual,
        });
    }
    fs::create_dir_all(root)?;
    let staging = root.join(format!(".staging-{}", random_token(10)));
    fs::create_dir_all(&staging)?;
    let result = extract_and_swap(archive_path, root, &staging, manifest, entry);
    let _ = fs::remove_dir_all(&staging);
    result
}

fn extract_and_swap(
    archive_path: &Path,
    root: &Path,
    staging: &Path,
    manifest: &EngineManifest,
    entry: &PlatformEntry,
) -> Result<PathBuf> {
    let out = staging.join("out");
    fs::create_dir_all(&out)?;

    let mut zip = zip::ZipArchive::new(fs::File::open(archive_path)?)?;
    let prefix = entry.extract_dir.as_deref().map(Path::new);
    let mut total: u64 = 0;

    for i in 0..zip.len() {
        let mut file = zip.by_index(i)?;
        // `enclosed_name` rejects absolute paths and `..` components (zip-slip).
        let rel = match file.enclosed_name() {
            Some(p) => p,
            None => return Err(EngineError::UnsafeArchive(file.name().to_string())),
        };
        if let Some(mode) = file.unix_mode() {
            if mode & 0o170000 == 0o120000 {
                return Err(EngineError::UnsafeArchive(format!(
                    "symlink {}",
                    file.name()
                )));
            }
        }
        let rel = match prefix {
            Some(p) => match rel.strip_prefix(p) {
                Ok(r) => r.to_path_buf(),
                Err(_) => continue,
            },
            None => rel,
        };
        if rel.as_os_str().is_empty() {
            continue;
        }
        let dest = out.join(&rel);
        if file.is_dir() {
            fs::create_dir_all(&dest)?;
            continue;
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        let remaining = MAX_EXTRACTED_BYTES.saturating_sub(total);
        let mut limited = (&mut file).take(remaining + 1);
        let mut out_file = fs::File::create(&dest)?;
        total += io::copy(&mut limited, &mut out_file)?;
        if total > MAX_EXTRACTED_BYTES {
            return Err(EngineError::TooLarge(MAX_EXTRACTED_BYTES));
        }
    }

    let staged_bin = out.join(&entry.bin);
    let actual = sha256_file(&staged_bin)
        .map_err(|_| EngineError::Manifest(format!("{} is missing from the archive", entry.bin)))?;
    if !actual.eq_ignore_ascii_case(&entry.bin_sha256) {
        return Err(EngineError::HashMismatch {
            what: "engine binary".into(),
            expected: entry.bin_sha256.clone(),
            actual,
        });
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&staged_bin, fs::Permissions::from_mode(0o755))?;
    }

    let final_dir = engine_dir(root, &manifest.version);
    if final_dir.exists() {
        fs::remove_dir_all(&final_dir)?;
    }
    if let Some(parent) = final_dir.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(&out, &final_dir)?;
    Ok(final_dir.join(&entry.bin))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn sha(bytes: &[u8]) -> String {
        hex::encode(Sha256::digest(bytes))
    }

    /// Build a zip in `dir` and a manifest/entry that describe it.
    fn fixture(
        dir: &Path,
        entries: &[(&str, &[u8])],
        bin_in_zip: &str,
        bin_bytes: &[u8],
    ) -> (PathBuf, EngineManifest, PlatformEntry) {
        let zip_path = dir.join("engine.zip");
        let mut w = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
        let opts = zip::write::SimpleFileOptions::default();
        for (name, data) in entries {
            w.start_file(*name, opts).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap();

        let entry = PlatformEntry {
            url: "https://example.invalid/engine.zip".into(),
            sha256: sha256_file(&zip_path).unwrap(),
            extract_dir: Some("pkg".into()),
            bin: bin_in_zip.into(),
            bin_sha256: sha(bin_bytes),
            max_bytes: None,
        };
        let manifest = EngineManifest {
            version: "9.9.9".into(),
            min_system_version: "1.36.0".into(),
            platforms: [("test".to_string(), entry.clone())].into_iter().collect(),
        };
        (zip_path, manifest, entry)
    }

    #[test]
    fn installs_and_verifies_a_good_archive() {
        let tmp = tempfile::tempdir().unwrap();
        let (zip, manifest, entry) = fixture(
            tmp.path(),
            &[
                ("pkg/engine.bin", b"engine-bytes"),
                ("pkg/COPYING", b"license"),
                ("other/ignored.txt", b"x"),
            ],
            "engine.bin",
            b"engine-bytes",
        );
        let root = tmp.path().join("root");
        let bin = install_from_archive(&zip, &root, &manifest, &entry).unwrap();
        assert!(bin.ends_with("aria2/9.9.9/engine.bin"));
        assert!(is_installed(&root, &manifest, &entry));
        assert!(root.join("aria2/9.9.9/COPYING").exists());
        assert!(!root.join("aria2/9.9.9/ignored.txt").exists());
        // staging directories are cleaned up
        let leftovers: Vec<_> = fs::read_dir(&root)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with('.'))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn rejects_archive_with_wrong_hash() {
        let tmp = tempfile::tempdir().unwrap();
        let (zip, manifest, mut entry) = fixture(
            tmp.path(),
            &[("pkg/engine.bin", b"engine-bytes")],
            "engine.bin",
            b"engine-bytes",
        );
        entry.sha256 = "0".repeat(64);
        let root = tmp.path().join("root");
        let err = install_from_archive(&zip, &root, &manifest, &entry).unwrap_err();
        assert!(matches!(err, EngineError::HashMismatch { .. }));
        assert!(!engine_dir(&root, "9.9.9").exists());
    }

    #[test]
    fn rejects_tampered_binary_even_if_archive_hash_matches() {
        let tmp = tempfile::tempdir().unwrap();
        let (zip, manifest, mut entry) = fixture(
            tmp.path(),
            &[("pkg/engine.bin", b"evil-bytes")],
            "engine.bin",
            b"engine-bytes",
        );
        entry.bin_sha256 = sha(b"engine-bytes");
        let root = tmp.path().join("root");
        let err = install_from_archive(&zip, &root, &manifest, &entry).unwrap_err();
        assert!(
            matches!(err, EngineError::HashMismatch { ref what, .. } if what == "engine binary")
        );
        assert!(!engine_dir(&root, "9.9.9").exists());
    }

    #[test]
    fn rejects_zip_slip_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let (zip, manifest, entry) = fixture(
            tmp.path(),
            &[
                ("pkg/engine.bin", b"engine-bytes"),
                ("../../evil.txt", b"pwned"),
            ],
            "engine.bin",
            b"engine-bytes",
        );
        let root = tmp.path().join("root");
        let err = install_from_archive(&zip, &root, &manifest, &entry).unwrap_err();
        assert!(matches!(err, EngineError::UnsafeArchive(_)));
        assert!(!tmp.path().join("evil.txt").exists());
        assert!(!engine_dir(&root, "9.9.9").exists());
    }

    #[test]
    fn detects_a_modified_installed_binary() {
        let tmp = tempfile::tempdir().unwrap();
        let (zip, manifest, entry) = fixture(
            tmp.path(),
            &[("pkg/engine.bin", b"engine-bytes")],
            "engine.bin",
            b"engine-bytes",
        );
        let root = tmp.path().join("root");
        let bin = install_from_archive(&zip, &root, &manifest, &entry).unwrap();
        assert!(is_installed(&root, &manifest, &entry));
        fs::write(&bin, b"tampered").unwrap();
        assert!(!is_installed(&root, &manifest, &entry));
    }

    #[tokio::test]
    async fn refuses_plain_http_downloads() {
        let tmp = tempfile::tempdir().unwrap();
        let err = download_verified(
            "http://example.invalid/x.zip",
            &"0".repeat(64),
            10,
            &tmp.path().join("x"),
            &|_, _| {},
        )
        .await
        .unwrap_err();
        assert!(matches!(err, EngineError::InsecureUrl(_)));
    }
}

#[cfg(test)]
mod network_tests {
    use super::*;

    /// Downloads the real archive from the official release and installs it.
    /// Run with: cargo test -p pdm-engine -- --ignored
    #[tokio::test]
    #[ignore = "needs network access"]
    async fn real_windows_archive_downloads_verifies_and_installs() {
        let manifest = EngineManifest::embedded().unwrap();
        let entry = manifest.platforms.get("windows-x86_64").unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let bin = ensure_installed(tmp.path(), &manifest, entry, |_, _| {})
            .await
            .unwrap();
        assert!(bin.exists());
        assert!(is_installed(tmp.path(), &manifest, entry));
        // second call is a no-op because the hash already matches
        let again = ensure_installed(tmp.path(), &manifest, entry, |_, _| {})
            .await
            .unwrap();
        assert_eq!(bin, again);
    }
}
