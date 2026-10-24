use crate::error::{EngineError, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Component, Path};

/// The manifest is compiled into the binary, so the trusted hashes ship inside
/// the signed app and cannot be swapped by whoever hosts the download.
pub const EMBEDDED_JSON: &str = include_str!("../../../engine-manifest.json");

#[derive(Debug, Clone, Deserialize)]
pub struct EngineManifest {
    pub version: String,
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

    fn validate(&self) -> Result<()> {
        let bad = |msg: String| Err(EngineError::Manifest(msg));
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
    }

    #[test]
    fn rejects_http_urls() {
        let json = r#"{"version":"1","platforms":{"windows-x86_64":{"url":"http://x/y.zip",
            "sha256":"0000000000000000000000000000000000000000000000000000000000000000",
            "bin":"a.exe","bin_sha256":"0000000000000000000000000000000000000000000000000000000000000000"}}}"#;
        assert!(EngineManifest::parse(json).is_err());
    }

    #[test]
    fn rejects_path_traversal_and_bad_hashes() {
        let good = "0000000000000000000000000000000000000000000000000000000000000000";
        let trav = format!(
            r#"{{"version":"1","platforms":{{"k":{{"url":"https://x/y.zip","sha256":"{good}","bin":"../a.exe","bin_sha256":"{good}"}}}}}}"#
        );
        assert!(EngineManifest::parse(&trav).is_err());
        let short = format!(
            r#"{{"version":"1","platforms":{{"k":{{"url":"https://x/y.zip","sha256":"abc","bin":"a.exe","bin_sha256":"{good}"}}}}}}"#
        );
        assert!(EngineManifest::parse(&short).is_err());
        let ver = format!(
            r#"{{"version":"../1","platforms":{{"k":{{"url":"https://x/y.zip","sha256":"{good}","bin":"a.exe","bin_sha256":"{good}"}}}}}}"#
        );
        assert!(EngineManifest::parse(&ver).is_err());
    }
}
