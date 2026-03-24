mod cli;
mod daemon;
mod config;
mod logging;

use clap::Parser;
use tracing::{info, error};
use anyhow::Result;

#[derive(Parser)]
#[command(name = "mccp")]
#[command(about = "Local-first MCP server for code intelligence")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Start the daemon (MCP server + indexer)
    Start,
    /// Stop the daemon
    Stop,
    /// Restart the daemon
    Restart,
    /// Show daemon status
    Status,
    /// Project management commands
    #[command(subcommand)]
    Project(ProjectCommands),
    /// Indexing commands
    #[command(subcommand)]
    Index(IndexCommands),
    /// Provider management commands
    #[command(subcommand)]
    Provider(ProviderCommands),
    /// Model management commands
    #[command(subcommand)]
    Model(ModelCommands),
    /// Docker management commands
    #[command(subcommand)]
    Docker(DockerCommands),
    /// Query and debug commands
    #[command(subcommand)]
    Query(QueryCommands),
    /// Logging commands
    #[command(subcommand)]
    Logs(LogCommands),
    /// Open the interactive TUI console
    Console,
}

#[derive(clap::Subcommand)]
enum ProjectCommands {
    /// Add a project
    Add { path: String, name: Option<String> },
    /// Remove a project
    Remove { name: String },
    /// List all projects
    List,
    /// Show project info
    Info { name: String },
    /// Set default project
    SetDefault { name: String },
}

#[derive(clap::Subcommand)]
enum IndexCommands {
    /// Index a project (incremental)
    Index { project: Option<String> },
    /// Force full re-index
    Full { project: Option<String> },
    /// Reset and re-index from scratch
    Reset { project: Option<String> },
    /// Show index status
    Status,
    /// Pause indexer
    Pause,
    /// Resume indexer
    Resume,
    /// Enable/disable file watching
    Watch,
}

#[derive(clap::Subcommand)]
enum ProviderCommands {
    /// Show provider status
    Status,
    /// Set embedding provider
    SetEmbed { provider: String, model: Option<String>, url: Option<String>, api_key: Option<String> },
    /// Set LLM provider
    SetLlm { provider: String, model: Option<String>, url: Option<String>, api_key: Option<String> },
    /// Set vector store provider
    SetVector { provider: String, url: Option<String>, api_key: Option<String> },
    /// Test provider connectivity
    Test { slot: String },
    /// Reset provider to default
    Reset { slot: String },
    /// List all configured providers
    List,
}

#[derive(clap::Subcommand)]
enum ModelCommands {
    /// List downloaded models
    List,
    /// List available models
    Available,
    /// Pull a model
    Pull { model: String },
    /// Remove a model
    Remove { model: String },
    /// Set active embedding model
    UseEmbed { model: String },
    /// Set active chat model
    UseChat { model: String },
    /// Set active rerank model
    UseRerank { model: String },
    /// Show model status
    Status,
    /// Pull recommended models
    PullRecommended,
}

#[derive(clap::Subcommand)]
enum DockerCommands {
    /// Install Docker and Compose
    Install,
    /// Show Docker status
    Status,
    /// Start Docker services
    Start,
    /// Stop Docker services
    Stop,
    /// Restart Docker services
    Restart,
    /// Reset Docker (destructive)
    Reset,
    /// Set data directory
    SetDataDir { path: String },
    /// Show Docker logs
    Logs { service: Option<String> },
    /// Upgrade Docker images
    Upgrade,
}

#[derive(clap::Subcommand)]
enum QueryCommands {
    /// Run a semantic query
    Query { text: String, project: Option<String>, top_k: Option<usize> },
    /// Trace execution flow
    Flow { entry: String, depth: Option<usize> },
    /// Show file summary
    Summary { path: String },
    /// Show related files
    Related { path: String, depth: Option<usize> },
    /// Search with mode override
    Search { text: String, mode: String },
}

#[derive(clap::Subcommand)]
enum LogCommands {
    /// Show logs
    Logs { level: Option<String>, project: Option<String>, component: Option<String>, since: Option<String> },
    /// Export logs
    Export { since: Option<String> },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Initialize logging
    logging::init()?;
    
