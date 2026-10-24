use crate::error::{EngineError, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::Path;
use std::time::Duration;

const STATUS_KEYS: [&str; 7] = [
    "gid",
    "status",
    "totalLength",
    "completedLength",
    "downloadSpeed",
    "errorMessage",
    "files",
];

/// Request-specific HTTP metadata forwarded to aria2 for a single download.
#[derive(Debug, Clone, Default)]
pub struct DownloadOptions {
    pub referer: Option<String>,
    pub cookies: Option<String>,
    pub user_agent: Option<String>,
    pub file_name: Option<String>,
    pub headers: Vec<(String, String)>,
}

/// One download as shown in the UI.
#[derive(Debug, Clone, Serialize)]
pub struct DownloadInfo {
    pub gid: String,
    pub status: String,
    pub name: String,
    /// Full path of the first file on disk (empty until the engine knows it).
    pub path: String,
    pub total: u64,
    pub completed: u64,
    pub speed: u64,
    pub error: Option<String>,
}

/// Minimal JSON-RPC client for the local engine.
#[derive(Clone)]
pub struct RpcClient {
    http: reqwest::Client,
    url: String,
    secret: String,
}

impl RpcClient {
    pub fn new(port: u16, secret: String) -> Result<Self> {
        let http = reqwest::Client::builder()
            .no_proxy() // never route loopback RPC through a system proxy
            .timeout(Duration::from_secs(10))
            .build()?;
        Ok(Self {
            http,
            url: format!("http://127.0.0.1:{port}/jsonrpc"),
            secret,
        })
    }

    pub async fn call(&self, method: &str, params: Vec<Value>) -> Result<Value> {
        let mut all = Vec::with_capacity(params.len() + 1);
        all.push(json!(format!("token:{}", self.secret)));
        all.extend(params);
        let body = json!({
            "jsonrpc": "2.0",
            "id": "pdm",
            "method": format!("aria2.{method}"),
            "params": all,
        });
        let resp: Value = self
            .http
            .post(&self.url)
            .json(&body)
            .send()
            .await?
            .json()
            .await?;
        if let Some(err) = resp.get("error") {
            let msg = err
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown error");
            return Err(EngineError::Rpc(msg.to_string()));
        }
        Ok(resp.get("result").cloned().unwrap_or(Value::Null))
    }

    pub async fn version(&self) -> Result<String> {
        let v = self.call("getVersion", vec![]).await?;
        Ok(v.get("version")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string())
    }

    /// Queue a download. Only plain network URLs are accepted.
    pub async fn add_uri(&self, uri: &str) -> Result<String> {
        self.add_uri_with_options(uri, &DownloadOptions::default()).await
    }

    /// Queue a download with request metadata that applies only to this URI.
    pub async fn add_uri_with_options(
        &self,
        uri: &str,
        options: &DownloadOptions,
    ) -> Result<String> {
        let parsed = reqwest::Url::parse(uri.trim())
            .map_err(|_| EngineError::Rpc("that does not look like a valid link".into()))?;
        if !matches!(parsed.scheme(), "http" | "https" | "ftp" | "sftp") {
            return Err(EngineError::Rpc(format!(
                "unsupported link type: {}",
                parsed.scheme()
            )));
        }
        let aria_options = build_aria_options(options)?;
        let mut params = vec![json!([parsed.as_str()])];
        if !aria_options.is_empty() {
            params.push(Value::Object(aria_options));
        }
        let gid = self.call("addUri", params).await?;
        Ok(gid.as_str().unwrap_or("").to_string())
    }

    pub async fn pause(&self, gid: &str) -> Result<()> {
        self.call("pause", vec![json!(gid)]).await.map(|_| ())
    }

    pub async fn resume(&self, gid: &str) -> Result<()> {
        self.call("unpause", vec![json!(gid)]).await.map(|_| ())
    }

    /// Remove a download. Finished/failed entries are purged from the result list.
    pub async fn remove(&self, gid: &str) -> Result<()> {
        if self.call("remove", vec![json!(gid)]).await.is_err() {
            self.call("removeDownloadResult", vec![json!(gid)]).await?;
        }
        Ok(())
    }

    pub async fn list(&self) -> Result<Vec<DownloadInfo>> {
        let keys = json!(STATUS_KEYS);
        let (active, waiting, stopped) = tokio::join!(
            self.call("tellActive", vec![keys.clone()]),
            self.call("tellWaiting", vec![json!(0), json!(1000), keys.clone()]),
            self.call("tellStopped", vec![json!(0), json!(1000), keys.clone()]),
        );
        let mut out = Vec::new();
        for part in [active?, waiting?, stopped?] {
            if let Some(items) = part.as_array() {
                out.extend(items.iter().map(parse_info));
            }
        }
        Ok(out)
    }

    pub async fn shutdown(&self) {
        let _ = self.call("shutdown", vec![]).await;
    }
}

fn build_aria_options(options: &DownloadOptions) -> Result<serde_json::Map<String, Value>> {
    let mut aria_options = serde_json::Map::new();
    if let Some(value) = options.referer.as_deref().filter(|s| !s.is_empty()) {
        if value.contains('\r') || value.contains('\n') {
            return Err(EngineError::Rpc("invalid referer".into()));
        }
        aria_options.insert("referer".into(), json!(value));
    }
    if let Some(value) = options.user_agent.as_deref().filter(|s| !s.is_empty()) {
        if value.contains('\r') || value.contains('\n') {
            return Err(EngineError::Rpc("invalid user-agent".into()));
        }
        aria_options.insert("user-agent".into(), json!(value));
    }
    if let Some(value) = options.file_name.as_deref().filter(|s| !s.is_empty()) {
        if value.len() > 255
            || value == "."
            || value == ".."
            || value.contains('/')
            || value.contains('\\')
            || value.chars().any(char::is_control)
        {
            return Err(EngineError::Rpc("invalid output file name".into()));
        }
        aria_options.insert("out".into(), json!(value));
    }
    let mut headers = options.headers.clone();
    if let Some(cookie) = options.cookies.as_deref().filter(|s| !s.is_empty()) {
        headers.push(("Cookie".into(), cookie.into()));
    }
    let mut rendered_headers = Vec::with_capacity(headers.len());
    for (name, value) in headers {
        if name.is_empty()
            || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
            || value.contains('\r')
            || value.contains('\n')
            || value.contains('\0')
        {
            return Err(EngineError::Rpc("invalid download header".into()));
        }
        rendered_headers.push(json!(format!("{name}: {value}")));
    }
    if !rendered_headers.is_empty() {
        aria_options.insert("header".into(), Value::Array(rendered_headers));
    }
    Ok(aria_options)
}

fn parse_info(v: &Value) -> DownloadInfo {
    let text = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("");
    let num = |k: &str| text(k).parse::<u64>().unwrap_or(0);

    let first_file = v.get("files").and_then(|f| f.get(0));
    let path = first_file
        .and_then(|f| f.get("path"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let mut name = Some(path.as_str())
        .and_then(|p| Path::new(p).file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if name.is_empty() {
        name = first_file
            .and_then(|f| f.get("uris"))
            .and_then(|u| u.get(0))
            .and_then(|u| u.get("uri"))
            .and_then(Value::as_str)
            .unwrap_or("Starting…")
            .to_string();
    }
    let error = Some(text("errorMessage").to_string()).filter(|s| !s.is_empty());

    DownloadInfo {
        gid: text("gid").to_string(),
        status: text("status").to_string(),
        name,
        path,
        total: num("totalLength"),
        completed: num("completedLength"),
        speed: num("downloadSpeed"),
        error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_status_payload() {
        let v = json!({
            "gid": "abc", "status": "active", "totalLength": "1000",
            "completedLength": "250", "downloadSpeed": "50", "errorMessage": "",
            "files": [{"path": "/tmp/dl/movie.mkv", "uris": []}]
        });
        let i = parse_info(&v);
        assert_eq!((i.gid.as_str(), i.name.as_str()), ("abc", "movie.mkv"));
        assert_eq!((i.total, i.completed, i.speed), (1000, 250, 50));
        assert!(i.error.is_none());
        assert_eq!(i.path, "/tmp/dl/movie.mkv");
    }

    #[test]
    fn falls_back_to_uri_when_path_is_empty() {
        let v = json!({"gid":"x","status":"waiting","files":[{"path":"","uris":[{"uri":"https://a/b.zip"}]}]});
        let info = parse_info(&v);
        assert_eq!(info.name, "https://a/b.zip");
        assert_eq!(info.path, "");
    }

    #[test]
    fn builds_per_download_options_and_rejects_header_injection() {
        let options = DownloadOptions {
            referer: Some("https://example.test/page".into()),
            cookies: Some("session=abc".into()),
            user_agent: Some("PDM test".into()),
            file_name: Some("example.zip".into()),
            headers: vec![("X-Test".into(), "ok".into())],
        };
        let built = build_aria_options(&options).unwrap();
        assert_eq!(built["referer"], "https://example.test/page");
        assert_eq!(built["user-agent"], "PDM test");
        assert_eq!(built["out"], "example.zip");
        let headers = built["header"].as_array().unwrap();
        assert!(headers.contains(&json!("X-Test: ok")));
        assert!(headers.contains(&json!("Cookie: session=abc")));
        let bad = DownloadOptions {
            headers: vec![("X-Test".into(), "ok\r\nX-Evil: yes".into())],
            ..DownloadOptions::default()
        };
        assert!(build_aria_options(&bad).is_err());
    }

    #[tokio::test]
    async fn rejects_non_network_links() {
        let c = RpcClient::new(1, "s".into()).unwrap();
        assert!(c.add_uri("file:///etc/passwd").await.is_err());
        assert!(c.add_uri("not a url").await.is_err());
    }
}
