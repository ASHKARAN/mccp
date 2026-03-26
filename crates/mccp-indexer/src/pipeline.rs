use super::*;
use mccp_core::*;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, watch, Mutex};
use tokio::time::{Duration, Instant};
use ignore::WalkBuilder;

/// Indexing pipeline for processing source files
pub struct IndexingPipeline {
    project: Project,
    config: IndexerConfig,
    chunker: Chunker,
    parser: Parser,
    summarizer: Summarizer,
    graph_builder: GraphBuilder,
    file_watcher: Option<FileWatcher>,
    file_hash_cache: dashmap::DashMap<String, String>,
    processing_queue: tokio::sync::mpsc::UnboundedSender<IndexJob>,
    processing_workers: Vec<tokio::task::JoinHandle<()>>,
    progress_tx: tokio::sync::watch::Sender<Option<IndexProgress>>,
    pub progress_rx: tokio::sync::watch::Receiver<Option<IndexProgress>>,
}

impl std::fmt::Debug for IndexingPipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IndexingPipeline")
            .field("project", &self.project)
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

/// A job to index a specific file
#[derive(Debug, Clone)]
pub struct IndexJob {
    pub project_id: String,
    pub file_path: String,
    pub content: String,
    pub language: Language,
    pub hash: String,
}

impl IndexingPipeline {
    /// Create a new indexing pipeline
    pub fn new(project: Project, config: IndexerConfig) -> Self {
        let (tx, rx) = mpsc::unbounded_channel::<IndexJob>();
        let (progress_tx, progress_rx) = watch::channel(None);
        // Wrap receiver in Arc<Mutex> so multiple workers can share it (work-stealing)
        let rx = Arc::new(Mutex::new(rx));
        
        let chunker = Chunker::new(ChunkConfig {
            max_tokens: config.max_chunk_tokens,
            overlap_tokens: config.chunk_overlap,
        });
        
        let parser = Parser::new();
        let summarizer = Summarizer::new();
        let graph_builder = GraphBuilder::new();
        
        let processing_workers = if config.parallel_workers == 0 {
            num_cpus::get()
        } else {
            config.parallel_workers
        };
        
        let workers = (0..processing_workers).map(|_| {
            let rx = Arc::clone(&rx);
            let chunker = chunker.clone();
            let parser = parser.clone();
            let summarizer = summarizer.clone();
            let graph_builder = graph_builder.clone();
            
            tokio::spawn(async move {
                loop {
                    let job = rx.lock().await.recv().await;
                    match job {
                        Some(job) => {
                            if let Err(e) = Self::process_job(job, &chunker, &parser, &summarizer, &graph_builder).await {
                                eprintln!("Error processing job: {}", e);
                            }
                        }
                        None => break,
                    }
                }
            })
        }).collect();
        
        Self {
            project,
            config,
            chunker,
            parser,
            summarizer,
            graph_builder,
            file_watcher: None,
            file_hash_cache: dashmap::DashMap::new(),
            processing_queue: tx,
            processing_workers: workers,
            progress_tx,
            progress_rx,
        }
    }

    /// Emit progress update
    fn emit_progress(&self, phase: &str, current: usize, total: usize, percentage: u8) {
        let _ = self.progress_tx.send(Some(IndexProgress {
            phase: phase.to_string(),
            current,
            total,
            percentage,
            project_id: self.project.id.as_str().to_string(),
        }));
    }

    /// Start the indexing pipeline
    pub async fn start(&mut self) -> Result<()> {
        // Start file watcher if enabled
        if self.config.watch_enabled {
            let watcher = FileWatcher::start(&self.project.root_path, 50)?;
            self.file_watcher = Some(watcher);
        }
        
        // Index existing files
        self.index_existing_files().await?;
        
        // Start watching for changes
        if let Some(watcher) = self.file_watcher.take() {
            self.start_watching(watcher).await?;
        }
        
        Ok(())
    }

    /// Stop the indexing pipeline
    pub async fn stop(&self) -> Result<()> {
        // Stop processing workers
        for worker in &self.processing_workers {
            worker.abort();
        }
        
        // Stop file watcher
        if let Some(watcher) = &self.file_watcher {
            // Note: FileWatcher doesn't have a stop method, it will be dropped
        }
        
        Ok(())
    }

