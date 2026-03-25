use super::*;
use mccp_core::*;
use mccp_server::*;
use mccp_indexer::*;
use mccp_storage::*;
use mccp_providers::*;
use colored::*;
use std::path::PathBuf;
use std::time::Instant;

/// Command trait for CLI commands
#[async_trait::async_trait]
pub trait Command {
    async fn execute(&self, config: &CliConfig) -> anyhow::Result<()>;
}

/// Initialize command
pub struct InitCommand {
    /// Project path
    pub path: PathBuf,
    
    /// Project name
    pub name: String,
    
    /// Project description
    pub description: Option<String>,
    
    /// Language to index
    pub language: Option<String>,
}

impl InitCommand {
    pub fn new(path: PathBuf, name: String, description: Option<String>, language: Option<String>) -> Self {
        Self {
            path,
            name,
            description,
            language,
        }
    }
}

#[async_trait::async_trait]
impl Command for InitCommand {
    async fn execute(&self, config: &CliConfig) -> anyhow::Result<()> {
        info!("Initializing project: {}", self.name);
        
        // Create project directory if it doesn't exist
        if !self.path.exists() {
            std::fs::create_dir_all(&self.path)?;
            info!("Created project directory: {}", self.path.display());
        }
        
        // Create MCCP configuration file
        let mccp_config = ProjectConfig {
            project_id: self.name.clone(),
            root_path: self.path.clone(),
            language: self.language.clone().map(|l| Language::from_extension(&l)).unwrap_or_default(),
            include_patterns: vec!["**/*".to_string()],
            exclude_patterns: vec![
                "**/node_modules/**".to_string(),
                "**/.git/**".to_string(),
                "**/target/**".to_string(),
                "**/build/**".to_string(),
            ],
            chunk_size: 512,
            chunk_overlap: 64,
            watch_enabled: true,
        };
        
        let config_path = self.path.join(".mccp.toml");
        mccp_config.save(&config_path)?;
        
        // Create .mccpignore file
        let ignore_content = r#"# Ignore common directories
node_modules/
.git/
target/
build/
dist/
out/

# Ignore common files
*.log
*.tmp
*.swp
.DS_Store
"#;
        
        std::fs::write(self.path.join(".mccpignore"), ignore_content)?;
        
        // Create README
        let readme_content = format!(r#"# {}

This project is configured for MCCP (Multi-Context Code Processor).

## Configuration

The project configuration is stored in `.mccp.toml`. You can customize:

- Language settings
- File patterns to include/exclude
- Chunking parameters
- Indexing options

## Usage

To index this project:

```bash
mccp index --path {}
```

To start the MCCP daemon:

```bash
mccp start
```

For more information, see the [MCCP documentation](https://github.com/your-org/mccp).
"#, self.name, self.path.display());
        
        std::fs::write(self.path.join("README.md"), readme_content)?;
        
        println!("{}", "Project initialized successfully!".green().bold());
        println!("Project: {}", self.name.bold());
        println!("Path: {}", self.path.display().to_string().bold());
        println!("Configuration: {}", config_path.display().to_string().bold());
        
        Ok(())
    }
}

/// Start command
pub struct StartCommand {
    /// Port to run the server on
    pub port: Option<u16>,
    
    /// Host to bind to
    pub host: Option<String>,
    
    /// Don't wait for server to start
    pub no_wait: bool,
}

impl StartCommand {
    pub fn new(port: Option<u16>, host: Option<String>, no_wait: bool) -> Self {
        Self {
            port,
            host,
            no_wait,
        }
    }
}

#[async_trait::async_trait]
impl Command for StartCommand {
    async fn execute(&self, config: &CliConfig) -> anyhow::Result<()> {
        info!("Starting MCCP daemon");
        
        // Create server configuration
        let server_config = ServerConfig {
            port: self.port.unwrap_or(config.server.port),
            host: self.host.clone().unwrap_or(config.server.host.clone()),
            max_connections: config.server.max_connections,
            enable_caching: config.server.enable_caching,
            cache_ttl: config.server.cache_ttl,
        };
        
        // Create and start server
        let server = MccpServer::new(server_config).await?;
        server.start().await?;
        
        println!("{}", "MCCP daemon started successfully!".green().bold());
        println!("Server: {}:{}", server_config.host, server_config.port);
        
        if !self.no_wait {
            println!("Press Ctrl+C to stop the server");
            
            // Wait for interrupt signal
            tokio::signal::ctrl_c().await?;
            println!("\nShutting down...");
            
            server.stop().await?;
            println!("{}", "MCCP daemon stopped".yellow().bold());
        }
        
        Ok(())
    }
}

/// Stop command
pub struct StopCommand;

impl StopCommand {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl Command for StopCommand {
    async fn execute(&self, _config: &CliConfig) -> anyhow::Result<()> {
        info!("Stopping MCCP daemon");
        
        // TODO: Implement daemon stopping logic
        // For now, just print a message
        println!("{}", "MCCP daemon stopping...".yellow().bold());
        println!("Note: This is a placeholder implementation. Use Ctrl+C to stop the server.");
        
        Ok(())
    }
}

/// Index command
pub struct IndexCommand {
    /// Project path
    pub path: PathBuf,
    
    /// Force re-indexing
    pub force: bool,
    
    /// Verbose output
    pub verbose: bool,

    /// Show index status without running a full re-index (V3-9)
    pub status_only: bool,
}

impl IndexCommand {
    pub fn new(path: PathBuf, force: bool, verbose: bool) -> Self {
        Self {
            path,
            force,
            verbose,
            status_only: false,
        }
    }

    /// Create a status-only query (V3-9)
    pub fn new_status(path: PathBuf) -> Self {
        Self {
            path,
            force: false,
            verbose: false,
            status_only: true,
        }
    }
}

#[async_trait::async_trait]
impl Command for IndexCommand {
    async fn execute(&self, config: &CliConfig) -> anyhow::Result<()> {
        // Load project configuration
        let config_path = self.path.join(".mccp.toml");
        let project_config = if config_path.exists() {
            ProjectConfig::load(&config_path)?
        } else {
            ProjectConfig::default()
        };

        // V3-9: --status flag — query daemon over HTTP without indexing
        if self.status_only {
            let port = config.server.port;
            let url = format!("http://127.0.0.1:{}/index/status", port);
            let client = reqwest::Client::new();
            match client.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    let body: serde_json::Value = resp.json().await.unwrap_or_default();
                    println!("{}", "Index Status".green().bold());
                    println!("Project ID:    {}", body["project_id"].as_str().unwrap_or("—").bold());
                    println!("Files:         {}", body["file_count"].as_u64().unwrap_or(0).to_string().bold());
                    println!("Indexed:       {}", body["indexed_files"].as_u64().unwrap_or(0).to_string().bold());
                    println!("Queue depth:   {}", body["queue_depth"].as_u64().unwrap_or(0).to_string().bold());
                    println!("Watching:      {}", if body["is_watching"].as_bool().unwrap_or(false) { "Yes" } else { "No" }.bold());
                }
                Ok(resp) => {
                    eprintln!("Daemon returned error: HTTP {}", resp.status());
                }
                Err(_) => {
                    // Fall back to local status if daemon not running
                    let project = Project::new(project_config.project_id, &self.path);
                    let indexer_config = IndexerConfig {
                        max_chunk_tokens: project_config.chunk_size,
                        chunk_overlap: project_config.chunk_overlap,
                        watch_enabled: false,
                        parallel_workers: config.indexer.parallel_workers,
                        include_patterns: project_config.include_patterns,
                        exclude_patterns: project_config.exclude_patterns,
                    };
                    let indexer = IndexingPipeline::new(project, indexer_config);
                    let status = indexer.status();
                    println!("{}", "Index Status (local)".yellow().bold());
                    println!("Project ID:    {}", status.project_id.bold());
                    println!("Files:         {}", status.file_count.to_string().bold());
                    println!("Indexed:       {}", status.indexed_files.to_string().bold());
                    println!("Queue depth:   {}", status.queue_depth.to_string().bold());
                    println!("Watching:      {}", if status.is_watching { "Yes" } else { "No" }.bold());
                }
            }
            return Ok(());
        }

        info!("Indexing project: {}", self.path.display());
        
        // Create project
        let project = Project::new(project_config.project_id, &self.path);
        
        // Create indexer configuration
        let indexer_config = IndexerConfig {
            max_chunk_tokens: project_config.chunk_size,
            chunk_overlap: project_config.chunk_overlap,
            watch_enabled: project_config.watch_enabled,
            parallel_workers: config.indexer.parallel_workers,
            include_patterns: project_config.include_patterns,
            exclude_patterns: project_config.exclude_patterns,
        };
        
        // Create and start indexer
        let mut indexer = IndexingPipeline::new(project, indexer_config);
        
        let start_time = Instant::now();
        
        if self.force {
            indexer.force_reindex().await?;
        } else {
            indexer.start().await?;
        }
        
        let duration = start_time.elapsed();
        
        let status = indexer.status();
        
        println!("{}", "Indexing completed successfully!".green().bold());
        println!("Files processed: {}", status.file_count.to_string().bold());
        println!("Indexed files: {}", status.indexed_files.to_string().bold());
        println!("Time elapsed: {:.2}s", duration.as_secs_f32());
        
        if self.verbose {
            println!("\nDetailed statistics:");
            println!("Project ID: {}", status.project_id.bold());
            println!("Queue depth: {}", status.queue_depth.to_string().bold());
            println!("Watching: {}", if status.is_watching { "Yes" } else { "No" }.bold());
        }
        
        Ok(())
    }
}

/// Search command
pub struct SearchCommand {
    /// Project path
    pub path: PathBuf,
    
