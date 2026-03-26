use super::*;

/// A chunk of source code with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub id: String,
    pub project_id: String,
    pub file_path: String,
    pub content: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub scope: ChunkScope,
    pub token_count: usize,
    pub embedding: Option<Vec<f32>>,
    pub summary: Option<String>,
}

impl Chunk {
    /// Create a new chunk
    pub fn new(
        project_id: String,
        file_path: String,
        content: String,
        start_byte: usize,
        end_byte: usize,
        start_line: usize,
        end_line: usize,
        scope: ChunkScope,
    ) -> Self {
        let token_count = content.split_whitespace().count();
        let id = format!("chunk_{}", uuid::Uuid::new_v4());
        
        Self {
            id,
            project_id,
            file_path,
            content,
            start_byte,
            end_byte,
            start_line,
            end_line,
            scope,
            token_count,
            embedding: None,
            summary: None,
        }
    }

    /// Get the chunk's relative importance score
    pub fn importance_score(&self) -> f32 {
        // Higher score for more specific scopes
        match self.scope.level() {
            0 => 0.5,  // Project
            1 => 0.6,  // Module
            2 => 0.7,  // File
            3 => 0.8,  // Class
            4 => 1.0,  // Method/Function
            _ => 0.5,
        }
    }

    /// Check if this chunk overlaps with another
    pub fn overlaps_with(&self, other: &Chunk) -> bool {
        if self.file_path != other.file_path {
            return false;
        }
        
        // Check byte overlap
        let self_start = self.start_byte;
        let self_end = self.end_byte;
        let other_start = other.start_byte;
        let other_end = other.end_byte;
        
        self_start < other_end && other_start < self_end
    }

    /// Get overlap percentage with another chunk
    pub fn overlap_percentage(&self, other: &Chunk) -> f32 {
        if !self.overlaps_with(other) {
            return 0.0;
        }
        
        let self_start = self.start_byte as i64;
        let self_end = self.end_byte as i64;
        let other_start = other.start_byte as i64;
        let other_end = other.end_byte as i64;
        
        let overlap = (self_end.min(other_end) - self_start.max(other_start)).max(0);
        let self_len = (self_end - self_start).max(1);
        
        overlap as f32 / self_len as f32
    }
}

/// Chunker for splitting source files into chunks
#[derive(Clone)]
pub struct Chunker {
    config: ChunkConfig,
}

impl Chunker {
    /// Create a new chunker with configuration
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
            let start_line = chunk_start;
            let end_line = chunk_end;
            
            let chunk = Chunk::new(
                "unknown".to_string(),
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
        let overlap_needed = self.config.overlap_tokens;
        if overlap_needed == 0 {
            return;
        }
        for i in 1..chunks.len() {
            // Compute overlap from previous chunk content without a borrow on `chunks`
            let overlap_content = {
                let prev = &chunks[i - 1];
                prev.content
                    .split_whitespace()
                    .rev()
                    .take(overlap_needed)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join(" ")
            };

            let curr = &mut chunks[i];
            curr.content = format!("{} {}", overlap_content, curr.content);
            curr.start_byte = curr.start_byte.saturating_sub(overlap_content.len());
        }
    }
}

/// Chunk filter for queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkFilter {
    pub language: Option<Language>,
    pub file_pattern: Option<String>,
    pub scope: Option<ChunkScope>,
    pub min_tokens: Option<usize>,
    pub max_tokens: Option<usize>,
}

impl ChunkFilter {
    /// Check if a chunk matches the filter
    pub fn matches(&self, chunk: &Chunk) -> bool {
        if let Some(lang) = &self.language {
            if !chunk.file_path.ends_with(&format!(".{}", lang.extensions()[0])) {
                return false;
            }
        }
        
        if let Some(pattern) = &self.file_pattern {
            if !glob::Pattern::new(pattern).unwrap().matches(&chunk.file_path) {
                return false;
            }
        }
        
        if let Some(scope) = &self.scope {
            if &chunk.scope != scope {
                return false;
            }
        }
        
        if let Some(min_tokens) = self.min_tokens {
            if chunk.token_count < min_tokens {
                return false;
            }
        }
        
        if let Some(max_tokens) = self.max_tokens {
            if chunk.token_count > max_tokens {
                return false;
            }
        }
        
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_chunk_creation() {
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
        
        assert_eq!(chunk.project_id, "proj_123");
        assert_eq!(chunk.file_path, "src/main.rs");
        assert_eq!(chunk.token_count, 3); // "fn", "main", "()"
        assert!(chunk.importance_score() > 0.8);
    }

    #[test]
    fn test_chunk_overlap() {
        let chunk1 = Chunk::new(
            "proj_123".to_string(),
            "src/main.rs".to_string(),
            "fn main() {}".to_string(),
            0,
            12,
            1,
            1,
            ChunkScope::Function("main".to_string()),
        );
        
        let chunk2 = Chunk::new(
            "proj_123".to_string(),
            "src/main.rs".to_string(),
            "fn test() {}".to_string(),
            10,
            22,
            2,
            2,
            ChunkScope::Function("test".to_string()),
        );
        
        assert!(chunk1.overlaps_with(&chunk2));
        assert!(chunk1.overlap_percentage(&chunk2) > 0.0);
    }

    #[test]
    fn test_chunker() {
        let temp_file = tempfile::Builder::new().suffix(".rs").tempfile().unwrap();
        let content = "fn main() {\n    println!(\"hello\");\n}\n\nfn test() {\n    assert_eq!(1 + 1, 2);\n}";
        std::fs::write(temp_file.path(), content).unwrap();
        
        let file = SourceFile::from_path(temp_file.path()).unwrap();
        let chunker = Chunker::new(ChunkConfig::default());
        let chunks = chunker.chunk_file(&file);
        
        assert!(!chunks.is_empty());
        assert!(chunks.iter().all(|c| c.token_count <= 512));
    }
}