    /// Index existing files in the project
    async fn index_existing_files(&self) -> Result<()> {
        let files = self.walk_project_files();
        let total = files.len();

        self.emit_progress("chunking", 0, total, 0);

        for (i, file_path) in files.into_iter().enumerate() {
            let relative_path = self.project.relative_path(&file_path)
                .ok_or_else(|| Error::FileReadError {
                    path: file_path.to_string_lossy().to_string(),
                    error: "Could not get relative path".to_string(),
                })?;

            // Check if file has changed
            if let Ok(source_file) = SourceFile::from_path(&file_path) {
                let cached_hash = self.file_hash_cache.get(&relative_path);

                if cached_hash.map_or(true, |h| h.value() != &source_file.hash) {
                    // File has changed, queue for indexing
                    let job = IndexJob {
                        project_id: self.project.id.as_str().to_string(),
                        file_path: relative_path.clone(),
                        content: source_file.content,
                        language: source_file.language,
                        hash: source_file.hash.clone(),
                    };

                    self.processing_queue
                        .send(job)
                        .map_err(|_| Error::IndexError("Failed to queue job".to_string()))?;

                    // Update cache
                    self.file_hash_cache.insert(relative_path, source_file.hash);
                }
            }

            let current = i + 1;
            let percentage = if total == 0 { 100 } else { ((current * 100) / total).min(100) as u8 };
            self.emit_progress("chunking", current, total, percentage);
        }

        if total == 0 {
            self.emit_progress("chunking", 0, 0, 100);
        }

        Ok(())
    }

    /// Start watching for file changes
    async fn start_watching(&self, mut watcher: FileWatcher) -> Result<()> {
        tokio::spawn(async move {
            while let Some(event) = watcher.next_event().await {
                match event {
                    FileChangeEvent::Created(path) | FileChangeEvent::Modified(path) => {
                        if let Some(ext) = path.extension() {
                            if Language::from_extension(&ext.to_string_lossy()).is_some() {
                                if let Ok(source_file) = SourceFile::from_path(&path) {
                                    let relative_path = source_file.relative_path(&PathBuf::from("/tmp")); // TODO: Use actual project root
                                    if let Some(relative_path) = relative_path {
                                        let job = IndexJob {
                                            project_id: "unknown".to_string(), // TODO: Get actual project ID
                                            file_path: relative_path,
                                            content: source_file.content,
                                            language: source_file.language,
                                            hash: source_file.hash,
                                        };
                                        
                                        // TODO: Send job to processing queue
                                    }
                                }
                            }
                        }
                    }
                    FileChangeEvent::Deleted(path) => {
                        // TODO: Handle file deletion
                    }
                    FileChangeEvent::Moved { from, to } => {
                        // TODO: Handle file moves
                    }
                }
            }
        });
        
        Ok(())
    }

    /// Process a single indexing job
    async fn process_job(
        job: IndexJob,
        chunker: &Chunker,
        parser: &Parser,
        summarizer: &Summarizer,
        graph_builder: &GraphBuilder,
    ) -> Result<()> {
        // Parse the file
        let symbols = parser.parse(&job.content, job.language)?;
        
        // Chunk the file
        let source_file = SourceFile {
            path: PathBuf::from(&job.file_path),
            language: job.language,
            content: job.content.clone(),
            hash: job.hash,
            size: job.content.len(),
            modified: chrono::Utc::now(),
        };
        
        let chunks = chunker.chunk_file(&source_file);
        
        // TODO: Generate summaries for chunks
        for chunk in &chunks {
            // TODO: Send chunk to summarizer
        }
        
        // Build graph
        graph_builder.add_symbols(&job.file_path, symbols);
        
        Ok(())
    }

    /// Force re-index all files
    pub async fn force_reindex(&self) -> Result<()> {
        self.file_hash_cache.clear();
        self.index_existing_files().await
    }

    /// Get indexing status
    pub fn status(&self) -> IndexingStatus {
        IndexingStatus {
            project_id: self.project.id.as_str().to_string(),
            file_count: self.project.source_files().len(),
            indexed_files: self.file_hash_cache.len(),
            queue_depth: 0, // unbounded channel has no fixed capacity
            is_watching: self.config.watch_enabled,
        }
    }

    /// Walk project files respecting .gitignore and other ignore patterns
    fn walk_project_files(&self) -> Vec<PathBuf> {
        let global_ignore = dirs::home_dir().map(|h| h.join(".mccp").join("ignore"));

        let mut builder = WalkBuilder::new(&self.project.root_path);
        builder
            .hidden(false)          // index dotfiles that aren't gitignored
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true);