    /// Search query
    pub query: String,
    
    /// Search type (symbols, chunks, both)
    pub search_type: String,
    
    /// Limit results
    pub limit: Option<usize>,
}

impl SearchCommand {
    pub fn new(path: PathBuf, query: String, search_type: String, limit: Option<usize>) -> Self {
        Self {
            path,
            query,
            search_type,
            limit,
        }
    }
}

#[async_trait::async_trait]
impl Command for SearchCommand {
    async fn execute(&self, _config: &CliConfig) -> anyhow::Result<()> {
        info!("Searching for: {}", self.query);
        
        // TODO: Implement search functionality
        // For now, just print a message
        println!("{}", "Search functionality is not yet implemented".yellow().bold());
        println!("Query: {}", self.query.bold());
        println!("Type: {}", self.search_type.bold());
        println!("Path: {}", self.path.display().to_string().bold());
        
        if let Some(limit) = self.limit {
            println!("Limit: {}", limit.to_string().bold());
        }
        
        Ok(())
    }
}

/// Project command
pub struct ProjectCommand {
    /// Project path
    pub path: PathBuf,
    
    /// Show detailed information
    pub detailed: bool,
}

impl ProjectCommand {
    pub fn new(path: PathBuf, detailed: bool) -> Self {
        Self {
            path,
            detailed,
        }
    }
}

#[async_trait::async_trait]
impl Command for ProjectCommand {
    async fn execute(&self, _config: &CliConfig) -> anyhow::Result<()> {
        info!("Getting project information for: {}", self.path.display());
        
        // TODO: Implement project information retrieval
        // For now, just print a message
        println!("{}", "Project information is not yet implemented".yellow().bold());
        println!("Path: {}", self.path.display().to_string().bold());
        println!("Detailed: {}", if self.detailed { "Yes" } else { "No" }.bold());
        
        Ok(())
    }
}

/// Provider command
pub struct ProviderCommand {
    /// List providers
    pub list: bool,
    
