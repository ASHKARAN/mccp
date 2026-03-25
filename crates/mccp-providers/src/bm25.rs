use std::collections::{HashMap, HashSet};
use mccp_core::ScoredChunk;

/// BM25 encoder for sparse vector generation
#[derive(Debug, Clone)]
pub struct BM25Encoder {
    term_to_id:   HashMap<String, u32>,
    doc_freq:     HashMap<u32, u32>,
    total_docs:   u32,
    avg_doc_len:  f32,
    k1: f32,
    b:  f32,
}

impl BM25Encoder {
    /// Create a new BM25 encoder
    pub fn new() -> Self {
        Self {
            term_to_id: HashMap::new(),
            doc_freq: HashMap::new(),
            total_docs: 0,
            avg_doc_len: 0.0,
            k1: 1.2, // BM25 parameters
            b: 0.75,
        }
    }

    /// Tokenize text into terms
    fn tokenize(text: &str) -> Vec<String> {
        text.split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|t| t.len() > 1)
            .map(|t| t.to_lowercase())
            .collect()
    }

    /// Register a document for corpus statistics
    pub fn register_doc(&mut self, text: &str) {
        let tokens = Self::tokenize(text);
        let n = tokens.len() as f32;
        self.avg_doc_len = (self.avg_doc_len * self.total_docs as f32 + n)
                           / (self.total_docs + 1) as f32;
        self.total_docs += 1;
        let unique: HashSet<_> = tokens.iter().collect();
        for t in unique {
            let id = self.term_id(t);
            *self.doc_freq.entry(id).or_default() += 1;
        }
    }

    /// Get or create term ID
    fn term_id(&mut self, term: &str) -> u32 {
        let next = self.term_to_id.len() as u32;
        *self.term_to_id.entry(term.to_string()).or_insert(next)
    }

    /// Encode query into sparse vector
    pub fn encode_query(&self, query: &str) -> Vec<(u32, f32)> {
        let tokens = Self::tokenize(query);
        let mut term_counts = HashMap::new();
        for t in tokens {
            *term_counts.entry(t).or_insert(0) += 1;
        }

        term_counts.into_iter()
            .filter_map(|(term, tf)| {
                let id = *self.term_to_id.get(&term)?;
                let df = *self.doc_freq.get(&id)? as f32;
                let idf = ((self.total_docs as f32 - df + 0.5) / (df + 0.5) + 1.0).ln();
                let score = idf * tf as f32 * (self.k1 + 1.0) / (tf as f32 + self.k1 * (1.0 - self.b + self.b * self.avg_doc_len));
                Some((id, score.max(0.0)))
            })
            .collect()
    }

    /// Encode document into sparse vector
    pub fn encode_doc(&self, text: &str) -> Vec<(u32, f32)> {
        let tokens = Self::tokenize(text);
        let doc_len = tokens.len() as f32;
        let mut term_counts = HashMap::new();
        for t in tokens {
            *term_counts.entry(t).or_insert(0) += 1;
        }

        term_counts.into_iter()
            .filter_map(|(term, tf)| {
                let id = *self.term_to_id.get(&term)?;
                let df = *self.doc_freq.get(&id)? as f32;
                let idf = ((self.total_docs as f32 - df + 0.5) / (df + 0.5) + 1.0).ln();
                let score = idf * tf as f32 * (self.k1 + 1.0) / (tf as f32 + self.k1 * (1.0 - self.b + self.b * doc_len / self.avg_doc_len));
                Some((id, score.max(0.0)))
            })
            .collect()
    }

    /// Get total number of documents
    pub fn total_docs(&self) -> u32 {
        self.total_docs
    }

    /// Get average document length
    pub fn avg_doc_len(&self) -> f32 {
        self.avg_doc_len
    }
}