        if let Some(ref p) = global_ignore {
            if p.exists() { builder.add_ignore(p); }
        }

        // Add extra ignore patterns from config
        for pattern in &self.config.extra_ignore_patterns {
            builder.add_ignore(pattern);
        }

        builder
            .build()
            .filter_map(|entry| entry.ok())
            .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
            .filter(|e| is_supported_extension(e.path()))
            .map(|e| e.into_path())
            .collect()
    }
}

/// Returns true if the file extension is a supported source language
fn is_supported_extension(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| Language::from_extension(ext).is_some())
        .unwrap_or(false)
}

/// Indexing progress event
#[derive(Debug, Clone, serde::Serialize)]
pub struct IndexProgress {
    pub phase:      String,
    pub current:    usize,
    pub total:      usize,
    pub percentage: u8,
    pub project_id: String,
}

/// Indexing status
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IndexingStatus {
    pub project_id: String,
    pub file_count: usize,
    pub indexed_files: usize,
    pub queue_depth: usize,
    pub is_watching: bool,
}

/// Parser for extracting symbols from source files
#[derive(Debug, Clone)]
pub struct Parser;

impl Parser {
    /// Create a new parser
    pub fn new() -> Self {
        Self
    }

    /// Parse a source file and extract symbols
    pub fn parse(&self, content: &str, language: Language) -> Result<Vec<Symbol>> {
        let source_file = SourceFile {
            path: PathBuf::from("temp"),
            language,
            content: content.to_string(),
            hash: {
                use sha2::Digest;
                format!("{:x}", sha2::Sha256::new().chain_update(content.as_bytes()).finalize())
            },
            size: content.len(),
            modified: chrono::Utc::now(),
        };
        
        Ok(SymbolExtractor::extract_symbols(&source_file))
    }
}

/// Summarizer for generating LLM summaries of code chunks
#[derive(Debug, Clone)]
pub struct Summarizer;

impl Summarizer {
    /// Create a new summarizer
    pub fn new() -> Self {
        Self
    }

    /// Generate a summary for a code chunk
    pub async fn summarize(&self, chunk: &Chunk) -> Result<String> {
        // TODO: Implement LLM summarization
        // For now, return a placeholder summary
        Ok(format!("Summary for chunk {} in file {}", chunk.id, chunk.file_path))
    }
}

/// Graph builder for constructing call graphs
#[derive(Debug, Clone)]
pub struct GraphBuilder;

impl GraphBuilder {
    /// Create a new graph builder
    pub fn new() -> Self {
        Self
    }

    /// Add symbols to the graph
    pub fn add_symbols(&self, file_path: &str, symbols: Vec<Symbol>) {
        // TODO: Implement graph building
    }

    /// Build a call graph for a project
    pub fn build_graph(&self, files: &[SourceFile]) -> GraphStore {
        mccp_core::GraphBuilder::build_graph(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_indexing_pipeline_creation() {
        let temp_dir = TempDir::new().unwrap();
        let project = Project::new("test".to_string(), temp_dir.path());
        let config = IndexerConfig::default();
        
        let pipeline = IndexingPipeline::new(project, config);
        
        assert_eq!(pipeline.project.name, "test");
        assert_eq!(pipeline.processing_workers.len(), num_cpus::get());
    }

    #[test]
    fn test_parser() {
        let parser = Parser::new();
        let content = "fn main() { println!(\"hello\"); }";
        let language = Language::Rust;
        
        let symbols = parser.parse(content, language).unwrap();
        
        assert!(!symbols.is_empty());
        assert_eq!(symbols[0].name, "main");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
    }

    #[tokio::test]
    async fn test_summarizer() {
        let summarizer = Summarizer::new();
        let chunk = Chunk::new(
            "proj_123".to_string(),
            "src/main.rs".to_string(),
            "fn main() {}".to_string(),
            0,
            12,
            1,
            1,
            ChunkScope::Function("main".to_string()),
        );
        
        let result = summarizer.summarize(&chunk).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_graph_builder() {
        let graph_builder = GraphBuilder::new();
        let files = vec![]; // Empty for testing
        
        let graph = graph_builder.build_graph(&files);
        
        assert_eq!(graph.all_nodes().len(), 0);
        assert_eq!(graph.all_edges().len(), 0);
    }
}