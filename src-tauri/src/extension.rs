//! Helps the user get the browser extension into their browser.
//!
//! Chrome, Edge and Firefox do not let a desktop app install an extension silently, and
//! Chrome 137+ removed `--load-extension`. What the app can do reliably is: put a ready
//! to load copy of the extension in a folder that does not move, open the browser's
//! extensions page, and then watch for the extension to pair (see `browser.rs`).

use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::Manager;

/// Files that make up the extension, relative to the extension folder.
pub const EXTENSION_FILES: [&str; 6] = [
    "background.js",
    "popup.html",
    "popup.css",
    "popup.js",
    "icon.png",
    "PRIVACY.md",
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Family {
    Chromium,
    Firefox,
}

impl Family {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "chromium" => Ok(Self::Chromium),
            "firefox" => Ok(Self::Firefox),
            _ => Err("Unknown browser family.".into()),
        }
    }

    fn dir_name(self) -> &'static str {
        match self {
            Self::Chromium => "chromium",
            Self::Firefox => "firefox",
        }
    }

    fn manifest_source(self) -> &'static str {
        match self {
            Self::Chromium => "manifest.json",
            Self::Firefox => "manifest.firefox.json",
        }
    }
}

struct Known {
    id: &'static str,
    name: &'static str,
    family: Family,
    extensions_url: &'static str,
}

const BROWSERS: [Known; 6] = [
    Known {
        id: "chrome",
        name: "Google Chrome",
        family: Family::Chromium,
        extensions_url: "chrome://extensions",
    },
    Known {
        id: "edge",
        name: "Microsoft Edge",
        family: Family::Chromium,
        extensions_url: "edge://extensions",
    },
    Known {
        id: "brave",
        name: "Brave",
        family: Family::Chromium,
        extensions_url: "brave://extensions",
    },
    Known {
        id: "vivaldi",
        name: "Vivaldi",
        family: Family::Chromium,
        extensions_url: "vivaldi://extensions",
    },
    Known {
        id: "chromium",
        name: "Chromium",
        family: Family::Chromium,
        extensions_url: "chrome://extensions",
    },
    Known {
        id: "firefox",
        name: "Firefox",
        family: Family::Firefox,
        extensions_url: "about:debugging#/runtime/this-firefox",
    },
];

#[derive(Serialize, Clone, Debug)]
pub struct BrowserInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub family: &'static str,
    pub installed: bool,
}

pub fn detect() -> Vec<BrowserInfo> {
    BROWSERS
        .iter()
        .map(|known| BrowserInfo {
            id: known.id,
            name: known.name,
            family: known.family.dir_name(),
            installed: locate(known.id).is_some(),
        })
        .collect()
}

/// Folder the user loads the extension from. It stays put between app updates so the
/// browser keeps pointing at it.
pub fn export_dir(data_root: &Path, family: Family) -> PathBuf {
    data_root.join("browser-extension").join(family.dir_name())
}

/// Where the extension sources live: bundled with the app, or the repo in development.
fn source_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let mut candidates = Vec::new();
    if let Ok(resources) = app.path().resource_dir() {
        candidates.push(resources.join("extension"));
    }
    candidates.push(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("extension"),
    );
    candidates
        .into_iter()
        .find(|dir| dir.join("background.js").is_file())
        .ok_or_else(|| "The extension files are missing from this installation.".to_string())
}

/// Copies the extension to its stable folder, replacing older files. Returns the folder.
pub fn export(app: &tauri::AppHandle, data_root: &Path, family: Family) -> Result<PathBuf, String> {
    let source = source_dir(app)?;
    let target = export_dir(data_root, family);
    copy_extension(&source, &target, family)?;
    Ok(target)
}

pub fn copy_extension(source: &Path, target: &Path, family: Family) -> Result<(), String> {
    std::fs::create_dir_all(target)
        .map_err(|e| format!("Could not create {}: {e}", target.display()))?;
    for file in EXTENSION_FILES {
        std::fs::copy(source.join(file), target.join(file))
            .map_err(|e| format!("Could not copy {file}: {e}"))?;
    }
    std::fs::copy(
        source.join(family.manifest_source()),
        target.join("manifest.json"),
    )
    .map_err(|e| format!("Could not copy the extension manifest: {e}"))?;
    Ok(())
}

/// Keeps folders the user already loaded in step with the installed app version.
pub fn refresh_existing(app: &tauri::AppHandle, data_root: &Path) {
    for family in [Family::Chromium, Family::Firefox] {
        if export_dir(data_root, family)
            .join("manifest.json")
            .is_file()
        {
            let _ = export(app, data_root, family);
        }
    }
}

pub fn open_extensions_page(browser_id: &str) -> Result<(), String> {
    let known = BROWSERS
        .iter()
        .find(|known| known.id == browser_id)
        .ok_or_else(|| "Unknown browser.".to_string())?;
    let launcher = locate(browser_id)
        .ok_or_else(|| format!("{} was not found on this computer.", known.name))?;
    launcher.open(known.extensions_url)
}

