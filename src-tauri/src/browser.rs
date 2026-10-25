use pdm_engine::DownloadOptions;
use rand::{distributions::Alphanumeric, Rng, RngCore};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::Manager;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};
use url::Url;

const DISCOVERY_PORT: u16 = 17653;
const MAX_BODY: usize = 1024 * 1024;
const MAX_BATCH: usize = 20;
const RATE_WINDOW: Duration = Duration::from_secs(60);
const RATE_LIMIT: usize = 60;

#[derive(Clone)]
pub struct BrowserState(Arc<Mutex<SecurityState>>);

struct SecurityState {
    token: String,
    port: u16,
    pairing: Option<Pairing>,
    allowed_origin: Option<String>,
    requests: VecDeque<Instant>,
    /// Where token and paired origin are persisted so pairing survives app restarts.
    config_path: Option<PathBuf>,
    /// Unix seconds of the last authenticated request from the extension.
    last_seen: Option<u64>,
    /// Set when the loopback discovery socket could not be opened.
    discovery_error: Option<String>,
}

/// What is written to `browser-connection.json`.
#[derive(Debug, Deserialize, Serialize, Default, Clone)]
struct StoredConfig {
    #[serde(default)]
    port: u16,
    #[serde(default)]
    token: String,
    #[serde(default)]
    origin: Option<String>,
}

/// Browser integration state shown in the app.
#[derive(Debug, Serialize, Clone)]
pub struct BrowserStatus {
    pub paired: bool,
    pub origin: Option<String>,
    pub last_seen: Option<u64>,
    pub port: u16,
    pub discovery_ok: bool,
    pub discovery_error: Option<String>,
}

struct Pairing {
    code: String,
    expires: Instant,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DownloadRequest {
    pub url: String,
    #[serde(default)]
    pub referer: Option<String>,
    #[serde(default)]
    pub cookies: Option<String>,
    #[serde(default)]
    pub user_agent: Option<String>,
    #[serde(default)]
    pub file_name: Option<String>,
    #[serde(default)]
    pub headers: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Deserialize)]
struct PairRequest {
    code: String,
    origin: String,
}

#[derive(Serialize)]
struct PairResponse {
    token: String,
    port: u16,
}

impl BrowserState {
    pub fn new(port: u16, token: String) -> Self {
        Self(Arc::new(Mutex::new(SecurityState {
            token,
            port,
            pairing: None,
            allowed_origin: None,
            requests: VecDeque::new(),
            config_path: None,
            last_seen: None,
            discovery_error: None,
        })))
    }

    pub fn status(&self) -> Result<BrowserStatus, String> {
        let state = self
            .0
            .lock()
            .map_err(|_| "Browser integration lock failed".to_string())?;
        Ok(BrowserStatus {
            paired: state.allowed_origin.is_some(),
            origin: state.allowed_origin.clone(),
            last_seen: state.last_seen,
            port: state.port,
            discovery_ok: state.discovery_error.is_none(),
            discovery_error: state.discovery_error.clone(),
        })
    }

    /// Forgets the paired extension and rotates the token, so the old extension
    /// copy can no longer talk to the app until it is paired again.
    pub fn disconnect(&self) -> Result<(), String> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| "Browser integration lock failed".to_string())?;
        let mut raw = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut raw);
        state.token = hex::encode(raw);
        state.allowed_origin = None;
        state.pairing = None;
        state.last_seen = None;
        persist(&state)
    }

    pub fn create_pairing_code(&self) -> Result<String, String> {
        let code: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(8)
            .map(char::from)
            .collect();
        let mut state = self
            .0
            .lock()
            .map_err(|_| "Browser integration lock failed".to_string())?;
        state.pairing = Some(Pairing {
            code: code.clone(),
            expires: Instant::now() + Duration::from_secs(300),
        });
        Ok(code)
    }
}

