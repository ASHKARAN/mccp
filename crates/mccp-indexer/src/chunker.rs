use super::*;
use mccp_core::*;

/// Chunker for splitting source files into chunks
#[derive(Debug, Clone)]
pub struct Chunker {
    config: ChunkConfig,
}

impl Chunker {
    /// Create a new chunker
    pub fn new(config: ChunkConfig) -> Self {
        Self { config }
    }

    /// Chunk a source file
    pub fn chunk_file(&self, file: &SourceFile) -> Vec<Chunk> {
        match file.language {
            Language::Rust | Language::TypeScript | Language::JavaScript | Language::Python => {
                self.chunk_with_parser(file)
            }
            _ => {
                self.chunk_simple(file)
            }
        }
    }

    /// Simple chunking by lines (fallback for unsupported languages)
    fn chunk_simple(&self, file: &SourceFile) -> Vec<Chunk> {
        let lines: Vec<&str> = file.content.lines().collect();
        let mut chunks = Vec::new();
        let mut current_pos = 0;
        
        for chunk_start in (0..lines.len()).step_by(self.config.max_tokens / 10) {
            let chunk_end = (chunk_start + self.config.max_tokens / 10).min(lines.len());
            let content = lines[chunk_start..chunk_end].join("\n");
            
            let start_byte = current_pos;
            let end_byte = start_byte + content.len();
            let start_line = chunk_start + 1;
            let end_line = chunk_end + 1;
            
            let chunk = Chunk::new(
                "unknown".to_string(), // TODO: Get actual project ID
                file.path.to_string_lossy().to_string(),
                content,
                start_byte,
                end_byte,
                start_line,
                end_line,
                ChunkScope::File,
            );
            
            chunks.push(chunk);
            current_pos = end_byte;
        }
        
        self.apply_overlap(&mut chunks);
        chunks
    }

    /// Chunk using tree-sitter parser (for supported languages)
    fn chunk_with_parser(&self, file: &SourceFile) -> Vec<Chunk> {
        // For now, fall back to simple chunking
        // TODO: Implement tree-sitter integration
        self.chunk_simple(file)
    }

    /// Apply overlap between adjacent chunks
    fn apply_overlap(&self, chunks: &mut Vec<Chunk>) {
        for i in 1..chunks.len() {
            let prev = &chunks[i - 1];
            let curr = &mut chunks[i];
            
            // Calculate overlap needed
            let overlap_needed = self.config.overlap_tokens;
            if overlap_needed == 0 {
                continue;
            }
            
            // Simple overlap by extending content backwards
            let overlap_content = prev.content
                .split_whitespace()
                .rev()
                .take(overlap_needed)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join(" ");
            
            curr.content = format!("{} {}", overlap_content, curr.content);
            curr.start_byte = curr.start_byte.saturating_sub(overlap_content.len());
        }
    }

    /// Get chunking statistics
    pub fn stats(&self, chunks: &[Chunk]) -> ChunkStats {
        let total_tokens: usize = chunks.iter().map(|c| c.token_count).sum();
        let avg_tokens = if chunks.is_empty() { 0 } else { total_tokens / chunks.len() };
        let max_tokens = chunks.iter().map(|c| c.token_count).max().unwrap_or(0);
        let min_tokens = chunks.iter().map(|c| c.token_count).min().unwrap_or(0);
        
        ChunkStats {
            total_chunks: chunks.len(),
            total_tokens,
            avg_tokens,
            max_tokens,
            min_tokens,
        }
    }
}

/// Chunking statistics
#[derive(Debug, Clone)]
pub struct ChunkStats {
    pub total_chunks: usize,
    pub total_tokens: usize,
    pub avg_tokens: usize,
    pub max_tokens: usize,
    pub min_tokens: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_chunker_creation() {
        let config = ChunkConfig::default();
        let chunker = Chunker::new(config);
        
        assert_eq!(chunker.config.max_tokens, 512);
        assert_eq!(chunker.config.overlap_tokens, 64);
    }

    #[test]
    fn test_chunk_simple() {
        let config = ChunkConfig {
            max_tokens: 100,
            overlap_tokens: 10,
        };
        let chunker = Chunker::new(config);
        
        let temp_file = NamedTempFile::new().unwrap();
        let content = "fn main() {\n    println!(\"hello\");\n}\n\nfn test() {\n    assert_eq!(1 + 1, 2);\n}";
        std::fs::write(temp_file.path(), content).unwrap();
        
        let file = SourceFile::from_path(temp_file.path()).unwrap();
        let chunks = chunker.chunk_simple(&file);
        
        assert!(!chunks.is_empty());
        assert!(chunks.iter().all(|c| c.token_count <= 100));
    }

    #[test]
    fn test_chunk_with_parser() {
        let config = ChunkConfig::default();
        let chunker = Chunker::new(config);
        
        let temp_file = NamedTempFile::new().unwrap();
        let content = "fn main() {\n    println!(\"hello\");\n}\n\nfn test() {\n    assert_eq!(1 + 1, 2);\n}";
        std::fs::write(temp_file.path(), content).unwrap();
        
        let file = SourceFile::from_path(temp_file.path()).unwrap();
        let chunks = chunker.chunk_with_parser(&file);
        
        assert!(!chunks.is_empty());
        assert!(chunks.iter().all(|c| c.token_count <= 512));
    }

    #[test]
    fn test_apply_overlap() {
        let config = ChunkConfig {
            max_tokens: 100,
            overlap_tokens: 10,
        };
        let chunker = Chunker::new(config);
        
        let mut chunks = vec![
            Chunk::new(
                "proj_123".to_string(),
                "src/main.rs".to_string(),
                "fn main() {}".to_string(),
                0,
                12,
                1,
                1,
                ChunkScope::Function("main".to_string()),
            ),
            Chunk::new(
                "proj_123".to_string(),
                "src/main.rs".to_string(),
                "fn test() {}".to_string(),
                13,
                25,
                2,
                2,
                ChunkScope::Function("test".to_string()),
            ),
        ];
        
        chunker.apply_overlap(&mut chunks);
        
        // Check that overlap was applied
        assert!(chunks[1].content.contains("fn main()"));
    }

    #[test]
    fn test_chunk_stats() {
        let config = ChunkConfig::default();
        let chunker = Chunker::new(config);
        
        let chunks = vec![
            Chunk::new(
                "proj_123".to_string(),
                "src/main.rs".to_string(),
                "fn main() {}".to_string(),
                0,
                12,
                1,
                1,
                ChunkScope::Function("main".to_string()),
            ),
            Chunk::new(
                "proj_123".to_string(),
                "src/main.rs".to_string(),
                "fn test() {}".to_string(),
                13,
                25,
                2,
                2,
                ChunkScope::Function("test".to_string()),
            ),
        ];
        
        let stats = chunker.stats(&chunks);
        
        assert_eq!(stats.total_chunks, 2);
        assert_eq!(stats.total_tokens, 6); // "fn", "main", "()", "fn", "test", "()"
        assert_eq!(stats.avg_tokens, 3);
        assert_eq!(stats.max_tokens, 3);
        assert_eq!(stats.min_tokens, 3);
    }
}