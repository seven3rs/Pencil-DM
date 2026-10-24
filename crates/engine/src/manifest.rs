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

#[derive(Debug, Clone, Deserialize)]
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

    fn validate(&self) -> Result<()> {
        let bad = |msg: String| Err(EngineError::Manifest(msg));
        if parse_version(&self.min_system_version).is_none() {
            return bad(format!(
                "invalid min_system_version: {:?}",
                self.min_system_version
            ));
        }
        if self.version.is_empty()
            || !self
                .version
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
        {
            return bad(format!("bad version string: {:?}", self.version));
        }
        for (key, e) in &self.platforms {
            if !e.url.starts_with("https://") {
                return bad(format!("{key}: url must be https"));
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
        || version.split('.').any(|part| {
            part.is_empty() || !part.chars().all(|c| c.is_ascii_digit())
        })
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
    if p.is_empty() {
        return false;
    }
    Path::new(p)
        .components()
        .all(|c| matches!(c, Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn rejects_invalid_minimum_system_version() {
        let json = r#"{"version":"1","min_system_version":"latest","platforms":{}}"#;
        assert!(EngineManifest::parse(json).is_err());
    }

    #[test]
    fn rejects_http_urls() {
        let json = r#"{"version":"1","min_system_version":"1.36.0","platforms":{"windows-x86_64":{"url":"http://x/y.zip",
            "sha256":"0000000000000000000000000000000000000000000000000000000000000000",
            "bin":"a.exe","bin_sha256":"0000000000000000000000000000000000000000000000000000000000000000"}}}"#;
        assert!(EngineManifest::parse(json).is_err());
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
}