pub fn start(app: tauri::AppHandle, data_dir: &Path) -> Result<BrowserState, String> {
    std::fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
    let config_path = data_dir.join("browser-connection.json");
    // Reuse the previous token, paired origin and port so an already paired extension
    // keeps working after the app restarts. Only a missing or damaged file starts over.
    let stored = load_config(&config_path);
    let token = match &stored {
        Some(config) => config.token.clone(),
        None => {
            let mut raw = [0u8; 32];
            rand::thread_rng().fill_bytes(&mut raw);
            hex::encode(raw)
        }
    };
    let preferred_port = stored.as_ref().map(|config| config.port).unwrap_or(0);

    let main_server = match preferred_port {
        0 => Server::http(("127.0.0.1", 0)),
        port => Server::http(("127.0.0.1", port)).or_else(|_| Server::http(("127.0.0.1", 0))),
    }
    .map_err(|e| e.to_string())?;
    let port = main_server
        .server_addr()
        .to_ip()
        .ok_or_else(|| "browser server did not bind an IP socket".to_string())?
        .port();
    let state = BrowserState::new(port, token.clone());
    {
        let mut security = state
            .0
            .lock()
            .map_err(|_| "Browser integration lock failed".to_string())?;
        security.allowed_origin = stored.and_then(|config| config.origin);
        security.config_path = Some(config_path);
        persist(&security)?;
    }

    let shared = state.clone();
    std::thread::Builder::new()
        .name("pdm-browser-http".into())
        .spawn(move || {
            for request in main_server.incoming_requests() {
                handle_request(request, &shared, &app);
            }
        })
        .map_err(|e| e.to_string())?;

    // Extensions cannot read the app's private data file. This loopback-only discovery
    // socket reveals only the ephemeral port; all useful operations still need the token.
    // If the port is taken the rest of the app (and an extension that already knows the
    // port) keeps working, so this is reported in the status instead of failing startup.
    match Server::http(("127.0.0.1", DISCOVERY_PORT)) {
        Ok(discovery) => {
            let discovery_state = state.clone();
            std::thread::Builder::new()
                .name("pdm-browser-discovery".into())
                .spawn(move || {
                    for request in discovery.incoming_requests() {
                        if request.method() == &Method::Get && request.url() == "/v1/discover" {
                            let paired = discovery_state
                                .status()
                                .map(|status| status.paired)
                                .unwrap_or(false);
                            let body = format!(
                                "{{\"app\":\"pencil-download-manager\",\"port\":{port},\"paired\":{paired}}}"
                            );
                            let _ = request.respond(json_response(200, &body));
                        } else {
                            let _ = request.respond(Response::empty(StatusCode(404)));
                        }
                    }
                })
                .map_err(|e| e.to_string())?;
        }
        Err(error) => {
            if let Ok(mut security) = state.0.lock() {
                security.discovery_error = Some(format!(
                    "Port {DISCOVERY_PORT} is in use by another program ({error}). \
                     Close it and restart Pencil so browsers can find the app."
                ));
            }
        }
    }

    Ok(state)
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// A stored config is only trusted when it is well formed; a hand-edited or
/// truncated file falls back to a fresh token and a new pairing.
fn load_config(path: &Path) -> Option<StoredConfig> {
    let bytes = std::fs::read(path).ok()?;
    let config: StoredConfig = serde_json::from_slice(&bytes).ok()?;
    let token_ok = config.token.len() == 64 && config.token.bytes().all(|b| b.is_ascii_hexdigit());
    let origin_ok = config
        .origin
        .as_deref()
        .map_or(true, valid_extension_origin);
    (token_ok && origin_ok).then_some(config)
}

fn persist(state: &SecurityState) -> Result<(), String> {
    match &state.config_path {
        Some(path) => write_private_config(
            path,
            &StoredConfig {
                port: state.port,
                token: state.token.clone(),
                origin: state.allowed_origin.clone(),
            },
        ),
        None => Ok(()),
    }
}

fn write_private_config(path: &Path, config: &StoredConfig) -> Result<(), String> {
    let bytes = serde_json::to_vec(config).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .map_err(|e| e.to_string())?;
        file.write_all(&bytes).map_err(|e| e.to_string())?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, &bytes).map_err(|e| e.to_string())?;
    }
    #[cfg(windows)]
    {
        let user = std::env::var("USERNAME").map_err(|e| e.to_string())?;
        let domain = std::env::var("USERDOMAIN").unwrap_or_default();
        let account = if domain.is_empty() {
            user
        } else {
            format!("{domain}\\{user}")
        };
        let status = std::process::Command::new("icacls")
            .arg(path)
            .arg("/inheritance:r")
            .arg("/grant:r")
            .arg(format!("{account}:(F)"))
            .arg("/grant:r")
            .arg("SYSTEM:(F)")
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("could not restrict browser token file permissions".into());
        }
    }
    Ok(())
}

