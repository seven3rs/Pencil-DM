//! Scoop-style verified install: download -> check SHA-256 -> safe extract ->
//! check the binary's SHA-256 -> atomic rename into place.

use crate::error::{EngineError, Result};
use crate::manifest::{EngineManifest, PlatformEntry};
use crate::random_token;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;
use tokio::io::AsyncWriteExt;

const DEFAULT_MAX_DOWNLOAD: u64 = 64 * 1024 * 1024;
const MAX_EXTRACTED_BYTES: u64 = 256 * 1024 * 1024;
const MAX_EXTRACTED_FILE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 4096;

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
    let parsed_url = reqwest::Url::parse(url)
        .map_err(|_| EngineError::InsecureUrl(url.to_string()))?;
    if parsed_url.scheme() != "https"
        || parsed_url.host_str().is_none()
        || !parsed_url.username().is_empty()
        || parsed_url.password().is_some()
    {
        return Err(EngineError::InsecureUrl(url.to_string()));
    }
    let client = reqwest::Client::builder()
        .user_agent("pencil-download-manager")
        .connect_timeout(Duration::from_secs(20))
        .read_timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 {
                return attempt.error("too many redirects");
            }
            let source_host = attempt.previous().first().and_then(|url| url.host_str());
            if redirect_is_allowed(source_host, attempt.url()) {
                attempt.follow()
            } else {
                attempt.error("redirect to an unsafe scheme or untrusted host")
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

fn redirect_is_allowed(source_host: Option<&str>, target: &reqwest::Url) -> bool {
    if target.scheme() != "https"
        || !target.username().is_empty()
        || target.password().is_some()
    {
        return false;
    }
    let target_host = target.host_str();
    let known_github_cdn = matches!(
        target_host,
        Some("release-assets.githubusercontent.com" | "objects.githubusercontent.com")
    );
    target_host == source_host || known_github_cdn
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
    if zip.len() > MAX_ARCHIVE_ENTRIES {
        return Err(EngineError::TooLarge(MAX_ARCHIVE_ENTRIES as u64));
    }
    let prefix = entry.extract_dir.as_deref().map(Path::new);
    let mut total: u64 = 0;
    // Normalize collision keys independently of host OS: archives must be safe
    // when installed on Windows as well as Unix. Track files and directories.
    let mut seen: HashMap<String, bool> = HashMap::new();

    for i in 0..zip.len() {
        let mut file = zip.by_index(i)?;
        let raw_name = file.name().to_string();
        if !archive_name_is_safe(&raw_name) {
            return Err(EngineError::UnsafeArchive(raw_name));
        }
        let rel = match file.enclosed_name() {
            Some(p) => p,
            None => return Err(EngineError::UnsafeArchive(raw_name)),
        };
        if let Some(mode) = file.unix_mode() {
            let kind = mode & 0o170000;
            if kind == 0o120000 || (kind != 0 && kind != 0o100000 && kind != 0o040000) {
                return Err(EngineError::UnsafeArchive(format!("special file {raw_name}")));
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
        let key = collision_key(&rel)?;
        let is_dir = file.is_dir();
        if seen.contains_key(&key)
            || seen
                .keys()
                .any(|existing| existing.starts_with(&(key.clone() + "/")) && !is_dir)
        {
            return Err(EngineError::UnsafeArchive(format!(
                "duplicate/colliding entry {raw_name}"
            )));
        }
        seen.insert(key.clone(), is_dir);
        // Also reject a file used as a parent directory (e.g. `a` and `a/b`).
        let parts: Vec<_> = key.split('/').collect();
        for n in 1..parts.len() {
            let parent = parts[..n].join("/");
            if seen.get(&parent) == Some(&false) {
                return Err(EngineError::UnsafeArchive(format!(
                    "file/directory collision {raw_name}"
                )));
            }
        }
        if !is_dir
            && parts.len() > 1
            && parts[..parts.len() - 1]
                .iter()
                .enumerate()
                .any(|(idx, _)| {
                    let parent = parts[..idx + 1].join("/");
                    seen.get(&parent) == Some(&false)
                })
        {
            return Err(EngineError::UnsafeArchive(format!(
                "file/directory collision {raw_name}"
            )));
        }
        if is_dir {
            fs::create_dir_all(out.join(&rel))?;
            continue;
        }
        if file.size() > MAX_EXTRACTED_FILE_BYTES {
            return Err(EngineError::TooLarge(MAX_EXTRACTED_FILE_BYTES));
        }
        total = total
            .checked_add(file.size())
            .ok_or(EngineError::TooLarge(MAX_EXTRACTED_BYTES))?;
        if total > MAX_EXTRACTED_BYTES {
            return Err(EngineError::TooLarge(MAX_EXTRACTED_BYTES));
        }
        let dest = out.join(&rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        // Never silently overwrite an earlier entry, even if the ZIP library
        // or filesystem treats the names differently.
        let mut out_file = fs::OpenOptions::new().write(true).create_new(true).open(&dest)?;
        let copied = io::copy(&mut file, &mut out_file)?;
        if copied != file.size() {
            return Err(EngineError::UnsafeArchive(format!("size mismatch for {raw_name}")));
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
    if let Some(parent) = final_dir.parent() {
        fs::create_dir_all(parent)?;
    }
    // Keep the previous verified installation recoverable if promotion fails.
    let backup = final_dir.with_file_name(format!(".previous-{}", random_token(10)));
    let had_previous = final_dir.exists();
    if had_previous {
        fs::rename(&final_dir, &backup)?;
    }
    if let Err(error) = fs::rename(&out, &final_dir) {
        if had_previous {
            let _ = fs::rename(&backup, &final_dir);
        }
        return Err(EngineError::Io(error));
    }
    if had_previous {
        let _ = fs::remove_dir_all(&backup);
    }
    Ok(final_dir.join(&entry.bin))
}

/// Reject names that can acquire a different meaning on Windows or Unix.
fn archive_name_is_safe(name: &str) -> bool {
    if name.is_empty()
        || name.starts_with('/')
        || name.starts_with('\\')
        || name.contains('\\')
        || name.contains(':')
        || name.chars().any(char::is_control)
    {
        return false;
    }
    let trimmed = name.strip_suffix('/').unwrap_or(name);
    if trimmed.is_empty() {
        return false;
    }
    trimmed.split('/').all(|part| {
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.ends_with('.')
            || part.ends_with(' ')
        {
            return false;
        }
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        !matches!(
            stem.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "COM1" | "COM2" | "COM3" | "COM4"
                | "COM5" | "COM6" | "COM7" | "COM8" | "COM9" | "LPT1" | "LPT2"
                | "LPT3" | "LPT4" | "LPT5" | "LPT6" | "LPT7" | "LPT8" | "LPT9"
        )
    })
}

fn collision_key(path: &Path) -> Result<String> {
    if path.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(EngineError::UnsafeArchive(path.display().to_string()));
    }
    let key = path.to_string_lossy().replace('\\', "/");
    Ok(key
        .split('/')
        .map(|p| p.trim_end_matches(&['.', ' '][..]).to_lowercase())
        .collect::<Vec<_>>()
        .join("/"))
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
    fn generated_archive_paths_obey_traversal_and_windows_invariants() {
        // Deterministic property-style generation: every component sequence that
        // contains a traversal or Windows separator/device marker must be rejected.
        let atoms = [
            "safe",
            "..",
            ".",
            "CON",
            "NUL.txt",
            "trailing.",
            "trailing ",
            "C:",
            "\\evil",
            "control\u{0001}",
        ];
        for first in atoms {
            for second in atoms {
                let candidate = format!("{first}/{second}/file.bin");
                if [first, second].iter().any(|part| {
                    ["..", ".", "CON", "NUL.txt", "trailing.", "trailing ", "C:", "\\evil"]
                        .contains(part)
                }) {
                    assert!(
                        !archive_name_is_safe(&candidate),
                        "accepted generated unsafe path {candidate:?}"
                    );
                }
            }
        }
        for n in 1..256 {
            let candidate = format!("pkg/{}x", "../".repeat(n));
            assert!(
                !archive_name_is_safe(&candidate),
                "accepted generated traversal {candidate:?}"
            );
        }
    }

    #[test]
    fn rejects_symlink_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("symlink.zip");
        let mut writer = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
        writer
            .start_file("pkg/engine.bin", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"engine-bytes").unwrap();
        let options = zip::write::SimpleFileOptions::default().unix_permissions(0o120777);
        writer.start_file("pkg/link", options).unwrap();
        writer.write_all(b"../../outside").unwrap();
        writer.finish().unwrap();
        let entry = PlatformEntry {
            url: "https://example.invalid/a.zip".into(),
            sha256: sha256_file(&zip_path).unwrap(),
            extract_dir: Some("pkg".into()),
            bin: "engine.bin".into(),
            bin_sha256: sha(b"engine-bytes"),
            max_bytes: None,
        };
        let manifest = EngineManifest {
            version: "9.9.9".into(),
            min_system_version: "1.36.0".into(),
            platforms: [("test".into(), entry.clone())].into_iter().collect(),
        };
        let err = install_from_archive(
            &zip_path,
            &tmp.path().join("root"),
            &manifest,
            &entry,
        )
        .unwrap_err();
        assert!(matches!(err, EngineError::UnsafeArchive(_)));
    }

    #[test]
    fn rejects_case_insensitive_duplicate_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let (zip, manifest, entry) = fixture(
            tmp.path(),
            &[
                ("pkg/engine.bin", b"engine-bytes"),
                ("pkg/ENGINE.BIN", b"engine-bytes"),
            ],
            "engine.bin",
            b"engine-bytes",
        );
        let err = install_from_archive(
            &zip,
            &tmp.path().join("root"),
            &manifest,
            &entry,
        )
        .unwrap_err();
        assert!(matches!(err, EngineError::UnsafeArchive(_)));
    }

    #[test]
    fn rejects_windows_paths_and_reserved_device_names() {
        for name in [
            "pkg/../evil",
            "pkg\\evil",
            "pkg/C:/evil",
            "pkg/CON.txt",
            "pkg/trailing. ",
        ] {
            assert!(
                !archive_name_is_safe(name),
                "accepted unsafe archive path {name:?}"
            );
        }
        assert!(archive_name_is_safe("pkg/aria2c.exe"));
    }

    #[test]
    fn rejects_archive_path_collisions_and_duplicate_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let (zip, manifest, entry) = fixture(
            tmp.path(),
            &[
                ("pkg/engine.bin", b"engine-bytes"),
                ("pkg/readme.txt", b"one"),
                ("pkg/readme.txt", b"two"),
            ],
            "engine.bin",
            b"engine-bytes",
        );
        let err = install_from_archive(
            &zip,
            &tmp.path().join("root"),
            &manifest,
            &entry,
        )
        .unwrap_err();
        assert!(matches!(err, EngineError::UnsafeArchive(_)));
    }

    #[test]
    fn rejects_single_file_zip_bomb_by_declared_uncompressed_size() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("large-entry.zip");
        let mut writer = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
        writer
            .start_file("pkg/engine.bin", zip::write::SimpleFileOptions::default())
            .unwrap();
        let mut repeated = io::repeat(b'x').take(MAX_EXTRACTED_FILE_BYTES + 1);
        io::copy(&mut repeated, &mut writer).unwrap();
        writer.finish().unwrap();
        let entry = PlatformEntry {
            url: "https://example.invalid/a.zip".into(),
            sha256: sha256_file(&zip_path).unwrap(),
            extract_dir: Some("pkg".into()),
            bin: "engine.bin".into(),
            bin_sha256: sha(b"engine"),
            max_bytes: None,
        };
        let manifest = EngineManifest {
            version: "9.9.9".into(),
            min_system_version: "1.36.0".into(),
            platforms: [("test".into(), entry.clone())].into_iter().collect(),
        };
        let err = install_from_archive(
            &zip_path,
            &tmp.path().join("root"),
            &manifest,
            &entry,
        )
        .unwrap_err();
        assert!(matches!(err, EngineError::TooLarge(MAX_EXTRACTED_FILE_BYTES)));
    }

    #[test]
    fn rejects_excessive_entry_count_before_extracting() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("many.zip");
        let mut writer = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
        for i in 0..=MAX_ARCHIVE_ENTRIES {
            writer
                .start_file(format!("pkg/f{i}"), zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"x").unwrap();
        }
        writer.finish().unwrap();
        let entry = PlatformEntry {
            url: "https://example.invalid/a.zip".into(),
            sha256: sha256_file(&zip_path).unwrap(),
            extract_dir: Some("pkg".into()),
            bin: "engine.bin".into(),
            bin_sha256: sha(b"engine"),
            max_bytes: None,
        };
        let manifest = EngineManifest {
            version: "9.9.9".into(),
            min_system_version: "1.36.0".into(),
            platforms: [("test".into(), entry.clone())].into_iter().collect(),
        };
        let err = install_from_archive(
            &zip_path,
            &tmp.path().join("root"),
            &manifest,
            &entry,
        )
        .unwrap_err();
        assert!(matches!(err, EngineError::TooLarge(_)));
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

    #[test]
    fn redirect_policy_rejects_downgrades_credentials_and_untrusted_hosts() {
        let original = reqwest::Url::parse("https://github.com/releases/file.zip").unwrap();
        assert!(redirect_is_allowed(
            Some("github.com"),
            &reqwest::Url::parse("https://github.com/other/file.zip").unwrap()
        ));
        assert!(redirect_is_allowed(
            Some("github.com"),
            &reqwest::Url::parse("https://release-assets.githubusercontent.com/file").unwrap()
        ));
        assert!(!redirect_is_allowed(
            Some("github.com"),
            &reqwest::Url::parse("http://github.com/file").unwrap()
        ));
        assert!(!redirect_is_allowed(
            Some("github.com"),
            &reqwest::Url::parse("https://attacker.invalid/file").unwrap()
        ));
        assert!(!redirect_is_allowed(
            Some("github.com"),
            &reqwest::Url::parse("https://user:pass@github.com/file").unwrap()
        ));
        assert!(redirect_is_allowed(original.host_str(), &original));
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
