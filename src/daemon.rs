use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

/// Handle to a running MCCP daemon (HTTP + WS server).
pub struct DaemonHandle {
    shutdown_tx: tokio::sync::oneshot::Sender<()>,
    join: tokio::task::JoinHandle<()>,
}

/// Global daemon state — at most one running at a time.
static DAEMON: std::sync::OnceLock<Arc<Mutex<Option<DaemonHandle>>>> = std::sync::OnceLock::new();

fn daemon_slot() -> &'static Arc<Mutex<Option<DaemonHandle>>> {
    DAEMON.get_or_init(|| Arc::new(Mutex::new(None)))
}

/// Start the daemon (HTTP + WS).  Returns immediately; server runs in background.
pub async fn start(host: &str, port: u16) -> anyhow::Result<()> {
    let slot = daemon_slot();
    let mut guard = slot.lock().await;

    if guard.is_some() {
        anyhow::bail!("daemon already running — stop it first");
    }

    let config = mccp_core::Config::load_or_default()?;
    let state = mccp_server::AppState::init(config).await?;

    let addr = format!("{host}:{port}");
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let bind_addr = addr.clone();
    let join = tokio::spawn(async move {
        if let Err(e) = mccp_server::run_http_with_shutdown(state, &bind_addr, shutdown_rx).await {
            tracing::error!("daemon error: {e}");
        }
        info!("daemon stopped");
    });

    *guard = Some(DaemonHandle { shutdown_tx, join });
    Ok(())
}

/// Stop the running daemon.
pub async fn stop() -> anyhow::Result<()> {
    let slot = daemon_slot();
    let mut guard = slot.lock().await;

    match guard.take() {
        Some(h) => {
            let _ = h.shutdown_tx.send(());
            let _ = h.join.await;
            info!("daemon stopped");
            Ok(())
        }
        None => {
            anyhow::bail!("daemon is not running");
        }
    }
}

/// Check if the daemon is currently running.
pub async fn is_running() -> bool {
    let slot = daemon_slot();
    let guard = slot.lock().await;
    guard.is_some()
}