    /// Add a provider
    pub add: Option<String>,
    
    /// Remove a provider
    pub remove: Option<String>,
    
    /// Test a provider
    pub test: Option<String>,
}

impl ProviderCommand {
    pub fn new(list: bool, add: Option<String>, remove: Option<String>, test: Option<String>) -> Self {
        Self {
            list,
            add,
            remove,
            test,
        }
    }
}

#[async_trait::async_trait]
impl Command for ProviderCommand {
    async fn execute(&self, _config: &CliConfig) -> anyhow::Result<()> {
        info!("Managing LLM providers");
        
        // TODO: Implement provider management
        // For now, just print a message
        if self.list {
            println!("{}", "Listing providers is not yet implemented".yellow().bold());
        }
        
        if let Some(provider) = &self.add {
            println!("Adding provider: {}", provider.bold());
            println!("{}", "Provider addition is not yet implemented".yellow().bold());
        }
        
        if let Some(provider) = &self.remove {
            println!("Removing provider: {}", provider.bold());
            println!("{}", "Provider removal is not yet implemented".yellow().bold());
        }
        
        if let Some(provider) = &self.test {
            println!("Testing provider: {}", provider.bold());
            println!("{}", "Provider testing is not yet implemented".yellow().bold());
        }
        
        Ok(())
    }
}

/// Stats command
pub struct StatsCommand {
    /// Project path
    pub path: Option<PathBuf>,
    
