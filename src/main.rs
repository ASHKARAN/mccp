mod daemon;
mod config;
mod system_config;
mod embeddings;
mod vector_store;
mod indexer;
mod command_input;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    command_input::run().await
}