fn handle_request(mut req: Request, state: &BrowserState, app: &tauri::AppHandle) {
    let path = req.url().to_string();
    let method = req.method().clone();
    let origin = header(&req, "Origin");
    let auth = header(&req, "Authorization");
    let host = header(&req, "Host");
    let host_ok = state
        .0
        .lock()
        .map(|security| host_allowed(host.as_deref(), security.port))
        .unwrap_or(false);
    if !host_ok {
        let _ = req.respond(error_response(403, "invalid Host header"));
        return;
    }
    if !allow_rate(state) {
        let _ = req.respond(error_response(429, "rate limit exceeded"));
        return;
    }

    if method == Method::Post && path == "/v1/pair" {
        let body = match read_body(&mut req) {
            Ok(body) => body,
            Err((code, message)) => {
                let _ = req.respond(error_response(code, message));
                return;
            }
        };
        let parsed: PairRequest = match serde_json::from_slice(&body) {
            Ok(parsed) => parsed,
            Err(_) => {
                let _ = req.respond(error_response(400, "invalid JSON"));
                return;
            }
        };
        if !valid_extension_origin(&parsed.origin)
            || origin.as_deref() != Some(parsed.origin.as_str())
        {
            let _ = req.respond(error_response(403, "invalid extension origin"));
            return;
        }
        let mut security = match state.0.lock() {
            Ok(security) => security,
            Err(_) => {
                let _ = req.respond(error_response(500, "internal error"));
                return;
            }
        };
        let valid = pairing_code_valid(security.pairing.as_ref(), &parsed.code, Instant::now());
        if !valid {
            security.pairing = None;
            let _ = req.respond(error_response(401, "pairing code expired or already used"));
            return;
        }
        security.pairing = None;
        security.allowed_origin = Some(parsed.origin);
        security.last_seen = Some(now_secs());
        if let Err(message) = persist(&security) {
            security.allowed_origin = None;
            let _ = req.respond(error_response(500, &message));
            return;
        }
        let body = serde_json::to_string(&PairResponse {
            token: security.token.clone(),
            port: security.port,
        })
        .unwrap_or_else(|_| "{}".into());
        let _ = req.respond(json_response(200, &body));
        return;
    }

    {
        let mut security = match state.0.lock() {
            Ok(security) => security,
            Err(_) => {
                let _ = req.respond(error_response(500, "internal error"));
                return;
            }
        };
        if !token_authorized(auth.as_deref(), &security.token) {
            let _ = req.respond(error_response(401, "missing or invalid token"));
            return;
        }
        if !origin_allowed(origin.as_deref(), security.allowed_origin.as_deref()) {
            let _ = req.respond(error_response(403, "unexpected origin"));
            return;
        }
        if security.allowed_origin.is_none() && (path != "/v1/ping" || method != Method::Get) {
            let _ = req.respond(error_response(403, "pair the browser extension first"));
            return;
        }
        security.last_seen = Some(now_secs());
    }

    if method == Method::Get && path == "/v1/ping" {
        let _ = req.respond(json_response(
            200,
            "{\"ok\":true,\"message\":\"connected\"}",
        ));
        return;
    }
    if method != Method::Post || (path != "/v1/download" && path != "/v1/batch") {
        let _ = req.respond(error_response(404, "not found"));
        return;
    }

    let body = match read_body(&mut req) {
        Ok(body) => body,
        Err((code, message)) => {
            let _ = req.respond(error_response(code, message));
            return;
        }
    };
    let items: Vec<DownloadRequest> = if path == "/v1/download" {
        match serde_json::from_slice::<DownloadRequest>(&body) {
            Ok(item) => vec![item],
            Err(_) => {
                let _ = req.respond(error_response(400, "invalid download JSON"));
                return;
            }
        }
    } else {
        match serde_json::from_slice::<Vec<DownloadRequest>>(&body) {
            Ok(items) if !items.is_empty() && items.len() <= MAX_BATCH => items,
            _ => {
                let _ = req.respond(error_response(400, "batch must contain 1 to 20 downloads"));
                return;
            }
        }
    };
    for item in &items {
        if let Err(message) = validate_download(item) {
            let _ = req.respond(error_response(400, &message));
            return;
        }
    }

    let app_state = app.state::<crate::AppState>();
    let result = tauri::async_runtime::block_on(async {
        let rpc = app_state.rpc().await?;
        let mut gids = Vec::with_capacity(items.len());
        for item in &items {
            let headers = item
                .headers
                .clone()
                .unwrap_or_default()
                .into_iter()
                .collect();
            let options = DownloadOptions {
                referer: item.referer.clone(),
                cookies: item.cookies.clone(),
                user_agent: item.user_agent.clone(),
                file_name: item.file_name.clone(),
                headers,
            };
            gids.push(
                rpc.add_uri_with_options(&item.url, &options)
                    .await
                    .map_err(|e| e.to_string())?,
            );
        }
        Ok::<_, String>(gids)
    });
    match result {
        Ok(gids) => {
            let body = serde_json::json!({ "ok": true, "gids": gids }).to_string();
            let _ = req.respond(json_response(200, &body));
        }
        Err(message) => {
            let _ = req.respond(error_response(503, &message));
        }
    }
}