    /// Show detailed statistics
    pub detailed: bool,
}

impl StatsCommand {
    pub fn new(path: Option<PathBuf>, detailed: bool) -> Self {
        Self {
            path,
            detailed,
        }
    }
}

#[async_trait::async_trait]
impl Command for StatsCommand {
    async fn execute(&self, _config: &CliConfig) -> anyhow::Result<()> {
        info!("Getting statistics");
        
        // TODO: Implement statistics retrieval
        // For now, just print a message
        println!("{}", "Statistics are not yet implemented".yellow().bold());
        
        if let Some(path) = &self.path {
            println!("Path: {}", path.display().to_string().bold());
        }
        
        println!("Detailed: {}", if self.detailed { "Yes" } else { "No" }.bold());
        
        Ok(())
    }
}

/// Config command
pub struct ConfigCommand {
    /// Show current configuration
    pub show: bool,
    
    /// Set a configuration value
    pub set: Option<(String, String)>,
    
    /// Reset configuration to defaults
    pub reset: bool,
}

impl ConfigCommand {
    pub fn new(show: bool, set: Option<(String, String)>, reset: bool) -> Self {
        Self {
            show,
            set,
            reset,
        }
    }
}

#[async_trait::async_trait]
impl Command for ConfigCommand {
    async fn execute(&self, config: &CliConfig) -> anyhow::Result<()> {
        info!("Managing configuration");
        
        if self.show {
            println!("{}", "Current Configuration:".bold().underline());
            println!("Server:");
            println!("  Host: {}", config.server.host.bold());
            println!("  Port: {}", config.server.port.to_string().bold());
            println!("  Max Connections: {}", config.server.max_connections.to_string().bold());
            println!("  Caching: {}", if config.server.enable_caching { "Enabled" } else { "Disabled" }.bold());
            println!("Indexer:");
            println!("  Watch Enabled: {}", if config.indexer.watch_enabled { "Yes" } else { "No" }.bold());
            println!("  Parallel Workers: {}", config.indexer.parallel_workers.to_string().bold());
        }
        
        if let Some((key, value)) = &self.set {
            println!("Setting {} = {}", key.bold(), value.bold());
            println!("{}", "Configuration setting is not yet implemented".yellow().bold());
        }
        
        if self.reset {
            println!("{}", "Resetting configuration to defaults".yellow().bold());
            println!("{}", "Configuration reset is not yet implemented".yellow().bold());
        }
        
        Ok(())
    }
}

/// Test command
pub struct TestCommand {
    /// Test all components
    pub all: bool,
    
