fn main() {
    // The sidecar's SHA-256 is compiled into the app so the launcher can refuse a
    // modified bundled engine. `scripts/fetch-engine.mjs` writes the digest file
    // (git-ignored); builds without a bundled engine (Linux, dev) simply omit it.
    println!("cargo:rerun-if-env-changed=PDM_BUNDLED_ENGINE_SHA256");
    println!("cargo:rerun-if-changed=binaries/engine-sidecar.sha256");
    let digest = std::env::var("PDM_BUNDLED_ENGINE_SHA256")
        .ok()
        .or_else(|| std::fs::read_to_string("binaries/engine-sidecar.sha256").ok())
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|h| h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit()));
    if let Some(digest) = digest {
        println!("cargo:rustc-env=PDM_BUNDLED_ENGINE_SHA256={digest}");
    }
    tauri_build::build()
}
