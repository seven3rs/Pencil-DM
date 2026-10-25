use crate::error::{EngineError, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Component, Path};
use std::time::Duration;

/// The manifest is compiled into the binary, so the trusted hashes ship inside
/// the signed app and cannot be swapped by whoever hosts the download.
pub const EMBEDDED_JSON: &str = include_str!("../../../engine-manifest.json");

#[derive(Debug, Clone, Deserialize)]
pub struct EngineManifest {
    pub version: String,
    /// Minimum version accepted for system-installed engines.
    pub min_system_version: String,
    pub platforms: BTreeMap<String, PlatformEntry>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct PlatformEntry {
    pub url: String,
    /// SHA-256 of the downloaded archive.
    pub sha256: String,
    /// Sub-directory inside the archive to extract (like Scoop's `extract_dir`).
    pub extract_dir: Option<String>,
    /// Engine executable, relative to the extracted directory.
    pub bin: String,
    /// SHA-256 of the executable; re-checked on every launch.
    pub bin_sha256: String,
    /// Optional download size cap in bytes.
    pub max_bytes: Option<u64>,
}

/// Key used to look up the current platform, e.g. `windows-x86_64`.
pub fn platform_key() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

impl EngineManifest {
    pub fn embedded() -> Result<Self> {
        Self::parse(EMBEDDED_JSON)
    }

    pub fn parse(json: &str) -> Result<Self> {
        let m: EngineManifest = serde_json::from_str(json)?;
        m.validate()?;
        Ok(m)
    }

    pub fn entry_for_current(&self) -> Option<&PlatformEntry> {
        self.platforms.get(&platform_key())
    }

    /// Refresh the system-engine minimum from the latest stable upstream release.
    /// The embedded manifest remains the offline fallback; this endpoint supplies
    /// version metadata only and is never used to obtain trusted binary hashes.
    pub async fn refresh_system_minimum(&mut self) {
        #[derive(Deserialize)]
        struct LatestRelease {
            tag_name: String,
        }

        let client = match reqwest::Client::builder()
            .timeout(Duration::from_secs(3))
            .user_agent(concat!("PencilDownloadManager/", env!("CARGO_PKG_VERSION")))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 5 {
                    return attempt.error("too many release metadata redirects");
                }
                let target = attempt.url();
                if target.scheme() != "https"
                    || !target.username().is_empty()
                    || target.password().is_some()
                {
                    return attempt.error("unsafe release metadata redirect");
                }
                if matches!(target.host_str(), Some("api.github.com" | "github.com")) {
                    attempt.follow()
                } else {
                    attempt.error("untrusted release metadata redirect host")
                }
            }))
            .build()
        {
            Ok(client) => client,
            Err(_) => return,
        };
        let Ok(response) = client
            .get("https://api.github.com/repos/aria2/aria2/releases/latest")
            .send()
            .await
        else {
            return;
        };
        if !response.status().is_success() {
            return;
        }
        let Ok(release) = response.json::<LatestRelease>().await else {
            return;
        };
        if let Some(version) = parse_release_tag(&release.tag_name) {
            self.min_system_version = version;
        }
    }

    pub(crate) fn validate(&self) -> Result<()> {
        let bad = |msg: String| Err(EngineError::Manifest(msg));
        if parse_version(&self.min_system_version).is_none() {
            return bad(format!(
                "invalid min_system_version: {:?}",
                self.min_system_version
            ));
        }
        if self.version.is_empty()
            || self.version.starts_with('.')
            || self.version.ends_with('.')
            || !self
                .version
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
        {
            return bad(format!("bad version string: {:?}", self.version));
        }
        for (key, e) in &self.platforms {
            let parsed_url = match reqwest::Url::parse(&e.url) {
                Ok(url) => url,
                Err(_) => return bad(format!("{key}: invalid engine URL")),
            };
            if parsed_url.scheme() != "https"
                || parsed_url.host_str().is_none()
                || !parsed_url.username().is_empty()
                || parsed_url.password().is_some()
            {
                return bad(format!(
                    "{key}: url must be an HTTPS URL without embedded credentials"
                ));
            }
            for (name, h) in [("sha256", &e.sha256), ("bin_sha256", &e.bin_sha256)] {
                if h.len() != 64 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
                    return bad(format!("{key}: {name} is not a 64-char hex digest"));
                }
            }
            if !is_safe_relative(&e.bin) {
                return bad(format!("{key}: unsafe bin path {:?}", e.bin));
            }
            if let Some(d) = &e.extract_dir {
                if !is_safe_relative(d) {
                    return bad(format!("{key}: unsafe extract_dir {d:?}"));
                }
            }
        }
        Ok(())
    }
}

