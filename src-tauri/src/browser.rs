use pdm_engine::DownloadOptions;
use rand::{distributions::Alphanumeric, Rng, RngCore};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    io::Read,
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
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
        })))
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
    let mut raw = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut raw);
    let token = hex::encode(raw);

    let main_server = Server::http(("127.0.0.1", 0)).map_err(|e| e.to_string())?;
    let port = main_server
        .server_addr()
        .to_ip()
        .ok_or_else(|| "browser server did not bind an IP socket".to_string())?
        .port();
    let state = BrowserState::new(port, token.clone());
    write_private_config(&data_dir.join("browser-connection.json"), port, &token)?;

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
    let discovery = Server::http(("127.0.0.1", DISCOVERY_PORT)).map_err(|e| {
        format!("browser discovery port {DISCOVERY_PORT} is unavailable: {e}")
    })?;
    std::thread::Builder::new()
        .name("pdm-browser-discovery".into())
        .spawn(move || {
            for request in discovery.incoming_requests() {
                if request.method() == &Method::Get && request.url() == "/v1/discover" {
                    let body = format!("{{\"port\":{port}}}");
                    let _ = request.respond(json_response(200, &body));
                } else {
                    let _ = request.respond(Response::empty(StatusCode(404)));
                }
            }
        })
        .map_err(|e| e.to_string())?;

    Ok(state)
}

fn write_private_config(path: &Path, port: u16, token: &str) -> Result<(), String> {
    let bytes = serde_json::to_vec(&serde_json::json!({
        "port": port,
        "token": token
    }))
    .map_err(|e| e.to_string())?;
    std::fs::write(path, bytes).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
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
        let body = serde_json::to_string(&PairResponse {
            token: security.token.clone(),
            port: security.port,
        })
        .unwrap_or_else(|_| "{}".into());
        let _ = req.respond(json_response(200, &body));
        return;
    }

    {
        let security = match state.0.lock() {
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
    }

    if method == Method::Get && path == "/v1/ping" {
        let _ = req.respond(json_response(200, "{\"ok\":true,\"message\":\"connected\"}"));
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
        if file_name.len() > 255
            || file_name.is_empty()
            || file_name == "."
            || file_name == ".."
            || file_name.contains('/')
            || file_name.contains('\\')
            || file_name.chars().any(char::is_control)
        {
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

fn validate_optional_header(value: &str, max: usize, name: &str) -> Result<(), String> {
    if value.len() > max
        || value.contains('\r')
        || value.contains('\n')
        || value.contains('\0')
    {
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
    pairing.is_some_and(|pairing| {
        pairing.expires > now && constant_time_eq(&pairing.code, code)
    })
}

fn token_authorized(header: Option<&str>, token: &str) -> bool {
    header
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|candidate| constant_time_eq(candidate, token))
}

fn origin_allowed(origin: Option<&str>, paired: Option<&str>) -> bool {
    origin.is_none_or(|origin| paired == Some(origin))
}

fn valid_extension_origin(origin: &str) -> bool {
    let Some((scheme, host)) = origin.split_once("://") else {
        return false;
    };
    matches!(scheme, "chrome-extension" | "moz-extension")
        && !host.is_empty()
        && host.len() <= 220
        && host.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn header(req: &Request, name: &str) -> Option<String> {
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
        .with_header(
            Header::from_bytes("Content-Type", "application/json; charset=utf-8").unwrap(),
        )
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
        item.headers = Some(std::collections::HashMap::from([(
            "Host".into(),
            "attacker.test".into(),
        )]));
        assert!(validate_download(&item).is_err());
    }

    #[test]
    fn token_and_origin_checks_reject_untrusted_requests() {
        assert!(token_authorized(Some("Bearer secret"), "secret"));
        assert!(!token_authorized(Some("Bearer wrong"), "secret"));
        assert!(!token_authorized(None, "secret"));
        assert!(valid_extension_origin("chrome-extension://abcdefghijklmnop"));
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
        assert!(!pairing_code_valid(security.pairing.as_ref(), &code, Instant::now()));
    }
}
