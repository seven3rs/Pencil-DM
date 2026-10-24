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
    /// Embedded manifest hash for managed binaries; checked immediately before spawn.
    pub expected_sha256: Option<String>,
}

/// A running engine process plus an RPC client that is already connected to it.
pub struct EngineProcess {
    child: Child,
    pub rpc: RpcClient,
}

impl EngineProcess {
    pub async fn spawn(opts: LaunchOptions) -> Result<Self> {
        // Recheck immediately before execution. This narrows the verification-to-use
        // window; it cannot eliminate replacement by a same-user attacker between
        // this check and OS process creation.
        if let Some(expected) = &opts.expected_sha256 {
            let actual = crate::install::sha256_file(&opts.bin)?;
            if !actual.eq_ignore_ascii_case(expected) {
                return Err(EngineError::HashMismatch {
                    what: "engine binary before launch".into(),
                    expected: expected.clone(),
                    actual,
                });
            }
        }
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

        // Keep the RPC bearer secret out of process arguments (visible in process
        // listings). aria2 reads it from a short-lived, owner-only config file.
        let rpc_config = opts
            .session_file
            .with_file_name(format!(".rpc-{}.conf", random_token(12)));
        let config_contents = format!("rpc-secret={secret}\n");
        #[cfg(unix)]
        {
            use std::io::Write as _;
            use std::os::unix::fs::OpenOptionsExt;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&rpc_config)?;
            if let Err(error) = file.write_all(config_contents.as_bytes()) {
                let _ = std::fs::remove_file(&rpc_config);
                return Err(EngineError::Io(error));
            }
        }
        #[cfg(not(unix))]
        {
            if let Err(error) = std::fs::write(&rpc_config, config_contents) {
                let _ = std::fs::remove_file(&rpc_config);
                return Err(EngineError::Io(error));
            }
        }
        #[cfg(windows)]
        if let Err(error) = restrict_file_to_current_user(&rpc_config) {
            let _ = std::fs::remove_file(&rpc_config);
            return Err(error);
        }

        let mut cmd = Command::new(&opts.bin);
        cmd.arg("--enable-rpc")
            .arg(format!("--rpc-listen-port={port}"))
            .arg(format!("--conf-path={}", rpc_config.display()))
            .arg("--rpc-listen-all=false")
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

        // Hash again at the last practical point before process creation. A
        // path-based spawn still cannot close the same-user race completely.
        if let Some(expected) = &opts.expected_sha256 {
            let actual = match crate::install::sha256_file(&opts.bin) {
                Ok(actual) => actual,
                Err(error) => {
                    let _ = std::fs::remove_file(&rpc_config);
                    return Err(error);
                }
            };
            if !actual.eq_ignore_ascii_case(expected) {
                let _ = std::fs::remove_file(&rpc_config);
                return Err(EngineError::HashMismatch {
                    what: "engine binary immediately before launch".into(),
                    expected: expected.clone(),
                    actual,
                });
            }
        }

        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(error) => {
                let _ = std::fs::remove_file(&rpc_config);
                return Err(EngineError::Start(error.to_string()));
            }
        };
        let rpc = match RpcClient::new(port, secret) {
            Ok(rpc) => rpc,
            Err(error) => {
                let _ = child.start_kill();
                let _ = std::fs::remove_file(&rpc_config);
                return Err(error);
            }
        };

        // Wait until the RPC endpoint answers.
        for _ in 0..80 {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let _ = std::fs::remove_file(&rpc_config);
                    return Err(EngineError::Start(format!("exited early ({status})")));
                }
                Ok(None) => {}
                Err(error) => {
                    let _ = child.start_kill();
                    let _ = std::fs::remove_file(&rpc_config);
                    return Err(EngineError::Io(error));
                }
            }
            if rpc.version().await.is_ok() {
                let _ = std::fs::remove_file(&rpc_config);
                return Ok(Self { child, rpc });
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let _ = child.start_kill();
        let _ = std::fs::remove_file(&rpc_config);
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

#[cfg(windows)]
fn restrict_file_to_current_user(path: &std::path::Path) -> Result<()> {
    let user = std::env::var("USERNAME").map_err(|e| EngineError::Start(e.to_string()))?;
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
        .map_err(|e| EngineError::Start(e.to_string()))?;
    if status.success() {
        Ok(())
    } else {
        Err(EngineError::Start(
            "could not restrict RPC config permissions".into(),
        ))
    }
}
