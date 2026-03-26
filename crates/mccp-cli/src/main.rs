mod commands;
mod config;
mod utils;

use clap::{Parser, Subcommand};
use commands::*;
use config::CliConfig;
use std::path::PathBuf;
use tracing::{info, error, warn};
use tracing_subscriber;

/// MCCP - Multi-Context Code Processor
/// A tool for indexing and analyzing codebases to provide context for LLMs
#[derive(Parser)]
#[command(name = "mccp")]
#[command(about = "Multi-Context Code Processor")]
#[command(version)]
struct Cli {
    /// Path to configuration file
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new project
    Init(InitCommand),
    
    /// Start the MCCP daemon
    Start(StartCommand),
    
    /// Stop the MCCP daemon
    Stop(StopCommand),
    
    /// Index a project
    Index(IndexCommand),
    
    /// Search for symbols or code
    Search(SearchCommand),
    
    /// Get project information
    Project(ProjectCommand),
    
    /// Manage LLM providers
    Provider(ProviderCommand),
    
    /// Get statistics
    Stats(StatsCommand),
    
    /// Configure the system
    Config(ConfigCommand),
    
    /// View MCCP logs
    Logs(LogsCommand),

    /// Test the system
    Test(TestCommand),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Set up logging based on verbosity — must happen before subscriber init
    let cli = Cli::parse();
    
    if cli.verbose {
        std::env::set_var("RUST_LOG", "mccp_cli=debug,mccp_core=debug");
    } else {
        std::env::set_var("RUST_LOG", "mccp_cli=info,mccp_core=info");
    }
    
    // Initialize tracing
    tracing_subscriber::fmt::init();
    
    // Load configuration
    let config = match &cli.config {
        Some(config_path) => CliConfig::load(config_path)?,
        None => CliConfig::default(),
    };
    
    // Execute command
    match cli.command {
        Commands::Init(cmd) => cmd.execute(&config).await,
        Commands::Start(cmd) => cmd.execute(&config).await,
        Commands::Stop(cmd) => cmd.execute(&config).await,
        Commands::Index(cmd) => cmd.execute(&config).await,
        Commands::Search(cmd) => cmd.execute(&config).await,
        Commands::Project(cmd) => cmd.execute(&config).await,
        Commands::Provider(cmd) => cmd.execute(&config).await,
        Commands::Stats(cmd) => cmd.execute(&config).await,
        Commands::Config(cmd) => cmd.execute(&config).await,
        Commands::Logs(cmd) => cmd.execute(&config).await,
        Commands::Test(cmd) => cmd.execute(&config).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_cli_parsing() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        
        let args = vec![
            "mccp",
            "--config", config_path.to_str().unwrap(),
            "--verbose",
            "init",
            "--path", "/tmp/test",
            "--name", "test-project",
        ];
        
        let cli = Cli::parse_from(args);
        
        assert!(cli.config.is_some());
        assert!(cli.verbose);
        assert!(matches!(cli.command, Commands::Init(_)));
    }

    #[test]
    fn test_default_config() {
        let config = CliConfig::default();
        
        assert_eq!(config.server.port, 3000);
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.indexer.watch_enabled, true);
        assert_eq!(config.indexer.parallel_workers, 0);
    }
}