    match cli.command {
        Commands::Start => {
            info!("Starting mccp daemon...");
            daemon::start().await?;
        }
        Commands::Stop => {
            info!("Stopping mccp daemon...");
            daemon::stop().await?;
        }
        Commands::Restart => {
            info!("Restarting mccp daemon...");
            daemon::restart().await?;
        }
        Commands::Status => {
            info!("Checking mccp daemon status...");
            daemon::status().await?;
        }
        Commands::Project(cmd) => {
            match cmd {
                ProjectCommands::Add { path, name } => {
                    info!("Adding project at {} with name {:?}", path, name);
                    // TODO: Implement project management
                }
                ProjectCommands::Remove { name } => {
                    info!("Removing project: {}", name);
                    // TODO: Implement project removal
                }
                ProjectCommands::List => {
                    info!("Listing projects...");
                    // TODO: Implement project listing
                }
                ProjectCommands::Info { name } => {
                    info!("Showing info for project: {}", name);
                    // TODO: Implement project info
                }
                ProjectCommands::SetDefault { name } => {
                    info!("Setting default project: {}", name);
                    // TODO: Implement default project setting
                }
            }
        }
        Commands::Index(cmd) => {
            match cmd {
                IndexCommands::Index { project } => {
                    info!("Indexing project: {:?}", project);
                    // TODO: Implement indexing
                }
                IndexCommands::Full { project } => {
                    info!("Full re-index for project: {:?}", project);
                    // TODO: Implement full re-index
                }
                IndexCommands::Reset { project } => {
                    info!("Resetting index for project: {:?}", project);
                    // TODO: Implement index reset
                }
                IndexCommands::Status => {
                    info!("Showing index status...");
                    // TODO: Implement index status
                }
                IndexCommands::Pause => {
                    info!("Pausing indexer...");
                    // TODO: Implement indexer pause
                }
                IndexCommands::Resume => {
                    info!("Resuming indexer...");
                    // TODO: Implement indexer resume
                }
                IndexCommands::Watch => {
                    info!("Toggling file watching...");
                    // TODO: Implement file watching toggle
                }
            }
        }
        Commands::Provider(cmd) => {
            match cmd {
                ProviderCommands::Status => {
                    info!("Showing provider status...");
                    // TODO: Implement provider status
                }
                ProviderCommands::SetEmbed { provider, model, url, api_key } => {
                    info!("Setting embedding provider: {} {:?} {:?} {:?}", provider, model, url, api_key);
                    // TODO: Implement embedding provider setting
                }
                ProviderCommands::SetLlm { provider, model, url, api_key } => {
                    info!("Setting LLM provider: {} {:?} {:?} {:?}", provider, model, url, api_key);
                    // TODO: Implement LLM provider setting
                }
                ProviderCommands::SetVector { provider, url, api_key } => {
                    info!("Setting vector provider: {} {:?} {:?}", provider, url, api_key);
                    // TODO: Implement vector provider setting
                }
                ProviderCommands::Test { slot } => {
                    info!("Testing provider slot: {}", slot);
                    // TODO: Implement provider testing
                }
                ProviderCommands::Reset { slot } => {
                    info!("Resetting provider slot: {}", slot);
                    // TODO: Implement provider reset
                }
                ProviderCommands::List => {
                    info!("Listing providers...");
                    // TODO: Implement provider listing
                }
            }
        }
        Commands::Model(cmd) => {
            match cmd {
                ModelCommands::List => {
                    info!("Listing models...");
                    // TODO: Implement model listing
                }
                ModelCommands::Available => {
                    info!("Listing available models...");
                    // TODO: Implement available models listing
                }
                ModelCommands::Pull { model } => {
                    info!("Pulling model: {}", model);
                    // TODO: Implement model pulling
                }
                ModelCommands::Remove { model } => {
                    info!("Removing model: {}", model);
                    // TODO: Implement model removal
                }
                ModelCommands::UseEmbed { model } => {
                    info!("Setting active embedding model: {}", model);
                    // TODO: Implement embedding model selection
                }
                ModelCommands::UseChat { model } => {
                    info!("Setting active chat model: {}", model);
                    // TODO: Implement chat model selection
                }
                ModelCommands::UseRerank { model } => {
                    info!("Setting active rerank model: {}", model);
                    // TODO: Implement rerank model selection
                }
                ModelCommands::Status => {
                    info!("Showing model status...");
                    // TODO: Implement model status
                }
                ModelCommands::PullRecommended => {
                    info!("Pulling recommended models...");
                    // TODO: Implement recommended model pulling
                }
            }
        }
        Commands::Docker(cmd) => {
            match cmd {
                DockerCommands::Install => {
                    info!("Installing Docker...");
                    // TODO: Implement Docker installation
                }
                DockerCommands::Status => {
                    info!("Showing Docker status...");
                    // TODO: Implement Docker status
                }
                DockerCommands::Start => {
                    info!("Starting Docker services...");
                    // TODO: Implement Docker start
                }
                DockerCommands::Stop => {
                    info!("Stopping Docker services...");
                    // TODO: Implement Docker stop
                }
                DockerCommands::Restart => {
                    info!("Restarting Docker services...");
                    // TODO: Implement Docker restart
                }
                DockerCommands::Reset => {
                    info!("Resetting Docker (destructive)...");
                    // TODO: Implement Docker reset
                }
                DockerCommands::SetDataDir { path } => {
                    info!("Setting Docker data directory: {}", path);
                    // TODO: Implement data directory setting
                }
                DockerCommands::Logs { service } => {
                    info!("Showing Docker logs for service: {:?}", service);
                    // TODO: Implement Docker logs
                }
                DockerCommands::Upgrade => {
                    info!("Upgrading Docker images...");
                    // TODO: Implement Docker upgrade
                }
            }
        }
        Commands::Query(cmd) => {
            match cmd {
                QueryCommands::Query { text, project, top_k } => {
                    info!("Querying: '{}' project: {:?} top_k: {:?}", text, project, top_k);
                    // TODO: Implement query
                }
                QueryCommands::Flow { entry, depth } => {
                    info!("Tracing flow from: '{}' depth: {:?}", entry, depth);
                    // TODO: Implement flow tracing
                }
                QueryCommands::Summary { path } => {
                    info!("Getting summary for: {}", path);
                    // TODO: Implement summary
                }
                QueryCommands::Related { path, depth } => {
                    info!("Finding related files for: '{}' depth: {:?}", path, depth);
                    // TODO: Implement related files
                }
                QueryCommands::Search { text, mode } => {
                    info!("Searching: '{}' mode: {}", text, mode);
                    // TODO: Implement search
                }
            }
        }
        Commands::Logs(cmd) => {
            match cmd {
                LogCommands::Logs { level, project, component, since } => {
                    info!("Showing logs level: {:?} project: {:?} component: {:?} since: {:?}", level, project, component, since);
                    // TODO: Implement logs
                }
                LogCommands::Export { since } => {
                    info!("Exporting logs since: {:?}", since);
                    // TODO: Implement log export
                }
            }
        }
        Commands::Console => {
            info!("Opening TUI console...");
            // TODO: Implement TUI console
        }
    }
    
    Ok(())
}