fn allow_rate(state: &BrowserState) -> bool {
    let Ok(mut security) = state.0.lock() else {
        return false;
    };
    let now = Instant::now();
    while security
        .requests
        .front()
        .is_some_and(|time| now.duration_since(*time) > RATE_WINDOW)
    {
        security.requests.pop_front();
    }
    if security.requests.len() >= RATE_LIMIT {
        return false;
    }
    security.requests.push_back(now);
    true
}

fn read_body(req: &mut Request) -> Result<Vec<u8>, (u16, &'static str)> {
    check_body_size(req.body_length(), 0)?;
    let mut body = Vec::new();
    req.as_reader()
        .take((MAX_BODY + 1) as u64)
        .read_to_end(&mut body)
        .map_err(|_| (400, "could not read request body"))?;
    check_body_size(None, body.len())?;
    Ok(body)
}

pub fn validate_download(item: &DownloadRequest) -> Result<(), String> {
    let url = Url::parse(item.url.trim()).map_err(|_| "invalid URL".to_string())?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("only HTTP and HTTPS URLs are allowed".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("URLs containing embedded credentials are not allowed".into());
    }
    if item.url.len() > 8192 {
        return Err("URL is too long".into());
    }
    if let Some(referer) = &item.referer {
        validate_optional_header(referer, 8192, "referer")?;
        if !referer.is_empty() {
            let url = Url::parse(referer).map_err(|_| "invalid referer".to_string())?;
            if !matches!(url.scheme(), "http" | "https")
                || !url.username().is_empty()
                || url.password().is_some()
            {
                return Err("referer must be an HTTP or HTTPS URL without credentials".into());
            }
        }
    }
    if let Some(cookies) = &item.cookies {
        validate_optional_header(cookies, 16384, "cookies")?;
    }
    if let Some(user_agent) = &item.user_agent {
        validate_optional_header(user_agent, 1024, "user-agent")?;
    }
    if let Some(file_name) = &item.file_name {
        if !safe_download_name(file_name) {
            return Err("invalid file name".into());
        }
    }
    if let Some(headers) = &item.headers {
        if headers.len() > 50 {
            return Err("too many headers".into());
        }
        for (name, value) in headers {
            if name.is_empty()
                || name.len() > 128
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
                || matches!(
                    name.to_ascii_lowercase().as_str(),
                    "host"
                        | "content-length"
                        | "connection"
                        | "transfer-encoding"
                        | "proxy-authorization"
                        | "proxy-connection"
                        | "keep-alive"
                        | "upgrade"
                )
            {
                return Err("invalid or forbidden header name".into());
            }
            validate_optional_header(value, 8192, "header")?;
        }
    }
    Ok(())
}