/// Pinned upstream source release. Used by the build-time fetch script and for
/// the GPL source offer; the runtime installer never reads it.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct SourceEntry {
    pub url: String,
    /// SHA-256 of the source tarball.
    pub sha256: String,
    /// Upstream git tag, e.g. `release-1.37.0`.
    pub tag: String,
    /// Top-level directory inside the tarball.
    pub extract_dir: String,
    /// SHA-256 of the `COPYING` file inside the tarball.
    pub copying_sha256: String,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SidecarMode {
    /// Use the prebuilt archive pinned in `platforms`.
    Download,
    /// Build from the pinned source tarball.
    BuildFromSource,
}

/// How the installer's bundled engine (Tauri sidecar) is produced per platform key.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct SidecarEntry {
    pub mode: SidecarMode,
    /// Rust target triple used in the `aria2c-<triple>` sidecar file name.
    pub target_triple: String,
    /// Minimum macOS version to build for (build-from-source only).
    pub deployment_target: Option<String>,
}

/// Build-time view of `engine-manifest.json`. Kept separate from
/// [`EngineManifest`] so the runtime installer's data model is unchanged.
#[derive(Debug, Clone, Deserialize)]
pub struct BuildManifest {
    pub source: SourceEntry,
    pub sidecars: BTreeMap<String, SidecarEntry>,
}

fn is_hex64(h: &str) -> bool {
    h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit())
}

impl BuildManifest {
    pub fn embedded() -> Result<Self> {
        Self::parse(EMBEDDED_JSON)
    }

    /// Parse and cross-check against the runtime manifest in the same JSON.
    pub fn parse(json: &str) -> Result<Self> {
        let build: BuildManifest = serde_json::from_str(json)?;
        let runtime = EngineManifest::parse(json)?;
        build.validate(&runtime)?;
        Ok(build)
    }

