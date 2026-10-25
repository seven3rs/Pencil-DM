//! Resolution of the engine that ships inside the installer (a Tauri sidecar).
//!
//! Order: user-configured absolute path, then the bundled sidecar (hash-checked
//! against a digest embedded at compile time), then the managed download, then
//! the system aria2. This module decides only the first two steps; callers fall
//! back to [`crate::resolve_engine`] for the rest. It never writes to disk.

use crate::install::sha256_file;
use std::path::{Path, PathBuf};

/// The bundled engine and the digest the app was compiled with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundledEngine {
    pub path: PathBuf,
    pub sha256: String,
}

/// What [`choose`] decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// A user-configured path that passed the compatibility check.
    Custom(PathBuf),
    /// The bundled sidecar exists and matches its embedded hash.
    Bundled(PathBuf),
    /// Continue with managed download / system search. `warning` explains why
    /// the bundled engine was skipped, for the engine panel.
    Fallback { warning: Option<String> },
}

pub fn choose(
    custom: Option<&Path>,
    custom_is_usable: impl Fn(&Path) -> bool,
    bundled: Option<&BundledEngine>,
) -> Choice {
    if let Some(path) = custom {
        if path.is_absolute() && custom_is_usable(path) {
            return Choice::Custom(path.to_path_buf());
        }
    }
    let Some(bundled) = bundled else {
        return Choice::Fallback { warning: None };
    };
    if !bundled.path.is_file() {
        return Choice::Fallback {
            warning: Some(format!(
                "The bundled download engine is missing ({}). Using a downloaded or system copy instead; reinstalling the app restores it.",
                bundled.path.display()
            )),
        };
    }
    match sha256_file(&bundled.path) {
        Ok(actual) if actual.eq_ignore_ascii_case(&bundled.sha256) => {
            Choice::Bundled(bundled.path.clone())
        }
        Ok(_) => Choice::Fallback {
            warning: Some(format!(
                "The bundled download engine ({}) does not match the expected checksum, so it was not run. It may have been modified or damaged by other software. Using a downloaded or system copy instead; reinstalling the app restores it.",
                bundled.path.display()
            )),
        },
        Err(error) => Choice::Fallback {
            warning: Some(format!(
                "The bundled download engine ({}) could not be read: {error}. Using a downloaded or system copy instead.",
                bundled.path.display()
            )),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(bytes: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        hex::encode(Sha256::digest(bytes))
    }

    fn bundled_in(dir: &Path, contents: &[u8], expected: &[u8]) -> BundledEngine {
        let path = dir.join("aria2c");
        std::fs::write(&path, contents).unwrap();
        BundledEngine {
            path,
            sha256: digest(expected),
        }
    }

    #[test]
    fn custom_path_wins_over_a_valid_bundled_engine() {
        let dir = tempfile::tempdir().unwrap();
        let bundled = bundled_in(dir.path(), b"engine", b"engine");
        let custom = dir.path().join("custom-aria2c");
        std::fs::write(&custom, b"x").unwrap();
        let choice = choose(Some(custom.as_path()), |_| true, Some(&bundled));
        assert_eq!(choice, Choice::Custom(custom));
    }

    #[test]
    fn unusable_or_relative_custom_path_does_not_win() {
        let dir = tempfile::tempdir().unwrap();
        let bundled = bundled_in(dir.path(), b"engine", b"engine");
        let custom = dir.path().join("custom-aria2c");
        assert_eq!(
            choose(Some(custom.as_path()), |_| false, Some(&bundled)),
            Choice::Bundled(bundled.path.clone())
        );
        assert_eq!(
            choose(Some(Path::new("aria2c")), |_| true, Some(&bundled)),
            Choice::Bundled(bundled.path.clone())
        );
    }

    #[test]
    fn bundled_engine_is_used_when_hash_matches() {
        let dir = tempfile::tempdir().unwrap();
        let bundled = bundled_in(dir.path(), b"engine", b"engine");
        assert_eq!(
            choose(None, |_| false, Some(&bundled)),
            Choice::Bundled(bundled.path.clone())
        );
    }

    #[test]
    fn modified_bundled_engine_is_never_chosen_and_warns() {
        let dir = tempfile::tempdir().unwrap();
        let bundled = bundled_in(dir.path(), b"tampered", b"engine");
        match choose(None, |_| false, Some(&bundled)) {
            Choice::Fallback { warning: Some(w) } => assert!(w.contains("checksum")),
            other => panic!("expected fallback with warning, got {other:?}"),
        }
    }

    #[test]
    fn missing_bundled_engine_falls_back_with_a_clear_warning() {
        let dir = tempfile::tempdir().unwrap();
        let bundled = BundledEngine {
            path: dir.path().join("aria2c"),
            sha256: digest(b"engine"),
        };
        match choose(None, |_| false, Some(&bundled)) {
            Choice::Fallback { warning: Some(w) } => assert!(w.contains("missing")),
            other => panic!("expected fallback with warning, got {other:?}"),
        }
    }

    #[test]
    fn no_bundled_engine_configured_falls_back_silently() {
        assert_eq!(
            choose(None, |_| false, None),
            Choice::Fallback { warning: None }
        );
    }
}