fn safe_download_name(value: &str) -> bool {
    if value.is_empty()
        || value.len() > 240
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
        || value.contains(':')
        || value.chars().any(|c| {
            c.is_control()
                || matches!(
                    c,
                    '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
                )
        })
        || value.ends_with('.')
        || value.ends_with(' ')
    {
        return false;
    }
    let stem = value.split('.').next().unwrap_or("").to_ascii_uppercase();
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
}

fn validate_optional_header(value: &str, max: usize, name: &str) -> Result<(), String> {
    if value.len() > max || value.contains('\r') || value.contains('\n') || value.contains('\0') {
        return Err(format!("invalid {name}"));
    }
    Ok(())
}

fn check_body_size(declared: Option<usize>, actual: usize) -> Result<(), (u16, &'static str)> {
    if declared.is_some_and(|size| size > MAX_BODY) || actual > MAX_BODY {
        Err((413, "request body too large"))
    } else {
        Ok(())
    }
}

fn pairing_code_valid(pairing: Option<&Pairing>, code: &str, now: Instant) -> bool {
    pairing.is_some_and(|pairing| pairing.expires > now && constant_time_eq(&pairing.code, code))
}

fn token_authorized(header: Option<&str>, token: &str) -> bool {
    header
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|candidate| constant_time_eq(candidate, token))
}

fn host_allowed(host: Option<&str>, port: u16) -> bool {
    host.is_some_and(|host| {
        host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}")
    })
}

fn origin_allowed(origin: Option<&str>, paired: Option<&str>) -> bool {
    origin.is_some_and(|origin| paired == Some(origin))
}

