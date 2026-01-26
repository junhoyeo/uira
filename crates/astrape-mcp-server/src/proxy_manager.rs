use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::process::{Child, Command};
use tokio::sync::RwLock;

pub const DEFAULT_PROXY_PORT: u16 = 8787;

pub struct ProxyManager {
    child: RwLock<Option<Child>>,
    port: u16,
    we_started_it: AtomicBool,
    binary_path: PathBuf,
}

impl ProxyManager {
    pub fn new(port: u16) -> Self {
        let binary_path = Self::find_proxy_binary();
        Self {
            child: RwLock::new(None),
            port,
            we_started_it: AtomicBool::new(false),
            binary_path,
        }
    }

    fn find_proxy_binary() -> PathBuf {
        let candidates = [
            std::env::var("CLAUDE_PLUGIN_ROOT")
                .map(|p| PathBuf::from(p).join("../../target/release/astrape-proxy"))
                .ok(),
            Some(PathBuf::from("target/release/astrape-proxy")),
            Some(PathBuf::from("target/debug/astrape-proxy")),
            which::which("astrape-proxy").ok(),
        ];

        for candidate in candidates.into_iter().flatten() {
            if candidate.exists() {
                return candidate;
            }
        }

        PathBuf::from("astrape-proxy")
    }

    #[allow(dead_code)]
    pub async fn health_check(&self) -> bool {
        let url = format!("http://localhost:{}/health", self.port);

        matches!(
            tokio::time::timeout(Duration::from_secs(2), async {
                let client = reqwest::Client::builder()
                    .timeout(Duration::from_secs(2))
                    .build()
                    .ok()?;

                client
                    .get(&url)
                    .send()
                    .await
                    .ok()?
                    .status()
                    .is_success()
                    .then_some(())
            })
            .await,
            Ok(Some(()))
        )
    }

    async fn is_port_in_use(&self) -> bool {
        use tokio::net::TcpStream;
        let addr = format!("127.0.0.1:{}", self.port);
        TcpStream::connect(&addr).await.is_ok()
    }

    pub async fn ensure_running(&self) -> Result<bool, String> {
        if self.is_port_in_use().await {
            tracing::info!(port = self.port, "Proxy already running on port");
            return Ok(true);
        }

        {
            let mut child_guard = self.child.write().await;
            if let Some(ref mut child) = *child_guard {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        tracing::warn!(status = ?status, "Proxy process exited, will restart");
                        *child_guard = None;
                    }
                    Ok(None) => {
                        drop(child_guard);
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        if self.is_port_in_use().await {
                            return Ok(true);
                        }
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to check proxy process status");
                        *child_guard = None;
                    }
                }
            }
        }

        self.start().await
    }

    async fn start(&self) -> Result<bool, String> {
        tracing::info!(
            binary = %self.binary_path.display(),
            port = self.port,
            "Starting astrape-proxy"
        );

        if !self.binary_path.exists() && which::which(&self.binary_path).is_err() {
            let msg = format!(
                "astrape-proxy binary not found at {} and not in PATH. \
                 Run 'cargo build --release -p astrape-proxy' to build it.",
                self.binary_path.display()
            );
            tracing::error!("{}", msg);
            return Err(msg);
        }

        let mut cmd = Command::new(&self.binary_path);
        cmd.env("PORT", self.port.to_string())
            .env("RUST_LOG", "info")
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let child = cmd.spawn().map_err(|e| {
            format!(
                "Failed to spawn astrape-proxy: {}. Binary: {}",
                e,
                self.binary_path.display()
            )
        })?;

        let pid = child.id();
        tracing::info!(pid = ?pid, port = self.port, "Spawned astrape-proxy");

        *self.child.write().await = Some(child);
        self.we_started_it.store(true, Ordering::SeqCst);

        for i in 0..20 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if self.is_port_in_use().await {
                tracing::info!(attempt = i + 1, "Proxy is now accepting connections");
                return Ok(true);
            }
        }

        let mut child_guard = self.child.write().await;
        if let Some(ref mut child) = *child_guard {
            if let Ok(Some(status)) = child.try_wait() {
                let stderr = if let Some(ref mut stderr) = child.stderr {
                    use tokio::io::AsyncReadExt;
                    let mut buf = String::new();
                    let _ = stderr.read_to_string(&mut buf).await;
                    buf
                } else {
                    String::new()
                };

                *child_guard = None;
                return Err(format!(
                    "Proxy exited during startup with status {:?}. stderr: {}",
                    status, stderr
                ));
            }
        }

        let pid = child_guard.as_ref().and_then(|c| c.id());
        drop(child_guard);

        Err(format!(
            "Proxy started (pid={:?}) but not responding on port {} after 2s timeout",
            pid, self.port
        ))
    }

    #[allow(dead_code)]
    pub async fn stop(&self) {
        if !self.we_started_it.load(Ordering::SeqCst) {
            tracing::debug!("Proxy was not started by us, not stopping");
            return;
        }

        let mut child_guard = self.child.write().await;
        if let Some(mut child) = child_guard.take() {
            tracing::info!("Stopping astrape-proxy");
            if let Err(e) = child.kill().await {
                tracing::warn!(error = %e, "Failed to kill proxy process");
            } else {
                tracing::info!("Proxy stopped");
            }
        }
    }

    #[allow(dead_code)]
    pub fn get_agent_url(&self, agent: &str) -> String {
        format!("http://localhost:{}/agent/{}", self.port, agent)
    }

    #[allow(dead_code)]
    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for ProxyManager {
    fn drop(&mut self) {
        if self.we_started_it.load(Ordering::SeqCst) {
            tracing::debug!("ProxyManager dropping, proxy will be killed");
        }
    }
}