    /// Test specific component
    pub component: Option<String>,
}

impl TestCommand {
    pub fn new(all: bool, component: Option<String>) -> Self {
        Self {
            all,
            component,
        }
    }
}

#[async_trait::async_trait]
impl Command for TestCommand {
    async fn execute(&self, _config: &CliConfig) -> anyhow::Result<()> {
        info!("Running tests");
        
        if self.all {
            println!("{}", "Running all tests...".bold());
            
            // Test core components
            test_core_components().await?;
            
            println!("{}", "All tests completed successfully!".green().bold());
        }
        
        if let Some(component) = &self.component {
            println!("Testing component: {}", component.bold());
            
            match component.as_str() {
                "core" => test_core_components().await?,
                "indexer" => test_indexer_components().await?,
                "storage" => test_storage_components().await?,
                "providers" => test_provider_components().await?,
                _ => {
                    println!("Unknown component: {}", component.bold());
                    return Err(anyhow::anyhow!("Unknown component: {}", component));
                }
            }
            
            println!("Component test completed successfully!".green().bold());
        }
        
        Ok(())
    }
}

/// Test core components
async fn test_core_components() -> anyhow::Result<()> {
    println!("Testing core components...");
    
    // Test project creation
    let temp_dir = std::env::temp_dir().join("mccp_test");
    let project = Project::new("test".to_string(), &temp_dir);
    
    assert_eq!(project.name, "test");
    assert_eq!(project.root_path, temp_dir);
    
    println!("  ✓ Project creation");
    
    // Test language detection
    assert_eq!(Language::from_extension("rs"), Some(Language::Rust));
    assert_eq!(Language::from_extension("js"), Some(Language::JavaScript));
    assert_eq!(Language::from_extension("py"), Some(Language::Python));
    assert_eq!(Language::from_extension("unknown"), None);
    
    println!("  ✓ Language detection");
    
    // Test symbol creation
    let symbol = Symbol::new(
        "test".to_string(),
        SymbolKind::Function,
        "test".to_string(),
        "test.rs".to_string(),
        1,
        0,
        "fn test() {}".to_string(),
        "module".to_string(),
        Language::Rust,
        RefKind::Definition,
    );
    
    assert_eq!(symbol.name, "test");
    assert_eq!(symbol.kind, SymbolKind::Function);
    assert_eq!(symbol.language, Language::Rust);
    
    println!("  ✓ Symbol creation");
    
    Ok(())
}

/// Test indexer components
async fn test_indexer_components() -> anyhow::Result<()> {
    println!("Testing indexer components...");
    
    // Test chunker
    let config = ChunkConfig::default();
    let chunker = Chunker::new(config);
    
    assert_eq!(chunker.config.max_tokens, 512);
    assert_eq!(chunker.config.overlap_tokens, 64);
    
    println!("  ✓ Chunker creation");
    
    // Test parser
    let parser = Parser::new();
    
    let symbols = parser.parse("fn main() {}", Language::Rust)?;
    assert!(!symbols.is_empty());
    
    println!("  ✓ Parser creation");
    
    // Test summarizer
    let summarizer = Summarizer::new();
    
    let chunk = Chunk::new(
        "test".to_string(),
        "test.rs".to_string(),
        "fn main() {}".to_string(),
        0,
        12,
        1,
        1,
        ChunkScope::Function("main".to_string()),
    );
    
    let summary = summarizer.summarize(&chunk).await?;
    assert!(!summary.is_empty());
    
    println!("  ✓ Summarizer creation");
    
    Ok(())
}

/// Test storage components
async fn test_storage_components() -> anyhow::Result<()> {
    println!("Testing storage components...");
    
    // Test storage backend
    let storage = StorageBackend::new();
    
    assert_eq!(storage.projects.len(), 0);
    assert_eq!(storage.symbols.len(), 0);
    assert_eq!(storage.chunks.len(), 0);
    
    println!("  ✓ Storage backend creation");
    
    // Test cache
    let cache = Cache::new();
    
    cache.set("test".to_string(), "value".to_string());
    assert_eq!(cache.get("test"), Some("value".to_string()));
    
    println!("  ✓ Cache creation");
    
    Ok(())
}

/// Test provider components
async fn test_provider_components() -> anyhow::Result<()> {
    println!("Testing provider components...");
    
    // Test provider manager
    let manager = ProviderManager::new();
    
    assert_eq!(manager.providers.len(), 0);
    assert_eq!(manager.configs.len(), 0);
    assert_eq!(manager.health_status.len(), 0);
    
    println!("  ✓ Provider manager creation");
    
    // Test local provider
    let provider = LocalProvider::new("test".to_string());
    
    assert_eq!(provider.name(), "Local");
    assert_eq!(provider.version(), "1.0.0");
    
    let health = provider.health().await;
    assert_eq!(health.status, ProviderStatusType::Healthy);
    
    println!("  ✓ Local provider creation");
    
    Ok(())
}