fn valid_extension_origin(origin: &str) -> bool {
    let Some((scheme, host)) = origin.split_once("://") else {
        return false;
    };
    matches!(scheme, "chrome-extension" | "moz-extension")
        && !host.is_empty()
        && host.len() <= 220
        && host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn header(req: &Request, name: &'static str) -> Option<String> {
    req.headers()
        .iter()
        .find(|header| header.field.equiv(name))
        .map(|header| header.value.as_str().to_string())
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

fn json_response(status: u16, body: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string(body.to_string())
        .with_status_code(StatusCode(status))
        .with_header(Header::from_bytes("Content-Type", "application/json; charset=utf-8").unwrap())
}

fn error_response(status: u16, message: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    json_response(
        status,
        &serde_json::json!({ "ok": false, "message": message }).to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(url: &str) -> DownloadRequest {
        DownloadRequest {
            url: url.into(),
            referer: None,
            cookies: None,
            user_agent: None,
            file_name: None,
            headers: None,
        }
    }

    #[test]
    fn validates_network_urls_and_rejects_file_urls() {
        assert!(validate_download(&request("https://example.test/file.zip")).is_ok());
        assert!(validate_download(&request("file:///etc/passwd")).is_err());
        assert!(validate_download(&request("javascript:alert(1)")).is_err());
    }

    #[test]
    fn rejects_injected_fields_and_forbidden_headers() {
        let mut item = request("https://example.test/file");
        item.cookies = Some("x\r\nInjected: yes".into());
        assert!(validate_download(&item).is_err());
        item.cookies = None;
        item.file_name = Some("../evil".into());
        assert!(validate_download(&item).is_err());
        item.file_name = None;
        for name in ["CON.txt", "file.", "file ", "bad\u{202e}exe", "C:evil.txt"] {
            item.file_name = Some(name.into());
            assert!(
                validate_download(&item).is_err(),
                "accepted unsafe name {name:?}"
            );
        }
        item.file_name = None;
        item.headers = Some(std::collections::HashMap::from([(
            "Host".into(),
            "attacker.test".into(),
        )]));
        assert!(validate_download(&item).is_err());
    }

    #[test]
    fn host_validation_rejects_missing_forged_and_wrong_port_hosts() {
        assert!(host_allowed(Some("127.0.0.1:1234"), 1234));
        assert!(host_allowed(Some("localhost:1234"), 1234));
        assert!(!host_allowed(None, 1234));
        assert!(!host_allowed(Some("evil.test:1234"), 1234));
        assert!(!host_allowed(Some("127.0.0.1:9999"), 1234));
        assert!(!host_allowed(Some("127.0.0.1"), 1234));
    }

    #[test]
    fn token_and_origin_checks_reject_untrusted_requests() {
        assert!(token_authorized(Some("Bearer secret"), "secret"));
        assert!(!token_authorized(Some("Bearer wrong"), "secret"));
        assert!(!token_authorized(None, "secret"));
        assert!(valid_extension_origin(
            "chrome-extension://abcdefghijklmnop"
        ));
        assert!(valid_extension_origin("moz-extension://abcdefghijklmnop"));
        assert!(!valid_extension_origin("https://example.test"));
        assert!(origin_allowed(
            Some("chrome-extension://abcdefghijklmnop"),
            Some("chrome-extension://abcdefghijklmnop")
        ));
        assert!(!origin_allowed(
            Some("https://evil.test"),
            Some("chrome-extension://abcdefghijklmnop")
        ));
        assert!(!origin_allowed(Some("chrome-extension://evil"), None));
        assert!(!origin_allowed(
            None,
            Some("chrome-extension://abcdefghijklmnop")
        ));
        assert!(!origin_allowed(None, None));
    }

    #[test]
    fn rejects_oversized_body_before_parsing() {
        assert_eq!(
            check_body_size(Some(MAX_BODY + 1), 0),
            Err((413, "request body too large"))
        );
        assert_eq!(
            check_body_size(None, MAX_BODY + 1),
            Err((413, "request body too large"))
        );
        assert!(check_body_size(Some(10), 10).is_ok());
    }

    #[test]
    fn pairing_code_expires_and_is_one_use_by_state_transition() {
        let state = BrowserState::new(1234, "token".into());
        let code = state.create_pairing_code().unwrap();
        let mut security = state.0.lock().unwrap();
        let pairing = security.pairing.as_ref().unwrap();
        assert!(pairing_code_valid(Some(pairing), &code, Instant::now()));
        assert!(!pairing_code_valid(
            Some(pairing),
            &code,
            Instant::now() + Duration::from_secs(301)
        ));
        security.pairing = None;
        assert!(!pairing_code_valid(
            security.pairing.as_ref(),
            &code,
            Instant::now()
        ));
    }

    #[test]
    fn stored_config_round_trips_and_rejects_damaged_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("browser-connection.json");
        let token = "a".repeat(64);
        write_private_config(
            &path,
            &StoredConfig {
                port: 4242,
                token: token.clone(),
                origin: Some("chrome-extension://abcdefghijklmnopabcdefghijklmnop".into()),
            },
        )
        .unwrap();
        let loaded = load_config(&path).expect("valid config loads");
        assert_eq!(loaded.token, token);
        assert_eq!(loaded.port, 4242);
        assert!(loaded.origin.is_some());

        std::fs::write(&path, b"{\"port\":1,\"token\":\"short\"}").unwrap();
        assert!(load_config(&path).is_none());
        std::fs::write(
            &path,
            format!("{{\"port\":1,\"token\":\"{token}\",\"origin\":\"https://evil.test\"}}"),
        )
        .unwrap();
        assert!(load_config(&path).is_none());
        std::fs::write(&path, b"not json").unwrap();
        assert!(load_config(&path).is_none());
    }

    #[test]
    fn disconnect_rotates_token_and_clears_pairing() {
        let dir = tempfile::tempdir().unwrap();
        let state = BrowserState::new(1234, "a".repeat(64));
        {
            let mut security = state.0.lock().unwrap();
            security.config_path = Some(dir.path().join("browser-connection.json"));
            security.allowed_origin = Some("chrome-extension://abc".into());
        }
        assert!(state.status().unwrap().paired);
        state.disconnect().unwrap();
        let status = state.status().unwrap();
        assert!(!status.paired);
        let security = state.0.lock().unwrap();
        assert_ne!(security.token, "a".repeat(64));
        let saved = load_config(security.config_path.as_ref().unwrap()).unwrap();
        assert_eq!(saved.token, security.token);
        assert!(saved.origin.is_none());
    }
}