enum Launcher {
    Program(PathBuf),
    #[cfg(target_os = "macos")]
    MacApp(String),
}

impl Launcher {
    fn open(&self, url: &str) -> Result<(), String> {
        let mut command = match self {
            Self::Program(path) => {
                let mut command = std::process::Command::new(path);
                command.arg(url);
                command
            }
            #[cfg(target_os = "macos")]
            Self::MacApp(app) => {
                let mut command = std::process::Command::new("open");
                command.args(["-a", app, url]);
                command
            }
        };
        command.spawn().map(|_| ()).map_err(|e| e.to_string())
    }
}

#[cfg(windows)]
fn locate(id: &str) -> Option<Launcher> {
    let relative = match id {
        "chrome" => r"Google\Chrome\Application\chrome.exe",
        "edge" => r"Microsoft\Edge\Application\msedge.exe",
        "brave" => r"BraveSoftware\Brave-Browser\Application\brave.exe",
        "vivaldi" => r"Vivaldi\Application\vivaldi.exe",
        "chromium" => r"Chromium\Application\chrome.exe",
        "firefox" => r"Mozilla Firefox\firefox.exe",
        _ => return None,
    };
    ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"]
        .iter()
        .filter_map(|var| std::env::var_os(var))
        .map(|root| PathBuf::from(root).join(relative))
        .find(|path| path.is_file())
        .map(Launcher::Program)
}

#[cfg(target_os = "macos")]
fn locate(id: &str) -> Option<Launcher> {
    let app = match id {
        "chrome" => "Google Chrome",
        "edge" => "Microsoft Edge",
        "brave" => "Brave Browser",
        "vivaldi" => "Vivaldi",
        "chromium" => "Chromium",
        "firefox" => "Firefox",
        _ => return None,
    };
    let mut roots = vec![PathBuf::from("/Applications")];
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join("Applications"));
    }
    roots
        .iter()
        .any(|root| root.join(format!("{app}.app")).is_dir())
        .then(|| Launcher::MacApp(app.to_string()))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn locate(id: &str) -> Option<Launcher> {
    let names: &[&str] = match id {
        "chrome" => &["google-chrome-stable", "google-chrome"],
        "edge" => &["microsoft-edge-stable", "microsoft-edge"],
        "brave" => &["brave-browser", "brave"],
        "vivaldi" => &["vivaldi-stable", "vivaldi"],
        "chromium" => &["chromium", "chromium-browser"],
        "firefox" => &["firefox"],
        _ => return None,
    };
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .flat_map(|dir| names.iter().map(move |name| dir.join(name)))
        .find(|candidate| candidate.is_file())
        .map(Launcher::Program)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_source(dir: &Path) {
        for file in EXTENSION_FILES {
            std::fs::write(dir.join(file), file).unwrap();
        }
        std::fs::write(dir.join("manifest.json"), "chromium").unwrap();
        std::fs::write(dir.join("manifest.firefox.json"), "firefox").unwrap();
    }

    #[test]
    fn copies_the_right_manifest_for_each_family() {
        let source = tempfile::tempdir().unwrap();
        fake_source(source.path());
        for (family, expected) in [(Family::Chromium, "chromium"), (Family::Firefox, "firefox")] {
            let target = tempfile::tempdir().unwrap();
            let out = target.path().join("ext");
            copy_extension(source.path(), &out, family).unwrap();
            assert_eq!(
                std::fs::read_to_string(out.join("manifest.json")).unwrap(),
                expected
            );
            assert!(out.join("background.js").is_file());
            assert!(!out.join("manifest.firefox.json").exists());
        }
    }

    #[test]
    fn copy_overwrites_stale_files_and_reports_missing_sources() {
        let source = tempfile::tempdir().unwrap();
        fake_source(source.path());
        let target = tempfile::tempdir().unwrap();
        std::fs::write(target.path().join("background.js"), "old").unwrap();
        copy_extension(source.path(), target.path(), Family::Chromium).unwrap();
        assert_eq!(
            std::fs::read_to_string(target.path().join("background.js")).unwrap(),
            "background.js"
        );
        std::fs::remove_file(source.path().join("popup.js")).unwrap();
        assert!(copy_extension(source.path(), target.path(), Family::Chromium).is_err());
    }

    #[test]
    fn rejects_unknown_families_and_browsers() {
        assert!(Family::parse("chromium").is_ok());
        assert!(Family::parse("../etc").is_err());
        assert!(open_extensions_page("not-a-browser").is_err());
    }

    #[test]
    fn export_dir_is_stable_per_family() {
        let root = Path::new("/data");
        assert_ne!(
            export_dir(root, Family::Chromium),
            export_dir(root, Family::Firefox)
        );
        assert_eq!(
            export_dir(root, Family::Chromium),
            export_dir(root, Family::Chromium)
        );
    }
}
