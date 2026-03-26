use mccp_server::{run_http, run_stdio, AppState};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn expand_tilde(p: &std::path::Path) -> std::path::PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return std::path::PathBuf::from(home).join(rest);
        }
    }
    p.to_path_buf()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = mccp_core::Config::load_or_default()?;
    let http_port = config.daemon.http_port;

    // Log to stdout + ~/.mccp/logs/mccp.log (or config.daemon.log_dir)
    let log_dir = expand_tilde(&config.daemon.log_dir);
    let _ = std::fs::create_dir_all(&log_dir);
    let file_appender = tracing_appender::rolling::never(&log_dir, "mccp.log");
    let (file_writer, _file_guard) = tracing_appender::non_blocking(file_appender);

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(config.daemon.log_level.clone()));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer().with_target(true))
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(true)
                .with_ansi(false)
                .with_writer(file_writer),
        )
        .init();

    tracing::info!(log_dir = %log_dir.display(), "logging initialized");

    let state = AppState::init(config).await?;

    // Start MCP server (stdio or HTTP/SSE depending on --transport flag)
    let transport = std::env::args().nth(1).unwrap_or_default();
    match transport.as_str() {
        "--stdio" => run_stdio(state).await,
        _ => {
            let addr = format!("127.0.0.1:{http_port}");
            tracing::info!(%addr, "starting http server");
            run_http(state, &addr).await
        }
    }
}
