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
        let parsed = reqwest::Url::parse(uri.trim())
            .map_err(|_| EngineError::Rpc("that does not look like a valid link".into()))?;
        if !matches!(parsed.scheme(), "http" | "https" | "ftp" | "sftp") {
            return Err(EngineError::Rpc(format!(
                "unsupported link type: {}",
                parsed.scheme()
            )));
        }
        let gid = self.call("addUri", vec![json!([parsed.as_str()])]).await?;
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

    #[tokio::test]
    async fn rejects_non_network_links() {
        let c = RpcClient::new(1, "s".into()).unwrap();
        assert!(c.add_uri("file:///etc/passwd").await.is_err());
        assert!(c.add_uri("not a url").await.is_err());
    }
}
