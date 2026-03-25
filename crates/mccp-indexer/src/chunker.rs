use super::*;
use mccp_core::*;
use tree_sitter::{Language, Parser, Node};

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
        let Some(cfg) = lang_config(file.language) else {
            return self.chunk_simple(file);
        };
        let mut parser = Parser::new();
        parser.set_language(cfg.ts_language)
              .expect("grammar version mismatch — update tree-sitter-* crates");

        let tree = match parser.parse(&file.content, None) {
            Some(t) => t,
            None    => return self.chunk_simple(file),
        };

        let mut raw_chunks = Vec::new();
        collect_splittable_nodes(
            tree.root_node(),
            &file.content,
            cfg.splittable_types,
            &mut raw_chunks,
            &file.path.to_string_lossy(),
            file.language,
        );

        if raw_chunks.is_empty() {
            return self.chunk_simple(file);
        }

        let refined = self.refine_large_chunks(raw_chunks);
        self.apply_overlap_ts_style(refined)
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

/// Language configuration for AST chunking
struct LangConfig {
    ts_language: tree_sitter::Language,
    splittable_types: &'static [&'static str],
}

/// Get language configuration for tree-sitter
fn lang_config(lang: mccp_core::Language) -> Option<LangConfig> {
    match lang {
        mccp_core::Language::Rust => Some(LangConfig {
            ts_language: tree_sitter_rust::language(),
            splittable_types: &[
                "function_item", "impl_item", "struct_item",
                "enum_item", "trait_item", "mod_item",
            ],
        }),
        mccp_core::Language::TypeScript | mccp_core::Language::JavaScript => Some(LangConfig {
            ts_language: tree_sitter_typescript::language_typescript(),
            splittable_types: &[
                "function_declaration", "arrow_function", "class_declaration",
                "method_definition", "export_statement",
                "interface_declaration", "type_alias_declaration",
            ],
        }),
        mccp_core::Language::Python => Some(LangConfig {
            ts_language: tree_sitter_python::language(),
            splittable_types: &[
                "function_definition", "class_definition",
                "decorated_definition", "async_function_definition",
            ],
        }),
        mccp_core::Language::Go => Some(LangConfig {
            ts_language: tree_sitter_go::language(),
            splittable_types: &[
                "function_declaration", "method_declaration",
                "type_declaration", "var_declaration", "const_declaration",
            ],
        }),
        mccp_core::Language::Java => Some(LangConfig {
            ts_language: tree_sitter_java::language(),
            splittable_types: &[
                "method_declaration", "class_declaration",
                "interface_declaration", "constructor_declaration",
            ],
        }),
        _ => None,
    }
}

/// Raw chunk before refinement
#[derive(Debug, Clone)]
struct RawChunk {
    content: String,
    start_line: usize,
    end_line: usize,
    start_byte: usize,
    end_byte: usize,
    path: String,
    language: mccp_core::Language,
}

/// Collect splittable nodes from AST
fn collect_splittable_nodes(
    node: Node,
    src: &str,
    splittable: &[&str],
    out: &mut Vec<RawChunk>,
    path: &str,
    lang: mccp_core::Language,
) {
    if splittable.contains(&node.kind()) {
        let text = &src[node.byte_range()];
        if !text.trim().is_empty() {
            out.push(RawChunk {
                content: text.to_string(),
                start_line: node.start_position().row + 1,
                end_line:   node.end_position().row + 1,
                start_byte: node.start_byte(),
                end_byte:   node.end_byte(),
                path:       path.to_string(),
                language:   lang,
            });
        }
        // Do NOT recurse into a matched node — its children are part of this chunk
        return;
    }
    for child in node.children(&mut node.walk()) {
        collect_splittable_nodes(child, src, splittable, out, path, lang);
    }
}

impl Chunker {
    /// Refine oversized chunks by line splitting
    fn refine_large_chunks(&self, chunks: Vec<RawChunk>) -> Vec<RawChunk> {
        let max_chars = self.config.max_tokens * 4; // ~4 chars per token
        let mut out = Vec::new();
        for chunk in chunks {
            if chunk.content.len() <= max_chars {
                out.push(chunk);
            } else {
                out.extend(self.split_large_chunk_by_line(chunk, max_chars));
            }
        }
        out
    }

    /// Split oversized chunk by lines
    fn split_large_chunk_by_line(&self, chunk: RawChunk, max_chars: usize) -> Vec<RawChunk> {
        let mut result = Vec::new();
        let mut current_lines: Vec<&str> = Vec::new();
        let mut current_len = 0;
        let mut start_line = chunk.start_line;

        for (i, line) in chunk.content.lines().enumerate() {
            current_len += line.len() + 1;
            current_lines.push(line);
            if current_len >= max_chars {
                result.push(RawChunk {
                    content:    current_lines.join("\n"),
                    start_line,
                    end_line:   chunk.start_line + i,
                    start_byte: 0, end_byte: 0, // approximate
                    path:       chunk.path.clone(),
                    language:   chunk.language,
                });
                start_line = chunk.start_line + i + 1;
                current_lines.clear();
                current_len = 0;
            }
        }
        if !current_lines.is_empty() {
            result.push(RawChunk {
                content:    current_lines.join("\n"),
                start_line,
                end_line:   chunk.end_line,
                start_byte: 0, end_byte: 0,
                path:       chunk.path.clone(),
                language:   chunk.language,
            });
        }
        result
    }

    /// Apply overlap from previous tail (TypeScript style)
    fn apply_overlap_ts_style(&self, chunks: Vec<RawChunk>) -> Vec<Chunk> {
        let overlap = self.config.overlap_tokens * 4;
        let mut out = Vec::new();
        for (i, raw) in chunks.iter().enumerate() {
            let content = if i > 0 && overlap > 0 {
                let prev = &chunks[i - 1].content;
                let tail = &prev[prev.len().saturating_sub(overlap)..];
                format!("{}\n{}", tail, raw.content)
            } else {
                raw.content.clone()
            };
            out.push(Chunk::new(
                String::new(), // project_id filled by pipeline
                raw.path.clone(),
                content,
                raw.start_byte, raw.end_byte,
                raw.start_line, raw.end_line,
                ChunkScope::Method,
            ));
        }
        out
    }
} // end impl Chunker
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