    fn validate(&self, runtime: &EngineManifest) -> Result<()> {
        let bad = |msg: String| Err(EngineError::Manifest(msg));
        let s = &self.source;
        match reqwest::Url::parse(&s.url) {
            Ok(u)
                if u.scheme() == "https"
                    && u.host_str().is_some()
                    && u.username().is_empty()
                    && u.password().is_none() => {}
            _ => return bad("source: url must be an HTTPS URL without credentials".into()),
        }
        if !is_hex64(&s.sha256) || !is_hex64(&s.copying_sha256) {
            return bad("source: sha256 values must be 64-char hex digests".into());
        }
        if !is_safe_relative(&s.extract_dir) {
            return bad(format!("source: unsafe extract_dir {:?}", s.extract_dir));
        }
        if s.tag.is_empty()
            || !s
                .tag
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
        {
            return bad(format!("source: bad tag {:?}", s.tag));
        }
        for (key, e) in &self.sidecars {
            if e.target_triple.is_empty()
                || !e
                    .target_triple
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            {
                return bad(format!("{key}: bad target_triple {:?}", e.target_triple));
            }
            if let Some(d) = &e.deployment_target {
                if parse_version(d).is_none() {
                    return bad(format!("{key}: bad deployment_target {d:?}"));
                }
            }
            match e.mode {
                SidecarMode::Download if !runtime.platforms.contains_key(key) => {
                    return bad(format!("{key}: download mode needs a platforms entry"));
                }
                SidecarMode::BuildFromSource if e.deployment_target.is_none() => {
                    return bad(format!("{key}: build-from-source needs deployment_target"));
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// Parse the first dotted numeric version in command output. For example,
/// "aria2 version 1.37.0" becomes `[1, 37, 0]`.
pub fn parse_version(output: &str) -> Option<Vec<u64>> {
    for word in output.split_whitespace() {
        let candidate = word.trim_matches(|c: char| !c.is_ascii_digit() && c != '.');
        if !candidate.contains('.') || candidate.is_empty() || candidate.ends_with('.') {
            continue;
        }
        let parts: Option<Vec<u64>> = candidate
            .split('.')
            .map(|part| {
                if part.is_empty() {
                    None
                } else {
                    part.parse().ok()
                }
            })
            .collect();
        if let Some(parts) = parts {
            if !parts.is_empty() {
                return Some(parts);
            }
        }
    }
    None
}

/// Accept only stable numeric upstream tags such as `release-1.37.0` or `v1.37.0`.
pub fn parse_release_tag(tag: &str) -> Option<String> {
    let version = tag
        .strip_prefix("release-")
        .or_else(|| tag.strip_prefix('v'))
        .unwrap_or(tag);
    let parsed = parse_version(version)?;
    if parsed.len() < 2
        || version
            .split('.')
            .any(|part| part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()))
    {
        return None;
    }
    Some(version.to_string())
}

pub fn version_at_least(found: &[u64], minimum: &[u64]) -> bool {
    let length = found.len().max(minimum.len());
    (0..length)
        .map(|i| found.get(i).copied().unwrap_or(0))
        .cmp((0..length).map(|i| minimum.get(i).copied().unwrap_or(0)))
        .is_ge()
}

fn is_safe_relative(p: &str) -> bool {
    if p.is_empty()
        || p.starts_with('/')
        || p.starts_with('\\')
        || p.contains('\\')
        || p.contains(':')
        || p.chars().any(char::is_control)
    {
        return false;
    }
    Path::new(p)
        .components()
        .all(|c| matches!(c, Component::Normal(_)))
        && p.split('/').all(|part| {
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
                "CON"
                    | "PRN"
                    | "AUX"
                    | "NUL"
                    | "COM1"
                    | "COM2"
                    | "COM3"
                    | "COM4"
                    | "COM5"
                    | "COM6"
                    | "COM7"
                    | "COM8"
                    | "COM9"
                    | "LPT1"
                    | "LPT2"
                    | "LPT3"
                    | "LPT4"
                    | "LPT5"
                    | "LPT6"
                    | "LPT7"
                    | "LPT8"
                    | "LPT9"
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn embedded_manifest_is_valid_and_has_windows_entries() {
        let m = EngineManifest::embedded().expect("embedded manifest must parse");
        assert!(m.platforms.contains_key("windows-x86_64"));
        assert!(m.platforms.contains_key("windows-x86"));
        assert_eq!(m.min_system_version, "1.36.0");
    }

    #[test]
    fn parses_and_compares_system_versions() {
        assert_eq!(
            parse_release_tag("release-1.37.0").as_deref(),
            Some("1.37.0")
        );
        assert_eq!(parse_release_tag("v1.38.1").as_deref(), Some("1.38.1"));
        assert_eq!(parse_release_tag("release-1.39.0-rc1"), None);
        assert_eq!(parse_release_tag("latest"), None);
        assert_eq!(parse_version("aria2 version 1.37.0"), Some(vec![1, 37, 0]));
        assert_eq!(parse_version("version 1.36"), Some(vec![1, 36]));
        assert_eq!(parse_version("no version here"), None);
        assert!(version_at_least(&[1, 37, 0], &[1, 36, 0]));
        assert!(version_at_least(&[1, 36], &[1, 36, 0]));
        assert!(!version_at_least(&[1, 35, 9], &[1, 36, 0]));
    }

    #[test]
    fn manifest_validation_rejects_malformed_generated_paths_and_digests() {
        let digest = "0".repeat(64);
        let unsafe_paths = [
            "../engine",
            "a/../engine",
            "\\\\server\\share",
            "C:\\engine.exe",
            "/tmp/engine",
            "CON.exe",
            "folder\\engine.exe",
        ];
        for path in unsafe_paths {
            let json = serde_json::json!({
                "version": "1.0.0", "min_system_version": "1.36.0",
                "platforms": {"test": {
                    "url": "https://github.com/example/engine.zip",
                    "sha256": digest,
                    "bin": path,
                    "bin_sha256": digest
                }}
            })
            .to_string();
            assert!(
                EngineManifest::parse(&json).is_err(),
                "accepted manifest path {path:?}"
            );
        }
        for malformed in ["", "not-json", "{", "null", "[]", r#"{"version":1}"#] {
            assert!(
                EngineManifest::parse(malformed).is_err(),
                "accepted malformed manifest {malformed:?}"
            );
        }
    }

    #[test]
    fn stable_release_tags_reject_suffixes_and_non_numeric_components() {
        for tag in [
            "release-1.2.3-rc1",
            "v1..2",
            "1.2.x",
            "https://evil.test/1.2.3",
            "",
        ] {
            assert!(
                parse_release_tag(tag).is_none(),
                "accepted release tag {tag:?}"
            );
        }
    }

    #[test]
    fn rejects_invalid_minimum_system_version() {
        let json = r#"{"version":"1","min_system_version":"latest","platforms":{}}"#;
        assert!(EngineManifest::parse(json).is_err());
    }

    #[test]
    fn build_manifest_embedded_is_valid() {
        let b = BuildManifest::embedded().expect("embedded build manifest must parse");
        assert!(b.source.url.ends_with(".tar.xz"));
        assert_eq!(b.sidecars["windows-x86_64"].mode, SidecarMode::Download);
        assert_eq!(
            b.sidecars["macos-aarch64"].mode,
            SidecarMode::BuildFromSource
        );
        assert_eq!(b.sidecars["macos-x86_64"].target_triple, "x86_64-apple-darwin");
    }

    #[test]
    fn old_manifests_without_build_fields_still_parse_as_runtime_manifests() {
        let json = r#"{"version":"1","min_system_version":"1.36.0","platforms":{}}"#;
        assert!(EngineManifest::parse(json).is_ok());
        assert!(BuildManifest::parse(json).is_err());
    }

    #[test]
    fn build_manifest_rejects_bad_source_and_sidecars() {
        let good: serde_json::Value = serde_json::from_str(EMBEDDED_JSON).unwrap();
        let mutate = |f: &dyn Fn(&mut serde_json::Value)| {
            let mut v = good.clone();
            f(&mut v);
            BuildManifest::parse(&v.to_string())
        };
        assert!(mutate(&|v| v["source"]["url"] = "http://x/a.tar.xz".into()).is_err());
        assert!(mutate(&|v| v["source"]["sha256"] = "abc".into()).is_err());
        assert!(mutate(&|v| v["source"]["extract_dir"] = "../x".into()).is_err());
        assert!(mutate(&|v| v["source"]["tag"] = "a/b".into()).is_err());
        assert!(mutate(&|v| v["sidecars"]["macos-aarch64"]["mode"] = "run-it".into()).is_err());
        assert!(mutate(&|v| v["sidecars"]["macos-aarch64"]["target_triple"] = "a b".into()).is_err());
        assert!(
            mutate(&|v| v["sidecars"]["macos-aarch64"]["deployment_target"] = serde_json::Value::Null)
                .is_err()
        );
        assert!(mutate(&|v| v["sidecars"]["windows-x86_64"]["mode"] = "download".into()).is_ok());
        assert!(mutate(&|v| {
            v["platforms"].as_object_mut().unwrap().remove("windows-x86");
        })
        .is_err());
    }

    #[test]
    fn rejects_http_urls() {
        let json = r#"{"version":"1","min_system_version":"1.36.0","platforms":{"windows-x86_64":{"url":"http://x/y.zip",
            "sha256":"0000000000000000000000000000000000000000000000000000000000000000",
            "bin":"a.exe","bin_sha256":"0000000000000000000000000000000000000000000000000000000000000000"}}}"#;
        assert!(EngineManifest::parse(json).is_err());
    }

    #[test]
    fn rejects_dot_only_or_dot_terminated_versions() {
        let digest = "0".repeat(64);
        for version in [".", "..", "...", ".1.2", "1.2."] {
            let json = serde_json::json!({
                "version": version,
                "min_system_version": "1.36.0",
                "platforms": {"test": {
                    "url": "https://github.com/example/engine.zip",
                    "sha256": digest,
                    "bin": "engine.exe",
                    "bin_sha256": digest
                }}
            })
            .to_string();
            assert!(
                EngineManifest::parse(&json).is_err(),
                "accepted unsafe version {version:?}"
            );
        }
    }

    #[test]
    fn rejects_manifest_urls_with_embedded_credentials() {
        let digest = "0".repeat(64);
        let json = format!(
            r#"{{"version":"1","min_system_version":"1.36.0","platforms":{{"k":{{"url":"https://user:pass@example.test/a.zip","sha256":"{digest}","bin":"engine.exe","bin_sha256":"{digest}"}}}}}}"#
        );
        assert!(EngineManifest::parse(&json).is_err());
    }

    #[test]
    fn rejects_path_traversal_and_bad_hashes() {
        let good = "0000000000000000000000000000000000000000000000000000000000000000";
        let trav = format!(
            r#"{{"version":"1","min_system_version":"1.36.0","platforms":{{"k":{{"url":"https://x/y.zip","sha256":"{good}","bin":"../a.exe","bin_sha256":"{good}"}}}}}}"#
        );
        assert!(EngineManifest::parse(&trav).is_err());
        let short = format!(
            r#"{{"version":"1","min_system_version":"1.36.0","platforms":{{"k":{{"url":"https://x/y.zip","sha256":"abc","bin":"a.exe","bin_sha256":"{good}"}}}}}}"#
        );
        assert!(EngineManifest::parse(&short).is_err());
        let ver = format!(
            r#"{{"version":"../1","min_system_version":"1.36.0","platforms":{{"k":{{"url":"https://x/y.zip","sha256":"{good}","bin":"a.exe","bin_sha256":"{good}"}}}}}}"#
        );
        assert!(EngineManifest::parse(&ver).is_err());
    }

    proptest! {
        #[test]
        fn arbitrary_manifest_input_never_panics(input in any::<String>()) {
            let _ = EngineManifest::parse(&input);
        }
    }
}
