use mccp_server::{AppState, run_http, run_stdio};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = mccp_core::Config::load_or_default()?;
    let state = AppState::init(config).await?;

    // Start MCP server (stdio or HTTP/SSE depending on --transport flag)
    let transport = std::env::args().nth(1).unwrap_or_default();
    match transport.as_str() {
        "--stdio" => run_stdio(state).await,
        _         => run_http(state, "127.0.0.1:7422").await,
    }
}