use super::*;
use mccp_core::*;

/// Summarizer for generating LLM summaries of code chunks
#[derive(Debug, Clone)]
pub struct Summarizer {
    /// LLM provider for generating summaries
    llm_provider: Option<std::sync::Arc<dyn LlmProvider>>,
}

impl Summarizer {
    /// Create a new summarizer
    pub fn new() -> Self {
        Self {
            llm_provider: None,
        }
    }

    /// Set the LLM provider
    pub fn with_provider(mut self, provider: std::sync::Arc<dyn LlmProvider>) -> Self {
        self.llm_provider = Some(provider);
        self
    }

    /// Generate a summary for a code chunk
    pub async fn summarize(&self, chunk: &Chunk) -> Result<String> {
        if let Some(provider) = &self.llm_provider {
            self.generate_summary_with_llm(chunk, provider.as_ref()).await
        } else {
            self.generate_fallback_summary(chunk)
        }
    }

    /// Generate a summary using an LLM provider
    async fn generate_summary_with_llm(&self, chunk: &Chunk, provider: &dyn LlmProvider) -> Result<String> {
        let prompt = self.build_summary_prompt(chunk);
        
        // TODO: Implement LLM provider interface
        // For now, return a placeholder
        Ok(format!("LLM summary for chunk {} in file {}", chunk.id, chunk.file_path))
    }

    /// Generate a fallback summary without LLM
    fn generate_fallback_summary(&self, chunk: &Chunk) -> Result<String> {
        let lines: Vec<&str> = chunk.content.lines().take(5).collect();
        let preview = lines.join("\n");
        
        Ok(format!(
            "Code chunk ({} tokens):\n{}",
            chunk.token_count,
            preview
        ))
    }

    /// Build a prompt for the LLM to generate a summary
    fn build_summary_prompt(&self, chunk: &Chunk) -> String {
        format!(
            r#"Please provide a structured summary of this code chunk:

File: {}
Scope: {}
Content:
{}

Please return a JSON object with the following structure:
{{
  "purpose": "What does this code do?",
  "responsibilities": ["List", "of", "responsibilities"],
  "methods": [
    {{"name": "method_name", "description": "what it does", "complexity": "low|medium|high"}}
  ],
  "variables": ["list", "of", "important", "variables"],
  "dependencies": ["list", "of", "dependencies"],
  "side_effects": ["list", "of", "side", "effects"],
  "endpoints": ["list", "of", "endpoints", "if", "any"],
  "call_sites": ["list", "of", "call", "sites", "if", "any"]
}}"#,
            chunk.file_path,
            chunk.scope.name(),
            chunk.content
        )
    }

    /// Generate summaries for multiple chunks
    pub async fn summarize_batch(&self, chunks: &[Chunk]) -> Result<Vec<String>> {
        let mut summaries = Vec::new();
        
        for chunk in chunks {
            let summary = self.summarize(chunk).await?;
            summaries.push(summary);
        }
        
        Ok(summaries)
    }

    /// Get summary statistics
    pub fn stats(&self, summaries: &[String]) -> SummaryStats {
        let total_chars: usize = summaries.iter().map(|s| s.len()).sum();
        let avg_chars = if summaries.is_empty() { 0 } else { total_chars / summaries.len() };
        let max_chars = summaries.iter().map(|s| s.len()).max().unwrap_or(0);
        let min_chars = summaries.iter().map(|s| s.len()).min().unwrap_or(0);
        
        SummaryStats {
            total_summaries: summaries.len(),
            total_chars,
            avg_chars,
            max_chars,
            min_chars,
        }
    }
}

/// Summary statistics
#[derive(Debug, Clone)]
pub struct SummaryStats {
    pub total_summaries: usize,
    pub total_chars: usize,
    pub avg_chars: usize,
    pub max_chars: usize,
    pub min_chars: usize,
}

/// LLM provider trait for generating summaries
#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    /// Generate text completion
    async fn complete(&self, prompt: &str, schema: Option<&JsonSchema>) -> Result<String>;
    
    /// Stream text completion
    async fn stream(&self, prompt: &str) -> Result<tokio::sync::mpsc::Receiver<String>>;
    
    /// Get provider health status
    async fn health(&self) -> ProviderHealth;
    
    /// Get provider ID
    fn id(&self) -> String;
    
    /// Get provider name
    fn name(&self) -> String;
    
    /// Get provider version
    fn version(&self) -> String;
    
    /// Get supported models
    fn models(&self) -> Vec<String>;
    
    /// Get current model
    fn current_model(&self) -> String;
    
    /// Set model
    fn set_model(&mut self, model: String) -> Result<()>;
    
    /// Download model (for local providers)
    async fn download_model(&self, model: &str) -> Result<()>;
}

/// JSON schema for structured responses
#[derive(Debug, Clone)]
pub struct JsonSchema {
    pub schema: serde_json::Value,
}

impl JsonSchema {
    /// Create a new JSON schema
    pub fn new(schema: serde_json::Value) -> Self {
        Self { schema }
    }
    
    /// Get the schema as a string
    pub fn to_string(&self) -> String {
        serde_json::to_string(&self.schema).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_summarizer_creation() {
        let summarizer = Summarizer::new();
        
        assert!(summarizer.llm_provider.is_none());
    }

    #[test]
    fn test_fallback_summary() {
        let summarizer = Summarizer::new();
        let chunk = Chunk::new(
            "proj_123".to_string(),
            "src/main.rs".to_string(),
            "fn main() {\n    println!(\"hello\");\n}".to_string(),
            0,
            30,
            1,
            3,
            ChunkScope::Function("main".to_string()),
        );
        
        let summary = summarizer.generate_fallback_summary(&chunk).unwrap();
        
        assert!(summary.contains("Code chunk"));
        assert!(summary.contains("fn main()"));
    }

    #[test]
    fn test_build_summary_prompt() {
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
        
        let prompt = summarizer.build_summary_prompt(&chunk);
        
        assert!(prompt.contains("File: src/main.rs"));
        assert!(prompt.contains("Scope: function::main"));
        assert!(prompt.contains("fn main()"));
        assert!(prompt.contains("Please return a JSON object"));
    }

    #[test]
    fn test_summary_stats() {
        let summarizer = Summarizer::new();
        let summaries = vec![
            "Summary 1".to_string(),
            "Summary 2 with more content".to_string(),
            "Short".to_string(),
        ];
        
        let stats = summarizer.stats(&summaries);
        
        assert_eq!(stats.total_summaries, 3);
        assert_eq!(stats.total_chars, 42); // "Summary 1" + "Summary 2 with more content" + "Short"
        assert_eq!(stats.avg_chars, 14);
        assert_eq!(stats.max_chars, 27);
        assert_eq!(stats.min_chars, 5);
    }

    #[test]
    fn test_json_schema() {
        let schema_json = serde_json::json!({
            "type": "object",
            "properties": {
                "purpose": {"type": "string"},
                "responsibilities": {
                    "type": "array",
                    "items": {"type": "string"}
                }
            }
        });
        
        let schema = JsonSchema::new(schema_json);
        let schema_str = schema.to_string();
        
        assert!(schema_str.contains("type"));
        assert!(schema_str.contains("object"));
        assert!(schema_str.contains("properties"));
    }
}