/// Reciprocal Rank Fusion (RRF) for combining search results
pub fn rrf_fuse(
    dense_results: Vec<ScoredChunk>,
    sparse_results: Vec<ScoredChunk>,
    top_k: usize,
) -> Vec<ScoredChunk> {
    const K: f64 = 60.0;
    let mut scores: HashMap<String, f64> = HashMap::new();
    let mut chunks_map: HashMap<String, ScoredChunk> = HashMap::new();

    // Score dense results
    for (rank, chunk) in dense_results.iter().enumerate() {
        *scores.entry(chunk.chunk_id.clone()).or_default() += 1.0 / (K + rank as f64 + 1.0);
        chunks_map.entry(chunk.chunk_id.clone()).or_insert_with(|| chunk.clone());
    }

    // Score sparse results
    for (rank, chunk) in sparse_results.iter().enumerate() {
        *scores.entry(chunk.chunk_id.clone()).or_default() += 1.0 / (K + rank as f64 + 1.0);
        chunks_map.entry(chunk.chunk_id.clone()).or_insert_with(|| chunk.clone());
    }

    // Sort by RRF score and return top-k
    let mut ranked: Vec<(String, f64)> = scores.into_iter().collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    
    ranked.into_iter()
        .take(top_k)
        .filter_map(|(id, score)| {
            chunks_map.get(&id).map(|c| {
                let mut c = c.clone();
                c.score = score as f32;
                c
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bm25_encoder_creation() {
        let encoder = BM25Encoder::new();
        
        assert_eq!(encoder.total_docs(), 0);
        assert_eq!(encoder.avg_doc_len(), 0.0);
    }

    #[test]
    fn test_bm25_encoder_register_doc() {
        let mut encoder = BM25Encoder::new();
        
        encoder.register_doc("fn main() { println!(\"hello\"); }");
        encoder.register_doc("fn test() { assert_eq!(1 + 1, 2); }");
        
        assert_eq!(encoder.total_docs(), 2);
        assert!(encoder.avg_doc_len() > 0.0);
    }

    #[test]
    fn test_bm25_encoder_encode_query() {
        let mut encoder = BM25Encoder::new();
        encoder.register_doc("fn main() { println!(\"hello\"); }");
        encoder.register_doc("fn test() { assert_eq!(1 + 1, 2); }");
        
        let query_vec = encoder.encode_query("main function");
        
        assert!(!query_vec.is_empty());
        assert!(query_vec.iter().all(|(_, score)| *score >= 0.0));
    }

    #[test]
    fn test_rrf_fuse() {
        let dense_results = vec![
            ScoredChunk {
                chunk_id: "chunk1".to_string(),
                score: 0.9,
                content: "content1".to_string(),
                file_path: "file1".to_string(),
                start_line: 1,
                end_line: 5,
                project_id: "proj1".to_string(),
                metadata: serde_json::Value::Null,
            },
            ScoredChunk {
                chunk_id: "chunk2".to_string(),
                score: 0.8,
                content: "content2".to_string(),
                file_path: "file2".to_string(),
                start_line: 1,
                end_line: 5,
                project_id: "proj1".to_string(),
                metadata: serde_json::Value::Null,
            },
        ];

        let sparse_results = vec![
            ScoredChunk {
                chunk_id: "chunk2".to_string(),
                score: 0.7,
                content: "content2".to_string(),
                file_path: "file2".to_string(),
                start_line: 1,
                end_line: 5,
                project_id: "proj1".to_string(),
                metadata: serde_json::Value::Null,
            },
            ScoredChunk {
                chunk_id: "chunk3".to_string(),
                score: 0.6,
                content: "content3".to_string(),
                file_path: "file3".to_string(),
                start_line: 1,
                end_line: 5,
                project_id: "proj1".to_string(),
                metadata: serde_json::Value::Null,
            },
        ];

        let fused = rrf_fuse(dense_results, sparse_results, 2);
        
        assert_eq!(fused.len(), 2);
        assert!(fused.iter().any(|c| c.chunk_id == "chunk2"));
        assert!(fused.iter().any(|c| c.chunk_id == "chunk1" || c.chunk_id == "chunk3"));
    }
}