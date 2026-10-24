use crate::error::{EngineError, Result};
use crate::random_token;
use crate::rpc::RpcClient;
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::{Child, Command};

pub struct LaunchOptions {
    /// Path to the engine executable.
    pub bin: PathBuf,
    /// Default folder for finished downloads.
    pub download_dir: PathBuf,
    /// File used to persist the queue across restarts.
    pub session_file: PathBuf,
}

/// A running engine process plus an RPC client that is already connected to it.
pub struct EngineProcess {
    child: Child,
    pub rpc: RpcClient,
}

impl EngineProcess {
    pub async fn spawn(opts: LaunchOptions) -> Result<Self> {
        // Loopback only, random port, random secret per launch.
        let port = {
            let l = TcpListener::bind("127.0.0.1:0")?;
            l.local_addr()?.port()
        };
        let secret = random_token(32);

        if let Some(parent) = opts.session_file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if !opts.session_file.exists() {
            std::fs::write(&opts.session_file, b"")?;
        }
        std::fs::create_dir_all(&opts.download_dir)?;

        let mut cmd = Command::new(&opts.bin);
        cmd.arg("--enable-rpc")
            .arg(format!("--rpc-listen-port={port}"))
            .arg(format!("--rpc-secret={secret}"))
            .arg("--rpc-listen-all=false")
            .arg("--no-conf=true")
            .arg(format!("--dir={}", opts.download_dir.display()))
            .arg(format!("--input-file={}", opts.session_file.display()))
            .arg(format!("--save-session={}", opts.session_file.display()))
            .arg("--save-session-interval=30")
            .arg("--continue=true")
            .arg("--split=16")
            .arg("--max-connection-per-server=16")
            .arg("--min-split-size=1M")
            .arg("--max-concurrent-downloads=5")
            .arg("--console-log-level=warn")
            .arg("--summary-interval=0")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);

        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW

        let mut child = cmd.spawn().map_err(|e| EngineError::Start(e.to_string()))?;
        let rpc = RpcClient::new(port, secret)?;

        // Wait until the RPC endpoint answers.
        for _ in 0..80 {
            if let Some(status) = child.try_wait()? {
                return Err(EngineError::Start(format!("exited early ({status})")));
            }
            if rpc.version().await.is_ok() {
                return Ok(Self { child, rpc });
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let _ = child.start_kill();
        Err(EngineError::Start(
            "timed out waiting for the engine".into(),
        ))
    }

    /// Ask the engine to stop (it saves the session), then make sure it is gone.
    pub async fn shutdown(mut self) {
        self.rpc.shutdown().await;
        if tokio::time::timeout(Duration::from_secs(3), self.child.wait())
            .await
            .is_err()
        {
            let _ = self.child.kill().await;
        }
    }
}
