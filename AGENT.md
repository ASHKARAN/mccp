# AGENT.md — Code Intelligence MCP Server

---

## Overview

`mccp` is a local-first MCP (Model Context Protocol) server written in Rust. It provides structured code intelligence — semantic search, call graph traversal, LLM-generated summaries, and execution flow tracing — to AI agents. It replaces blind file reading with a queryable intelligence layer that dramatically reduces token usage and improves retrieval accuracy.

All processing is local. All LLM inference runs via Ollama. No data leaves the machine.

---

## Design Principles

- **Local-first** — all indexing, embedding, and inference runs on the user's machine via Ollama and Docker
- **Deterministic outputs** — no randomness in indexing or retrieval pipelines
- **Hybrid intelligence** — embeddings + LLM summaries + graph traversal + exact symbol index, combined at query time
- **Project-isolated** — multiple codebases live in one container, namespaced by project ID; their data never mixes
- **Daemon-capable** — the MCP server runs as a background process; closing the terminal does not kill it
- **Model-aware** — Ollama model lifecycle (download, switch, re-index) is a first-class concern
- **Observable** — structured, filterable logs; metrics endpoint; per-query result quality feedback
- **Incremental** — file changes trigger partial re-index, never full re-index unless forced

---

## System Architecture

```
mccp (CLI / TUI console)
│
├── Daemon Process (background, platform-native: double-fork Unix / Service Win)
│   ├── MCP Server  ←─ AI agents connect here (stdio or HTTP/SSE)
│   │   ├── Tool Layer          (query, get_flow, get_summary, get_related, feedback,
│   │   │                        find_usages, find_definition, get_symbol_map)
│   │   ├── Query Engine        (embed → retrieve → rank → respond)
│   │   │   ├── Query Cache     (LRU, keyed by query hash + project ID + provider fingerprint)
│   │   │   ├── Ranker          (configurable weights: similarity, graph, metadata)
│   │   │   └── Observability   (latency, cache hit rate, indexing lag, feedback log)
│   │   ├── Symbol Resolver     (exact hash lookup into Symbol Store, no LLM cost)
│   │   └── Security Boundary   (project namespace isolation, path ACL)
│   │
│   ├── Indexer Pipeline (async, per-project queue, parallel workers)
│   │   ├── File Watcher        (inotify Linux / FSEvents macOS / ReadDirectoryChanges Win)
│   │   ├── Secrets Scrubber    (redacts keys/tokens before any provider call)
│   │   ├── Parser              (tree-sitter, language-aware, runs in parallel)
│   │   ├── Symbol Extractor    (definitions + references → Symbol Store)
│   │   ├── Chunker             (method-boundary-aware, 512 tokens, 64 overlap)
│   │   ├── Embedder            (→ Embedding Provider interface)
│   │   ├── Summarizer          (→ LLM Provider interface → strict JSON)
│   │   └── Graph Builder       (BFS, cycle-safe, depth-limited)
│   │
│   ├── Provider Registry
│   │   ├── Embedding Provider  (active driver + fallback chain)
│   │   │   ├── OllamaEmbedding     (default, local Docker)
│   │   │   ├── OpenAIEmbedding     (text-embedding-3-small/large)
│   │   │   ├── AzureEmbedding      (Azure OpenAI endpoint)
│   │   │   ├── CohereEmbedding     (embed-english-v3.0)
│   │   │   ├── HuggingFaceEmbedding (TEI Docker image, self-hosted)
│   │   │   └── CustomHttpEmbedding (any OpenAI-compatible /v1/embeddings)
│   │   │
│   │   ├── LLM Provider        (active driver + fallback chain)
│   │   │   ├── OllamaLlm           (default, local Docker)
│   │   │   ├── OpenAILlm           (gpt-4o, gpt-4o-mini)
│   │   │   ├── AzureLlm            (Azure OpenAI endpoint)
│   │   │   ├── AnthropicLlm        (claude-3-5-haiku, claude-3-5-sonnet)
│   │   │   ├── GroqLlm             (llama3, mixtral via Groq API)
│   │   │   ├── VllmLlm             (vLLM Docker image, self-hosted)
│   │   │   └── CustomHttpLlm       (any OpenAI-compatible /v1/chat/completions)
│   │   │
│   │   └── Vector Store Provider (active driver)
│   │       ├── QdrantVector        (default, local Docker)
│   │       ├── PgVectorStore       (PostgreSQL + pgvector extension)
│   │       ├── WeaviateVector      (Weaviate Docker image)
│   │       ├── ChromaVector        (Chroma Docker image)
│   │       └── MemoryVector        (in-process, dev/test only)
│   │
│   └── Docker Manager          (install, start, compose lifecycle, OS-aware)
│
└── Storage Layer  (Docker volume, host path configurable per-service)
    ├── Vector Store    (via Vector Store Provider — default: Qdrant)
    ├── Symbol Store    (inverted index: symbol → [{file, line, col, kind}])
    ├── Graph Store     (adjacency list, project-namespaced, cycle-safe)
    ├── Metadata Store  (file hashes, schema version, provider fingerprint, chunk map)
    ├── Summary Cache   (LLM output, keyed by file_hash + provider fingerprint)
    ├── Query Log       (per-query latency, provider used, hit/miss, quality signals)
    └── Feedback Store  (agent quality signals → ranker tuning data)
```

---

## Project Isolation

All data is namespaced by `project_id` at every storage layer. A single Qdrant collection, graph store, and metadata store serves all registered projects without mixing vectors, summaries, or graph edges between them.

```
project_id = sha256(canonical_project_root)[0..12]   # stable, derived, not user-supplied
```

Every Qdrant payload includes `{ "project_id": "...", "workspace": "..." }` for hard-partition filtering. Graph nodes carry the same field. Summary cache keys are prefixed `{project_id}::{file_hash}`. Queries are always scoped: the ranker will never surface results from a different project regardless of vector similarity.

---

## MCP Tools (Agent-Facing API)

All tools are semantic and high-level. Agents must use these tools — never raw file access.

### `query`
Semantic search across the entire project codebase.
```json
{
  "project": "my-api",
  "query": "how is the user authentication token validated",
  "filters": {
    "language": "rust",
    "file_pattern": "src/auth/**",
    "scope": "method"
  },
  "top_k": 10
}
```

### `get_file`
Returns full file content, with metadata (last indexed, chunk count, language).
```json
{ "project": "my-api", "path": "src/auth/validator.rs" }
```

### `get_summary`
Returns LLM-generated structured summary for a file or class.
```json
{ "project": "my-api", "path": "src/auth/validator.rs", "scope": "class::TokenValidator" }
```

### `get_related`
Returns related files and classes via graph traversal.
```json
{ "project": "my-api", "path": "src/auth/validator.rs", "depth": 2 }
```

### `get_flow`
Returns the full execution flow from an entry point.
```json
{ "project": "my-api", "entry": "UserController::login", "max_depth": 5 }
```

### `search_flow`
Finds an execution path across multiple files using semantic + graph.
```json
{ "project": "my-api", "from": "HTTP POST /login", "to": "database::users::find_by_email" }
```

### `feedback`
Signals result quality back to the ranker. Used to tune weights over time.
```json
{ "project": "my-api", "query_id": "q_abc123", "signal": "good" | "bad" | "irrelevant" }
```

### `find_usages`
Returns every location in the codebase where a symbol is referenced, grouped by file. Uses the Symbol Store — exact lookup, no embedding, sub-millisecond response.
```json
{
  "project": "my-api",
  "symbol": "UserService",
  "symbol_kind": "class",          // optional filter: class|method|variable|interface|type|enum|const
  "ref_kind": ["call", "import"],  // optional filter — omit to return all reference kinds
  "file_pattern": "src/**"         // optional glob filter
}
```

Response:
```json
{
  "symbol": "UserService",
  "symbol_kind": "class",
  "total_references": 14,
  "definition": {
    "file": "src/services/user.service.ts",
    "line": 12,
    "col": 14,
    "context": "export class UserService {"
  },
  "usages": [
    {
      "file": "src/controllers/auth.controller.ts",
      "references": [
        {
          "line": 3,
          "col": 8,
          "ref_kind": "import",
          "context": "import { UserService } from '../services/user.service';",
          "containing_scope": "module"
        },
        {
          "line": 18,
          "col": 22,
          "ref_kind": "type_annotation",
          "context": "constructor(private userService: UserService) {}",
          "containing_scope": "AuthController::constructor"
        }
      ]
    },
    {
      "file": "src/controllers/user.controller.ts",
      "references": [
        {
          "line": 5,
          "col": 8,
          "ref_kind": "import",
          "context": "import { UserService } from '../services/user.service';",
          "containing_scope": "module"
        }
      ]
    }
  ]
}
```

### `find_definition`
Returns the canonical declaration site for a symbol — where it is defined, not where it is used.
```json
{
  "project": "my-api",
  "symbol": "validateToken",
  "scope_hint": "AuthService"   // optional: disambiguates overloaded names
}
```

Response:
```json
{
  "symbol": "validateToken",
  "symbol_kind": "method",
  "file": "src/services/auth.service.ts",
  "line": 44,
  "col": 2,
  "context": "async validateToken(token: string): Promise<User> {",
  "containing_scope": "AuthService",
  "qualified_name": "AuthService.validateToken"
}
```

### `get_symbol_map`
Returns all symbols defined in a file — classes, methods, variables, interfaces, types, enums — with their line numbers. Gives an AI agent a full structural outline of a file in one call, without reading the entire source.
```json
{
  "project": "my-api",
  "path": "src/services/user.service.ts"
}
```

Response:
```json
{
  "file": "src/services/user.service.ts",
  "symbols": [
    { "name": "UserService",        "kind": "class",    "line": 12, "exported": true },
    { "name": "constructor",        "kind": "method",   "line": 18, "scope": "UserService" },
    { "name": "findById",           "kind": "method",   "line": 24, "scope": "UserService" },
    { "name": "createUser",         "kind": "method",   "line": 38, "scope": "UserService" },
    { "name": "updateEmail",        "kind": "method",   "line": 57, "scope": "UserService" },
    { "name": "deleteUser",         "kind": "method",   "line": 72, "scope": "UserService" },
    { "name": "DEFAULT_ROLE",       "kind": "const",    "line": 8,  "exported": true },
    { "name": "UserNotFoundException", "kind": "class", "line": 6,  "exported": true }
  ]
}
```

### `rename_preview`
Dry-run rename: returns every file and line that would need to change if a symbol were renamed. Does not modify any files — provides the complete change set for the AI agent to act on.
```json
{
  "project": "my-api",
  "symbol": "UserService",
  "new_name": "AccountService",
  "symbol_kind": "class"
}
```

Response:
```json
{
  "symbol": "UserService",
  "new_name": "AccountService",
  "files_affected": 6,
  "total_changes": 14,
  "changes": [
    {
      "file": "src/services/user.service.ts",
      "changes": [
        { "line": 12, "col": 14, "ref_kind": "definition",
          "before": "export class UserService {",
          "after":  "export class AccountService {" }
      ]
    },
    {
      "file": "src/controllers/auth.controller.ts",
      "changes": [
        { "line": 3,  "col": 8,  "ref_kind": "import",
          "before": "import { UserService } from '../services/user.service';",
          "after":  "import { AccountService } from '../services/user.service';" },
        { "line": 18, "col": 22, "ref_kind": "type_annotation",
          "before": "constructor(private userService: UserService) {}",
          "after":  "constructor(private userService: AccountService) {}" }
      ]
    }
  ]
}
```

---

## Symbol Index

### Architecture

The Symbol Index is a **separate, parallel data structure** built alongside the vector index. It is not stored in Qdrant and requires no embedding model. It is a deterministic inverted index: a hash map from `(project_id, symbol_name) → [SymbolRecord]`.

This gives AI agents IDE-quality exact lookup that is:
- **Zero LLM cost** — pure tree-sitter AST extraction, no inference
- **Sub-millisecond** — hash map lookup, not ANN search
- **Exact** — no false positives from semantic similarity
- **Line-precise** — returns file path, line number, column, and context snippet
- **Complementary to `query`** — `query` handles "find me code that does X" (semantic), `find_usages` handles "find every place Y is used" (exact)

### When agents should use `find_usages` vs `query`

| Task | Use |
|------|-----|
| Find code that implements login | `query` — semantic intent |
| Find all files that import `UserService` | `find_usages` — exact symbol |
| Understand what `AuthController` does | `get_summary` — LLM summary |
| Find every call site of `validateToken` | `find_usages` — exact symbol |
| Rename `UserService` to `AccountService` | `rename_preview` then edit |
| Find all classes that extend `BaseRepository` | `find_usages` with `ref_kind: inheritance` |
| Understand how authentication flows | `get_flow` — graph traversal |
| Find all usages of the `role` field on `User` | `find_usages` with `symbol: role`, `symbol_kind: variable` |

### Symbol Record Schema

```rust
struct SymbolRecord {
    project_id:        String,       // project namespace
    symbol_name:       String,       // unqualified name: "UserService"
    qualified_name:    String,       // fully qualified: "AuthModule.UserService"
    symbol_kind:       SymbolKind,   // class | method | variable | interface | type | enum | const
    ref_kind:          RefKind,      // definition | call | import | type_annotation |
                                     // inheritance | field_access | assignment | export
    file_path:         String,       // relative to project root
    line:              u32,
    col:               u32,
    context_snippet:   String,       // the full source line, trimmed
    containing_scope:  String,       // "AuthService.constructor" or "module"
    language:          String,       // "typescript" | "rust" | "python" | ...
}
```

### Extraction Rules (per language)

The Symbol Extractor runs tree-sitter queries for each supported language. The output schema is identical regardless of language; only the AST queries differ.

**What gets extracted as a definition:**
- Class / struct / interface / trait declarations
- Function and method declarations
- Variable and constant declarations (`const`, `let`, `var`, `val`, static fields)
- Type aliases and enum declarations
- Module and namespace declarations

**What gets extracted as a reference:**
- Import / `use` / `require` statements (ref_kind: `import`)
- Function call expressions (ref_kind: `call`)
- Type annotations in function signatures, variable declarations (ref_kind: `type_annotation`)
- Class inheritance / interface implementation (ref_kind: `inheritance`)
- Field access expressions: `obj.field` (ref_kind: `field_access`)
- Assignment targets (ref_kind: `assignment`)
- Re-exports (ref_kind: `export`)

### Name Resolution and Disambiguation

Many codebases have multiple symbols with the same short name (e.g., `create` appears in 30 services). The Symbol Index handles this at three levels:

1. **Exact qualified match** — if the agent supplies `AuthService.validateToken`, the lookup is scoped to that class. Always prefer this.
2. **Scope hint** — if `scope_hint` is provided to `find_definition` or `find_usages`, results are ranked by proximity to that scope.
3. **Fuzzy unqualified** — if only `validateToken` is provided, all definitions named `validateToken` are returned with their qualified names, and the agent must disambiguate using `scope_hint` on a follow-up call.

The recommended agent workflow for ambiguous symbols:
```
1. find_definition("validateToken")               → returns N candidates with qualified names
2. find_usages("AuthService.validateToken")       → scoped exact lookup on chosen candidate
```

### Symbol Store Storage

The Symbol Store is backed by a persistent key-value store (RocksDB or SQLite, configurable). It is stored in the same data directory as other stores, partitioned by `project_id`.

```
<data-dir>/mccp/symbols/
  {project_id}.db    # RocksDB or SQLite, one file per project
```

The index keys are:
- Forward index: `{project_id}:sym:{symbol_name}` → `[SymbolRecord]` (all locations for a symbol)
- File index: `{project_id}:file:{file_path}` → `[SymbolRecord]` (all symbols in a file, for `get_symbol_map`)
- Qualified index: `{project_id}:qual:{qualified_name}` → `SymbolRecord` (single definition lookup)

### Incremental Updates

When a file changes, the Symbol Extractor:
1. Deletes all existing `SymbolRecord` entries for that file from all index keys
2. Re-parses the file with tree-sitter
3. Inserts the new set of records

This is safe because the Symbol Store is always rebuilt from source — it is never the source of truth, only a derived index. A corrupted Symbol Store can always be fully regenerated with `mccp index --reset`.

### Supported Languages (tree-sitter grammars)

| Language | Grammar |
|----------|---------|
| TypeScript / JavaScript | `tree-sitter-typescript` |
| Rust | `tree-sitter-rust` |
| Python | `tree-sitter-python` |
| Java | `tree-sitter-java` |
| Go | `tree-sitter-go` |
| C / C++ | `tree-sitter-c` / `tree-sitter-cpp` |
| C# | `tree-sitter-c-sharp` |
| Ruby | `tree-sitter-ruby` |
| PHP | `tree-sitter-php` |
| Kotlin | `tree-sitter-kotlin` |

Additional languages can be added by implementing a `SymbolExtractor` trait and registering a tree-sitter grammar. The output schema is fixed; only the AST query patterns differ per language.

---

## Retrieval Pipeline

```
1.  Receive query (project_id, query_text, filters)
2.  Check query cache  →  cache hit: return immediately
3.  Embed query via Ollama (embedding model)
4.  Retrieve top-K candidates from Qdrant (filtered by project_id)
5.  Load summaries from cache (lazy — disk only if not in RAM)
6.  Score each candidate:
      final_score = w_sim  × cosine_similarity
                  + w_graph × graph_centrality_score
                  + w_meta  × metadata_relevance_score
    (weights are configurable per-project; default: 0.6, 0.25, 0.15)
7.  Re-rank top results using cross-encoder (optional, Ollama rerank model)
8.  Store result + latency in query log
9.  Return structured response — load full code only if explicitly requested
```

---

## Indexing Pipeline

```
1.  File watcher detects change (inotify/FSEvents)
2.  Compute SHA-256 hash of file → compare to metadata store
3.  If unchanged: skip
4.  Secrets scan: redact API keys, tokens, passwords from content before LLM/embed
5.  Parse with tree-sitter: extract classes, methods, imports, call sites
6.  Chunk at method boundaries:
      - max 512 tokens per chunk
      - 64-token overlap between adjacent chunks
      - never split a method across chunks
      - oversized methods get sub-chunked by logical block
7.  Batch-embed all chunks via Ollama (embedding model)
8.  Generate LLM summary (chat model → strict JSON)
9.  Update graph edges (caller → callee, import → dependency)
10. Persist to Qdrant, graph store, metadata store
11. Update file hash + model version in metadata
```

### Chunking Spec (non-negotiable)

| Parameter        | Value                         |
|------------------|-------------------------------|
| Max tokens       | 512                           |
| Overlap          | 64 tokens                     |
| Boundary rule    | Never split across a method   |
| Oversized method | Sub-chunk by logical block    |
| Scope levels     | project → module → file → class → method |

---

## LLM Summary Schema

Every file and class is summarized once. Summaries are stored with model metadata and invalidated on model change or code change.

```json
{
  "_meta": {
    "schema_version": 2,
    "model_id": "codellama:13b",
    "indexed_at": "2025-03-24T10:00:00Z",
    "file_hash": "abc123..."
  },
  "purpose": "",
  "responsibilities": [],
  "methods": [
    { "name": "", "description": "", "complexity": "low|medium|high" }
  ],
  "variables": [],
  "dependencies": [],
  "side_effects": [],
  "endpoints": [],
  "call_sites": []
}
```

When the active model changes (see Model Manager), all summaries for that project are marked stale and re-queued asynchronously. Queries during re-indexing fall back to the stale summary with a staleness flag in the response.

---

## Graph Engine

### Construction

```
- Method calls:         A.foo() → B.bar()
- Class usage:          Controller → Service → Repository
- Import graph:         file A imports file B
- Inheritance:          class A extends B
- DTO usage:            CreateUserDto → UserService.create()
```

### Traversal Rules

- Algorithm: BFS from root node
- Default max depth: 3 hops (configurable per query, max 8)
- Cycle detection: visited-set per traversal (never re-visit a node in one traversal)
- Node ranking: degree centrality — nodes with more edges rank higher in `get_related`
- Disconnected nodes: returned with graph score 0.0 (not excluded)

### Architectural Analysis

The graph engine computes these automatically at index time:

- **Entry points**: methods with in-degree 0 (nothing calls them) — likely handlers, jobs, or CLI commands
- **Bottlenecks**: methods with high betweenness centrality — changes here have wide impact
- **Leaf nodes**: methods with out-degree 0 — likely pure functions or I/O calls

---

## Provider Abstraction Layer

The embedding, LLM, and vector store layers are fully pluggable. The default stack (Qdrant + Ollama, both local Docker) works out of the box with no configuration. Every provider is swappable independently via config file, environment variables, or CLI — without changing any other part of the system.

### Provider Interface Contract

All providers implement a thin async Rust trait. The rest of the system only speaks the trait — never a concrete driver.

```rust
// Embedding provider
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
    async fn embed_one(&self, text: &str) -> Result<Vec<f32>>;
    fn dimensions(&self) -> usize;
    fn provider_fingerprint(&self) -> String;  // used to detect stale indexes
    async fn health(&self) -> ProviderHealth;
}

// LLM provider
#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, prompt: &str, schema: Option<&JsonSchema>) -> Result<String>;
    async fn stream(&self, prompt: &str) -> Result<BoxStream<String>>;
    fn provider_fingerprint(&self) -> String;
    async fn health(&self) -> ProviderHealth;
}

// Vector store provider
#[async_trait]
pub trait VectorStoreProvider: Send + Sync {
    async fn upsert(&self, project_id: &str, chunks: &[EmbeddedChunk]) -> Result<()>;
    async fn search(&self, project_id: &str, query: &[f32], top_k: usize, filters: &Filters) -> Result<Vec<ScoredChunk>>;
    async fn delete_project(&self, project_id: &str) -> Result<()>;
    async fn health(&self) -> ProviderHealth;
}
```

`provider_fingerprint()` returns a stable string encoding the provider type + model name + endpoint hash. It is stored in the metadata alongside every index entry. When the fingerprint changes, affected projects are auto-queued for re-index.

### Provider Registry

The active provider for each slot is resolved at startup using this priority order:

```
1. Environment variable   (highest priority — overrides everything)
2. Project-level config   (~/.mccp/projects/<id>/config.toml)
3. Global config          (~/.mccp/config.toml)
4. Built-in default       (Ollama + Qdrant — lowest priority)
```

### Supported Providers

#### Embedding Providers

| Driver key | Description | Protocol |
|---|---|---|
| `ollama` | Local Ollama Docker (default) | Ollama REST API |
| `openai` | OpenAI text-embedding-3-* | OpenAI `/v1/embeddings` |
| `azure-openai` | Azure OpenAI deployment | Azure OpenAI REST |
| `cohere` | Cohere embed-english-v3.0 | Cohere REST API |
| `huggingface-tei` | HuggingFace TEI Docker image | OpenAI-compatible REST |
| `custom-http` | Any OpenAI-compatible endpoint | OpenAI `/v1/embeddings` |

#### LLM Providers

| Driver key | Description | Protocol |
|---|---|---|
| `ollama` | Local Ollama Docker (default) | Ollama REST API |
| `openai` | GPT-4o, GPT-4o-mini | OpenAI `/v1/chat/completions` |
| `azure-openai` | Azure OpenAI deployment | Azure OpenAI REST |
| `anthropic` | Claude 3.5 Haiku / Sonnet | Anthropic Messages API |
| `groq` | Llama3, Mixtral via Groq | OpenAI-compatible REST |
| `vllm` | vLLM Docker self-hosted | OpenAI-compatible REST |
| `custom-http` | Any OpenAI-compatible endpoint | OpenAI `/v1/chat/completions` |

#### Vector Store Providers

| Driver key | Description | Notes |
|---|---|---|
| `qdrant` | Qdrant Docker (default) | Recommended for production |
| `pgvector` | PostgreSQL + pgvector | Use existing Postgres instance |
| `weaviate` | Weaviate Docker | Alternative managed option |
| `chroma` | Chroma Docker | Lightweight alternative |
| `memory` | In-process HashMap | Dev/test only — no persistence |

### Provider Fallback Chain

Each provider slot supports an ordered fallback chain. If the primary provider fails a health check at startup, `mccp` tries the next in the chain and logs a warning.

```toml
[embedding]
providers = [
  { driver = "ollama", model = "nomic-embed-text" },
  { driver = "openai", model = "text-embedding-3-small" },  # fallback if Ollama down
]
```

Fallback is for startup resilience only — mid-query provider switching does not happen. If the active provider fails during a query, the query returns an error with the provider failure message rather than silently switching and returning results indexed with a different model.

### Environment Variable Configuration

Every provider setting can be set via environment variable. This is the recommended approach for CI, containers, and shared team setups. Env vars take full priority over config files.

```bash
# Embedding provider
MCCP_EMBEDDING_PROVIDER=openai
MCCP_EMBEDDING_MODEL=text-embedding-3-small
MCCP_EMBEDDING_URL=https://api.openai.com/v1
MCCP_EMBEDDING_API_KEY=sk-...

# LLM provider
MCCP_LLM_PROVIDER=anthropic
MCCP_LLM_MODEL=claude-3-5-haiku-20241022
MCCP_LLM_URL=https://api.anthropic.com
MCCP_LLM_API_KEY=sk-ant-...

# Vector store
MCCP_VECTOR_PROVIDER=qdrant
MCCP_VECTOR_URL=http://localhost:6333
MCCP_VECTOR_API_KEY=                        # optional, for cloud Qdrant

# Custom HTTP (OpenAI-compatible) — overrides provider-specific vars
MCCP_EMBEDDING_URL=http://my-embed-server:8080/v1
MCCP_LLM_URL=http://my-llm-server:8000/v1

# Ollama-specific
MCCP_OLLAMA_HOST=http://localhost:11434
```

### Provider Fingerprint & Re-index Trigger

Every file indexed carries the provider fingerprint in its metadata entry:

```
fingerprint = sha256("{provider_type}:{model_name}:{endpoint_host}:{dimensions}")
```

When the active provider changes (via CLI, env var, or config edit), `mccp` computes the new fingerprint at startup. Projects where any chunk has a different fingerprint are queued for re-index. Re-index runs in the background — queries continue to be served from stale data (flagged with `stale: true`) until the project is fully re-indexed with the new provider.

### Model Manager (Ollama-specific)

When the active embedding or LLM provider is `ollama`, `mccp` manages the full Ollama model lifecycle. No manual `ollama pull` required.

```toml
# ~/.mccp/models.toml (auto-managed, Ollama only)

[ollama.embedding]
active   = "nomic-embed-text"
fallback = "all-minilm"

[ollama.chat]
active   = "codellama:13b"
fallback = "codellama:7b"

[ollama.rerank]
active   = ""   # optional: mxbai-rerank-base-v1
```

When `active` differs from the fingerprint stored in project metadata, the project is flagged for re-index identical to any other provider change.

---

## CLI Reference

`mccp` is the single entry point. All commands support `--project <name|path>` to scope to a specific codebase.

### Daemon

```bash
mccp start                    # start daemon (MCP server + indexer) in background
mccp stop                     # gracefully stop daemon
mccp restart                  # stop + start
mccp status                   # show daemon health, uptime, active projects, queue depth
mccp daemon logs              # tail daemon logs (see Logging)
```

### Project Management

```bash
mccp project add <path> [--name <alias>]     # register a codebase
mccp project remove <name>                   # deregister (data preserved unless --purge)
mccp project list                            # list all registered projects + status
mccp project info <name>                     # show index stats, model versions, last updated
mccp project set-default <name>              # set default project for CLI commands
```

### Indexing

```bash
mccp index                           # index default project (incremental)
mccp index --project <name>          # index specific project
mccp index --full                    # force full re-index (ignores file hashes)
mccp index --reset                   # wipe all data for project and re-index from scratch
mccp index status                    # show queue depth, current file, ETA
mccp index pause                     # pause indexer (queries still served from stale data)
mccp index resume                    # resume indexer
mccp index watch                     # enable/disable filesystem watcher for auto-index
```

### Provider Management

```bash
# Show active provider configuration for all slots
mccp provider status

# Set the embedding provider (queues re-index for all projects)
mccp provider set embed ollama --model nomic-embed-text
mccp provider set embed openai --model text-embedding-3-small --api-key sk-...
mccp provider set embed azure-openai --url https://my.openai.azure.com --deployment my-embed --api-key ...
mccp provider set embed huggingface-tei --url http://localhost:8080
mccp provider set embed custom-http --url http://my-server:8080/v1 --api-key ...

# Set the LLM provider (queues re-index for all projects — summaries regenerated)
mccp provider set llm ollama --model codellama:13b
mccp provider set llm openai --model gpt-4o-mini --api-key sk-...
mccp provider set llm anthropic --model claude-3-5-haiku-20241022 --api-key sk-ant-...
mccp provider set llm groq --model llama3-8b-8192 --api-key gsk_...
mccp provider set llm vllm --url http://localhost:8000/v1 --model codellama
mccp provider set llm custom-http --url http://my-llm:8000/v1 --model mymodel

# Set the vector store provider
mccp provider set vector qdrant --url http://localhost:6333
mccp provider set vector pgvector --url postgresql://user:pass@localhost:5432/mccp
mccp provider set vector weaviate --url http://localhost:8080
mccp provider set vector memory          # dev/test only

# Per-project provider override (overrides global config for this project only)
mccp provider set embed openai --project my-api --model text-embedding-3-large

# Test connectivity to a provider before committing
mccp provider test embed
mccp provider test llm
mccp provider test vector

# Reset a slot back to the default (Ollama or Qdrant)
mccp provider reset embed
mccp provider reset llm
mccp provider reset vector

# List all configured providers including fingerprints
mccp provider list
```

### Model Management (Ollama only)

```bash
mccp model list                      # list all downloaded Ollama models
mccp model list --available          # list recommended models for mccp use cases
mccp model pull <model>              # download a model via Ollama
mccp model remove <model>            # delete a model from Ollama
mccp model use embed <model>         # set active embedding model (queues re-index)
mccp model use chat <model>          # set active chat/summary model (queues re-index)
mccp model use rerank <model>        # set active rerank model (optional)
mccp model status                    # show active models, Ollama health, VRAM usage
mccp model pull --recommended        # pull the full recommended model set for mccp
```

### Query & Debug

```bash
mccp query "<text>" [--project <name>] [--top-k 10]     # run a semantic query
mccp flow "<entry point>" [--depth 3]                    # trace execution flow
mccp summary <file path>                                 # show LLM summary for a file
mccp related <file path> [--depth 2]                     # show related files via graph
mccp search "<text>" --mode hybrid|semantic|graph        # explicit mode override
```

### Docker Management

```bash
mccp docker install                  # install Docker + Compose (prompts for sudo if needed)
mccp docker status                   # show container health, ports, volumes
mccp docker start                    # start all mccp containers (Qdrant, Ollama)
mccp docker stop                     # stop all mccp containers
mccp docker restart                  # restart all mccp containers
mccp docker reset                    # wipe all containers and volumes (destructive!)
mccp docker set-data-dir <path>      # set Docker volume host path for all persistent data
mccp docker logs [--service <name>]  # stream container logs
mccp docker upgrade                  # pull latest images and restart
```

### Logging

```bash
mccp logs                            # tail all logs (pretty formatted)
mccp logs --level debug|info|warn|error
mccp logs --project <name>           # filter to one project
mccp logs --component indexer|query|graph|model|docker
mccp logs --since "10m" | "1h" | "2024-01-01"
mccp logs --json                     # raw JSON for piping to jq / external systems
mccp logs --query-id <id>            # full trace for a single query
mccp logs export [--since "24h"]     # write filtered logs to file
```

### Console (TUI)

```bash
mccp console                         # open the interactive TUI console
```

The TUI console (`mccp console`) is a full-screen interactive dashboard. It is the recommended interface for day-to-day management.

---

## TUI Console

The interactive console uses a Claude-CLI / GitHub Copilot CLI visual style: minimal chrome, keyboard-driven, high information density.

### Layout

```
┌─────────────────────────────────────────────────────────────────────┐
│  mccp  •  daemon: running  •  3 projects  •  queue: 0             ↑  │
├────────────┬────────────────────────────────────────────────────────┤
│  PROJECTS  │  PROJECT: my-api                                        │
│            │                                                         │
│ ▶ my-api   │  Status      indexed (up-to-date)                       │
│   shop-svc │  Files       1,842 indexed  •  0 pending                │
│   ml-core  │  Chunks      24,601                                     │
│            │  Last index  2 min ago                                  │
│            │  Embed model nomic-embed-text  ✓ current                │
│            │  Chat model  codellama:13b    ✓ current                 │
│            │                                                         │
│            │  ─── Recent Queries ──────────────────────────────────  │
│            │  12:04  "how is auth token validated"   142ms  ✓ good   │
│            │  12:01  "user creation flow"             98ms           │
│            │  11:58  "find payment retry logic"      201ms  ✗ bad   │
│            │                                                         │
│            │  ─── Index Queue ─────────────────────────────────────  │
│            │  idle                                                   │
│            │                                                         │
│            │  ─── Actions ─────────────────────────────────────────  │
│            │  [i] index  [r] reset  [s] stop  [l] logs  [q] back    │
├────────────┴────────────────────────────────────────────────────────┤
│  MODELS   nomic-embed-text  •  codellama:13b  •  Ollama: healthy    │
│  DOCKER   qdrant: up  •  ollama: up  •  data: /var/mccp/data        │
└─────────────────────────────────────────────────────────────────────┘
```

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `↑ ↓` | Navigate project list |
| `i` | Trigger incremental index for selected project |
| `I` | Trigger full re-index for selected project |
| `r` | Reset + re-index selected project |
| `s` / `S` | Stop / Start MCP server for selected project |
| `l` | Open log viewer for selected project |
| `m` | Open model manager panel |
| `d` | Open Docker status panel |
| `q` | Back / Quit |
| `/` | Filter log stream |
| `?` | Help overlay |

---

## Daemon & Background Operation

The daemon runs as a detached background process via a process supervisor. Closing the terminal does not stop the MCP server or indexer.

### Process Model

```
mccp start
  → writes PID to ~/.mccp/daemon.pid
  → redirects stdout/stderr to ~/.mccp/logs/daemon.log
  → daemonizes using double-fork (Unix) or Windows Service wrapper
  → MCP server binds to stdio (for AI agent clients) + HTTP :7422 (for CLI/TUI)
  → indexer threads start per registered project
  → file watchers attach to all registered project roots
```

### Health Check

```bash
mccp status
# output:
# daemon      running   PID 18423   uptime 3h 12m
# mcp-server  listening  stdio + HTTP :7422
# indexer     idle       queue: 0
# qdrant      healthy    :6333
# ollama      healthy    :11434
```

The daemon exposes a local HTTP health endpoint: `GET http://localhost:7422/health`

---

## Logging

All logs are structured JSON internally. The CLI renders them as human-readable output with color coding. Logs are written to `~/.mccp/logs/` by default, rotated daily, retained 14 days.

### Log Schema

```json
{
  "ts":        "2025-03-24T12:04:11.423Z",
  "level":     "info",
  "component": "query_engine",
  "project":   "my-api",
  "query_id":  "q_abc123",
  "msg":       "query completed",
  "latency_ms": 142,
  "cache_hit":  false,
  "top_k":      10,
  "model":      "nomic-embed-text"
}
```

### Log Levels

| Level | Used for |
|-------|----------|
| `error` | unrecoverable failures, data corruption risks |
| `warn`  | degraded mode, stale results, model mismatch |
| `info`  | query lifecycle, index events, model changes |
| `debug` | chunking decisions, graph traversal steps, cache ops |
| `trace` | full embedding payloads, Ollama request/response |

### Filterable Dimensions

Every log entry carries at minimum: `level`, `component`, `project`, `query_id` (when in a query context). The `mccp logs` command filters on any combination of these fields.

---

## Docker Integration

`mccp` manages its own Docker environment. Users do not need to interact with Docker directly.

### Managed Services (Default Stack)

| Service | Image | Default Port | Purpose | Optional |
|---------|-------|-------------|---------|----------|
| `qdrant` | `qdrant/qdrant` | 6333 | Vector database (default provider) | Yes — swap for pgvector, Weaviate, Chroma |
| `ollama` | `ollama/ollama` | 11434 | LLM + embedding inference (default provider) | Yes — swap for OpenAI, Anthropic, vLLM, etc. |

When a provider is switched to an external service (e.g. OpenAI), `mccp` stops managing the corresponding Docker container and removes it from `docker-compose.yml`. The `mccp docker status` command always shows which containers are managed vs which providers are external.

### Auto-Install Behavior (`mccp docker install`)

1. Detect OS (Linux / macOS / Windows WSL2)
2. Check if Docker is installed → if not, download and run the official install script
3. On Linux: add current user to the `docker` group (requires sudo — prompts with explanation)
4. Verify Docker Compose v2 is available → install plugin if missing
5. Write `~/.mccp/docker-compose.yml` with `qdrant` and `ollama` services
6. Set data directory to `~/.mccp/data` (overridable with `mccp docker set-data-dir`)
7. Start services and confirm health

When sudo is required, `mccp` explains exactly what it needs and why before prompting:

```
mccp needs to add your user to the 'docker' group so you can run Docker
without sudo. This requires one-time root access.

  sudo usermod -aG docker $USER

Enter your password to continue (or Ctrl+C to cancel):
```

After adding the user to the `docker` group, `mccp` reminds the user to log out and back in (or runs `newgrp docker` to activate in the current session).

### Data Directory

All persistent data lives under a single configurable host directory, mounted as a Docker volume.

```bash
mccp docker set-data-dir /mnt/fast-ssd/mccp-data
```

This updates `docker-compose.yml` and restarts the affected services. The directory layout:

```
<data-dir>/
├── qdrant/        # Qdrant storage (vectors, payloads, indexes)
├── ollama/        # Ollama model weights
└── mccp/          # Summaries, graph store, metadata, logs, query log
```

---

## Configuration

Config file: `~/.mccp/config.toml`

Environment variables take full priority over all config file values. Both are shown below with their correspondence.

```toml
[daemon]
http_port       = 7422               # env: MCCP_HTTP_PORT
log_level       = "info"             # env: MCCP_LOG_LEVEL
log_dir         = "~/.mccp/logs"     # env: MCCP_LOG_DIR
log_retention   = "14d"

[indexer]
watch_enabled       = true
max_chunk_tokens    = 512
chunk_overlap       = 64
batch_size          = 32             # embedding batch size — tune per GPU/CPU
parallel_workers    = 0              # 0 = auto (num_cpus); env: MCCP_INDEXER_WORKERS
secrets_scan        = true
io_buffer_kb        = 256            # read buffer per file — larger = fewer syscalls
mmap_threshold_kb   = 512            # files larger than this are mmap'd, not read()

# ─── Embedding Provider ────────────────────────────────────────────────────────
# env: MCCP_EMBEDDING_PROVIDER, MCCP_EMBEDDING_MODEL, MCCP_EMBEDDING_URL,
#      MCCP_EMBEDDING_API_KEY, MCCP_EMBEDDING_DIMENSIONS
[embedding]
providers = [
  { driver = "ollama",  model = "nomic-embed-text", url = "http://localhost:11434" },
  # fallback examples (uncomment to enable):
  # { driver = "openai",  model = "text-embedding-3-small", api_key_env = "OPENAI_API_KEY" },
  # { driver = "custom-http", url = "http://my-embed:8080/v1", model = "my-model" },
]
dimensions        = 768              # must match model output — validated at startup
request_timeout_s = 30
max_retries       = 3

# ─── LLM Provider ─────────────────────────────────────────────────────────────
# env: MCCP_LLM_PROVIDER, MCCP_LLM_MODEL, MCCP_LLM_URL, MCCP_LLM_API_KEY
[llm]
providers = [
  { driver = "ollama", model = "codellama:13b", url = "http://localhost:11434" },
  # { driver = "openai",     model = "gpt-4o-mini",                  api_key_env = "OPENAI_API_KEY"    },
  # { driver = "anthropic",  model = "claude-3-5-haiku-20241022",    api_key_env = "ANTHROPIC_API_KEY" },
  # { driver = "groq",       model = "llama3-8b-8192",               api_key_env = "GROQ_API_KEY"      },
  # { driver = "vllm",       model = "codellama", url = "http://localhost:8000/v1" },
  # { driver = "custom-http", url = "http://my-llm:8000/v1", model = "mymodel" },
]
max_tokens        = 2048
request_timeout_s = 120
max_retries       = 2

# ─── Vector Store Provider ────────────────────────────────────────────────────
# env: MCCP_VECTOR_PROVIDER, MCCP_VECTOR_URL, MCCP_VECTOR_API_KEY
[vector]
driver            = "qdrant"         # qdrant | pgvector | weaviate | chroma | memory
url               = "http://localhost:6333"
api_key           = ""               # env: MCCP_VECTOR_API_KEY (for cloud Qdrant etc.)
request_timeout_s = 10
# Qdrant HNSW tuning (ignored for other drivers):
hnsw_m            = 16
hnsw_ef_construct = 128
hnsw_ef           = 64
quantization      = "scalar"         # none | scalar | product — tradeoff: speed vs recall

# ─── Query Engine ─────────────────────────────────────────────────────────────
[query]
default_top_k          = 10
cache_size_entries     = 10000
ranker_weights         = { similarity = 0.6, graph = 0.25, metadata = 0.15 }
max_graph_depth        = 3

# ─── Storage ──────────────────────────────────────────────────────────────────
[storage]
data_dir              = "~/.mccp/data"   # env: MCCP_DATA_DIR
metadata_backend      = "sled"           # sled (embedded, default) | sqlite | rocksdb
graph_backend         = "memory+wal"     # in-process with WAL for crash safety

# ─── Docker ───────────────────────────────────────────────────────────────────
[docker]
compose_file          = "~/.mccp/docker-compose.yml"
auto_start            = true             # start managed containers with daemon
```

### API Key Security

API keys are never stored in plaintext in `config.toml`. Use `api_key_env` to reference an environment variable name:

```toml
{ driver = "openai", model = "text-embedding-3-small", api_key_env = "OPENAI_API_KEY" }
```

Or pass via environment directly:

```bash
MCCP_EMBEDDING_API_KEY=sk-... mccp start
```

Keys stored via `mccp provider set ... --api-key` are written to the OS keychain (macOS Keychain, Windows Credential Manager, Linux libsecret) — not to disk in plaintext.

---

## Performance Architecture

Performance is a primary design constraint, not an afterthought. Every layer has explicit targets and strategies.

### Targets

| Operation | Target | Measurement |
|---|---|---|
| Query latency (cached) | < 5ms | p99, warm cache |
| Query latency (uncached) | < 150ms | p99, 50k-file project |
| Incremental index (100 files) | < 15s | wall time, Ollama local |
| Embedding throughput | ≥ 200 chunks/s | Ollama on 8-core CPU |
| File watcher event to queue | < 50ms | debounce window |
| Daemon startup | < 500ms | to first query served |

### I/O Strategy

**Memory-mapped file reading.** Files above 512 KB are read via `mmap` rather than `read()`. This eliminates kernel-to-userspace copies for large source files and lets the OS page cache do its job.

**Buffered reads for small files.** Files under 512 KB use a 256 KB `BufReader` — enough to read most source files in a single syscall.

**Parallel file discovery.** At index start, `mccp` walks the directory tree using a work-stealing thread pool (`rayon`). File discovery runs in parallel with hashing, so the indexer is never sequentially blocked waiting for directory entries.

**Zero-copy chunk passing.** Chunks are passed between pipeline stages as `Arc<str>` slices pointing into the original file buffer — no allocation per chunk during the parse → chunk → embed pipeline.

### Indexer Pipeline — Parallel Stage Design

```
File Watcher
    │  (changed file paths, debounced 50ms)
    ▼
Hash Check Pool  ──── 4 threads, async I/O
    │  (only changed files pass through)
    ▼
Parse Pool  ──────────── num_cpus threads (rayon), tree-sitter is thread-safe
    │  (AST, symbol list, chunk list)  parallel per file
    ▼
Batch Assembler  ─────── collects N chunks (default: 32) before flushing
    │
    ├──► Embed Pool  ───── async HTTP to Embedding Provider, pipelined
    │        │  (vectors)
    │        ▼
    │    Qdrant Upsert ─── batched, async, non-blocking to query engine
    │
    └──► Summarize Pool ─── 2 threads (rate-limited by LLM throughput)
             │  (JSON summaries)
             ▼
         Summary Cache ──── sled write (async, WAL-backed)
```

Parse and embedding are the two hotspots. Parse runs fully parallel on all CPU cores via rayon — there is no global lock on the parser. Embedding batches are pipelined: while batch N is in flight to the provider, batch N+1 is being assembled. This keeps the provider saturated with minimal idle time.

**Summarization is intentionally throttled.** LLM summaries are expensive and not on the query critical path — they run in a 2-thread pool with a back-pressure channel so they don't starve the embedder.

### Embedding Batching

The batch size (default: 32) is the primary throughput knob. Larger batches mean fewer HTTP round trips to the provider but more memory per batch.

| System | Recommended batch size |
|---|---|
| Ollama on CPU (8-core) | 16–32 |
| Ollama on GPU (8GB VRAM) | 64–128 |
| OpenAI / cloud provider | 256–2048 (rate-limit aware) |
| HuggingFace TEI self-hosted | 64–256 |

`mccp` auto-detects Ollama GPU availability and adjusts `batch_size` at startup. The config value is a ceiling, not a fixed size — the assembler flushes early if the provider is idle.

### Qdrant HNSW Tuning

The default parameters are tuned for code retrieval (high recall, moderate index time):

| Parameter | Default | Effect |
|---|---|---|
| `hnsw_m` | 16 | Graph connectivity — higher = better recall, more RAM |
| `hnsw_ef_construct` | 128 | Build-time beam width — higher = slower index, better quality |
| `hnsw_ef` | 64 | Query-time beam width — higher = better recall, slower query |
| `quantization` | scalar | 4× memory reduction, ~1% recall loss |

For very large codebases (>200k chunks), reduce `hnsw_m` to 8 and enable product quantization to keep memory under control.

### Query Engine — Hot Path

```
query text
    │
    ▼
Cache lookup ─── O(1) hash map, ~1µs
    │ miss
    ▼
Embed query ─── single text, async HTTP (~5-50ms depending on provider)
    │
    ▼
Qdrant search ─── HNSW ANN, async HTTP (~2-10ms)
    │
    ▼
Load summaries ─── all from in-process sled read-cache (< 1ms if warm)
    │
    ▼
Score & rank ─── pure CPU, ~0.1ms for top-50 candidates
    │
    ▼
Return + write cache ─── async, does not block response
```

The query hot path has zero blocking I/O after the embed and vector search calls. Summary loading, scoring, and ranking are all in-process operations.

### In-Process Data Structures

| Store | Structure | Notes |
|---|---|---|
| Query cache | `Arc<DashMap<CacheKey, CachedResult>>` | Lock-free concurrent hashmap |
| Graph adjacency | `Arc<RwLock<HashMap<NodeId, SmallVec<[NodeId; 8]>>>>` | `SmallVec` avoids heap alloc for nodes with ≤8 edges (covers ~90% of methods) |
| Symbol index | `Arc<DashMap<SymbolName, SmallVec<[SymbolRef; 4]>>>` | Lock-free, optimized for single-definition common case |
| Summary cache | `Arc<SegmentedLruCache<FileHash, Summary>>` | Segmented to reduce contention under concurrent queries |
| File hash map | `sled` embedded B-tree (mmap'd) | Survives restarts, no separate DB process |

### Platform-Specific I/O

| Platform | File watcher | Async runtime | File reads |
|---|---|---|---|
| Linux | `inotify` via `notify` crate | `tokio` (io_uring optional via `tokio-uring`) | `mmap` + `pread` |
| macOS | `FSEvents` via `notify` crate | `tokio` (kqueue) | `mmap` + `pread` |
| Windows | `ReadDirectoryChangesW` via `notify` crate | `tokio` (IOCP) | `mmap` (`MapViewOfFile`) |

On Linux, `io_uring` can be enabled for file reads via the `MCCP_IO_URING=1` environment variable. This reduces syscall overhead further for high-throughput indexing on NVMe storage. Requires Linux 5.1+.

### Startup Performance

The daemon uses lazy project activation: at startup it only loads the metadata index (fast, sled mmap). Heavy data (graph adjacency, summary cache) is loaded on first query per project, not at startup. Daemon is ready to serve in under 500ms even with dozens of registered projects.

---

## Platform Independence

`mccp` runs on Linux, macOS, and Windows (native + WSL2). All platform-specific behavior is isolated to thin adapter modules behind a common interface.

### OS Abstractions

| Concern | Linux | macOS | Windows |
|---|---|---|---|
| Daemon process | `double-fork` + `setsid` | `double-fork` + `setsid` | Windows Service via `windows-service` crate |
| PID file | `~/.mccp/daemon.pid` | `~/.mccp/daemon.pid` | `%APPDATA%\mccp\daemon.pid` |
| Config home | `~/.mccp/` | `~/.mccp/` | `%APPDATA%\mccp\` |
| Data dir (default) | `~/.mccp/data/` | `~/.mccp/data/` | `%APPDATA%\mccp\data\` |
| Log dir | `~/.mccp/logs/` | `~/.mccp/logs/` | `%APPDATA%\mccp\logs\` |
| Keychain | `libsecret` (Secret Service) | macOS Keychain | Windows Credential Manager |
| File watcher | `inotify` | `FSEvents` | `ReadDirectoryChangesW` |
| Async I/O | `epoll` / `io_uring` | `kqueue` | IOCP |
| Docker socket | `/var/run/docker.sock` | `/var/run/docker.sock` | `npipe:////./pipe/docker_engine` |

All paths are resolved using Rust's `dirs` crate for cross-platform correctness. Hardcoded Unix paths are never used anywhere in the codebase.

### Windows Notes

On Windows, `mccp` runs natively without WSL2, though WSL2 is also supported. Docker Desktop for Windows is supported in addition to Docker Engine on WSL2. The Docker socket path is auto-detected.

The daemon registers itself as a Windows Service via the `windows-service` crate, enabling `mccp start` on Windows to behave identically to Linux. Service management integrates with `sc.exe` and the Services MMC snap-in as a fallback.

### Path Handling

All paths stored in the metadata store and config are canonicalized at write time and normalized to forward slashes in serialized form. On Windows, the path `C:\Users\alice\project` is stored as `C:/Users/alice/project` internally and round-tripped correctly on all platforms.

### CI / Container Environments

In headless environments (no TTY), `mccp` automatically disables spinner output and TUI prompts. Detection is via `atty::is(atty::Stream::Stdout)`. The `NO_COLOR=1` or `CI=true` environment variable forces plain text output.

### Metrics (available at `GET localhost:7422/metrics`)

| Metric | Description |
|--------|-------------|
| `query_latency_p50/p95/p99` | Query engine latency (total) |
| `query_cache_hit_rate` | Cache effectiveness |
| `indexer_queue_depth` | Backlog of files waiting to be indexed |
| `indexer_lag_seconds` | Time since last file change processed |
| `embedding_provider_latency_p99` | Embedding provider call latency |
| `embedding_batch_size_avg` | Average batch size sent to embedding provider |
| `llm_provider_latency_p99` | LLM summarization call latency |
| `vector_store_upsert_latency_p99` | Vector store write latency |
| `vector_store_search_latency_p99` | Vector store query latency |
| `provider_error_rate` | Errors per minute per provider slot |
| `graph_traversal_depth_avg` | Average depth of graph queries |
| `feedback_good_rate` | % of agent-signalled results rated "good" |
| `active_embedding_provider` | Label: which provider is currently active |
| `active_llm_provider` | Label: which provider is currently active |
| `active_vector_provider` | Label: which provider is currently active |

### Feedback Loop

When agents call `feedback` with a `"bad"` or `"irrelevant"` signal:
- The query + result + signal are written to the feedback store
- After 50 feedback signals, `mccp` automatically re-tunes ranker weights using the logged data
- Weight adjustments are logged with before/after values at `info` level

---

## Recommended Models (Ollama)

```bash
mccp model pull --recommended
```

This pulls the full recommended stack:

| Use case | Model | Size |
|----------|-------|------|
| Embedding | `nomic-embed-text` | 274 MB |
| Code summarization (fast) | `codellama:7b` | 3.8 GB |
| Code summarization (quality) | `codellama:13b` | 7.3 GB |
| General reasoning | `mistral:7b` | 4.1 GB |
| Re-ranking (optional) | `mxbai-rerank-base-v1` | 278 MB |

The `--recommended` flag pulls only `nomic-embed-text` + `codellama:7b` on machines with less than 16 GB RAM, and the full quality stack on machines with 16 GB or more.

---

## Quick Start

```bash
# 1. Install mccp
cargo install mccp

# 2. Install Docker and start all services
mccp docker install
mccp docker start

# 3. Pull recommended models
mccp model pull --recommended

# 4. Register your project and index it
mccp project add /path/to/my-project --name my-api
mccp index --project my-api

# 5. Start the daemon (MCP server + indexer run in background)
mccp start

# 6. Open the TUI console
mccp console

# 7. Connect your AI agent to the MCP server
# stdio:  mccp --project my-api
# HTTP:   http://localhost:7422/mcp
```

---

## CLI Design Conventions

`mccp` follows the GitHub CLI / Claude CLI style:

- **Spinner on all async operations** with descriptive status text (not a bare cursor)
- **Color output** — green for success, yellow for warnings, red for errors, dim for metadata
- **Confirm prompts** for destructive actions (`reset`, `docker reset`, `model remove`)
- **Plain output mode** via `--plain` or `NO_COLOR=1` for scripting
- **JSON output** via `--json` on query and status commands for piping
- **Short flags** for common options: `-p` for `--project`, `-v` for `--verbose`
- **Helpful errors** — when something fails, the error message says what to do next
- **No silent failures** — every command exits with a non-zero code on failure and prints the reason

---

## Non-Goals

- Does not replace Git or version control
- Does not execute code or run tests
- Does not modify source files
- Does not generate new code (intelligence layer only)
- Does not sync or replicate data between machines
- Does not require cloud connectivity of any kind

---

## Constraints & Invariants

- Summaries include model metadata — stale summaries are never served without a staleness flag
- Chunker never splits a method body across chunk boundaries
- Graph traversal always uses a visited set — infinite loops are impossible by construction
- All vector store queries include a `project_id` filter — cross-project data leakage is impossible regardless of backend
- Secrets scrubber runs before any content reaches any provider (embedding, LLM, or vector store)
- All destructive CLI commands require explicit confirmation
- Re-index is always resumable — a crash mid-index leaves the project in a partial-but-valid state; the next `mccp index` continues from the last completed file
- Provider fingerprint is stored with every indexed chunk — a provider change is always detected at startup
- Mid-query provider switching never happens — provider failures during a query return an error, they do not silently fall back
- API keys are never written to disk in plaintext — always stored via OS keychain or referenced by env var name
- All paths stored internally use forward-slash form and are canonicalized — no platform-specific path separators in stored data
- `NO_COLOR=1` and `CI=true` are always respected — no color or interactive output in non-TTY environments
- The daemon exposes identical behavior across Linux, macOS, and Windows — no platform-specific CLI flags or workflows

---

## Testing Requirements

Every component must have tests at multiple levels. Tests are co-located with the module they test (`src/*/tests.rs`) and gated by feature where integration infrastructure (Qdrant, Ollama) is required.

Test categories:

| Category | Requires Docker | Requires Ollama | Run in CI |
|---|---|---|---|
| Unit | No | No | Always |
| Integration | Yes (Qdrant) | Optional (mock) | Always |
| End-to-end | Yes | Yes | On release branch |
| Performance | Yes | Yes | On release branch |

Run unit + integration tests:
```bash
cargo test
```

Run full suite including end-to-end:
```bash
cargo test --features e2e
```

Run only a specific component:
```bash
cargo test --package mccp-indexer
cargo test --package mccp-query
cargo test --package mccp-graph
```

---

### 1. Indexer Tests

Located: `crates/mccp-indexer/src/tests.rs`

#### Unit Tests

```rust
// test: hash delta — unchanged file is skipped
#[test]
fn test_unchanged_file_skipped() {
    let mut store = MetadataStore::in_memory();
    let file = fake_file("src/main.rs", "fn main() {}");
    store.record_hash(&file);
    let result = should_reindex(&file, &store);
    assert_eq!(result, false);
}

// test: hash delta — modified file is queued
#[test]
fn test_modified_file_queued() {
    let mut store = MetadataStore::in_memory();
    let original = fake_file("src/main.rs", "fn main() {}");
    store.record_hash(&original);
    let modified = fake_file("src/main.rs", "fn main() { println!(\"hi\"); }");
    let result = should_reindex(&modified, &store);
    assert_eq!(result, true);
}

// test: new file is always queued
#[test]
fn test_new_file_always_queued() {
    let store = MetadataStore::in_memory();
    let file = fake_file("src/new.rs", "pub fn new() {}");
    assert_eq!(should_reindex(&file, &store), true);
}

// test: deleted file is removed from metadata
#[test]
fn test_deleted_file_removed_from_metadata() {
    let mut store = MetadataStore::in_memory();
    let file = fake_file("src/gone.rs", "fn x() {}");
    store.record_hash(&file);
    store.remove("src/gone.rs");
    assert!(store.get("src/gone.rs").is_none());
}

// test: resumable index — second run from partial state skips already-indexed files
#[test]
fn test_resume_skips_completed_files() {
    let mut store = MetadataStore::in_memory();
    let files = vec![
        fake_file("a.rs", "fn a() {}"),
        fake_file("b.rs", "fn b() {}"),
        fake_file("c.rs", "fn c() {}"),
    ];
    // simulate crash after indexing a.rs and b.rs
    store.record_hash(&files[0]);
    store.record_hash(&files[1]);
    let pending = files.iter().filter(|f| should_reindex(f, &store)).collect::<Vec<_>>();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].path, "c.rs");
}

// test: secrets scrubber redacts API keys before content reaches embedder
#[test]
fn test_secrets_scrubbed_before_embed() {
    let raw = r#"
        const API_KEY: &str = "sk-prod-abc123xyz";
        fn connect() { client.auth(API_KEY); }
    "#;
    let scrubbed = scrub_secrets(raw);
    assert!(!scrubbed.contains("sk-prod-abc123xyz"));
    assert!(scrubbed.contains("[REDACTED]"));
}

// test: scrubber does not redact non-secret strings
#[test]
fn test_scrubber_preserves_non_secrets() {
    let raw = r#"let name = "hello world"; fn greet() { println!("{}", name); }"#;
    let scrubbed = scrub_secrets(raw);
    assert_eq!(scrubbed, raw);
}

// test: scrubber handles multiple secrets in one file
#[test]
fn test_scrubber_handles_multiple_secrets() {
    let raw = "key1=AKIA1234567890ABCDEF\nkey2=ghp_abcdefghijklmnopqrstuvwxyz123456";
    let scrubbed = scrub_secrets(raw);
    assert!(!scrubbed.contains("AKIA1234567890ABCDEF"));
    assert!(!scrubbed.contains("ghp_abcdefghijklmnopqrstuvwxyz123456"));
    assert_eq!(scrubbed.matches("[REDACTED]").count(), 2);
}

// test: file watcher detects new file within debounce window
#[tokio::test]
async fn test_watcher_detects_new_file() {
    let dir = tempdir().unwrap();
    let (tx, mut rx) = mpsc::channel(10);
    let _watcher = FileWatcher::start(dir.path(), tx).unwrap();
    tokio::fs::write(dir.path().join("new.rs"), "fn x() {}").await.unwrap();
    let event = timeout(Duration::from_millis(500), rx.recv()).await.unwrap().unwrap();
    assert_eq!(event.kind, WatchEventKind::Created);
}

// test: watcher debounces rapid successive changes into one event
#[tokio::test]
async fn test_watcher_debounces_rapid_changes() {
    let dir = tempdir().unwrap();
    let (tx, mut rx) = mpsc::channel(10);
    let _watcher = FileWatcher::start(dir.path(), tx).unwrap();
    let path = dir.path().join("file.rs");
    for i in 0..10 {
        tokio::fs::write(&path, format!("fn x() {{ {} }}", i)).await.unwrap();
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    let mut count = 0;
    while rx.try_recv().is_ok() { count += 1; }
    assert!(count <= 2, "expected debounced events, got {}", count);
}
```

#### Integration Tests

```rust
// test: full index cycle on a small Rust project — all files indexed, metadata persisted
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_full_index_cycle_rust_project() {
    let project = fixture_project("small_rust");
    let ctx = TestContext::start().await;
    ctx.indexer.run(&project).await.unwrap();
    let meta = ctx.metadata_store.list(&project.id).await.unwrap();
    assert_eq!(meta.len(), project.source_files().len());
    for entry in &meta {
        assert!(entry.chunk_count > 0);
        assert!(entry.embedding_model.is_some());
    }
}

// test: incremental index only re-indexes changed files
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_incremental_index_only_changed() {
    let project = fixture_project("small_rust");
    let ctx = TestContext::start().await;
    ctx.indexer.run(&project).await.unwrap();
    let original_hashes = ctx.metadata_store.all_hashes(&project.id).await.unwrap();
    // modify one file
    project.modify_file("src/lib.rs", "pub fn changed() {}");
    ctx.indexer.run(&project).await.unwrap();
    let new_hashes = ctx.metadata_store.all_hashes(&project.id).await.unwrap();
    let changed: Vec<_> = new_hashes.iter()
        .filter(|(path, hash)| original_hashes.get(*path) != Some(hash))
        .collect();
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].0, "src/lib.rs");
}

// test: index is resumable after simulated mid-run crash
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_index_resumable_after_crash() {
    let project = fixture_project("medium_rust");
    let ctx = TestContext::start().await;
    // index half the files then abort
    ctx.indexer.run_with_abort_after(&project, 5).await;
    let partial = ctx.metadata_store.list(&project.id).await.unwrap();
    // resume
    ctx.indexer.run(&project).await.unwrap();
    let complete = ctx.metadata_store.list(&project.id).await.unwrap();
    assert_eq!(complete.len(), project.source_files().len());
    // already-indexed files should have same hashes (not re-indexed)
    for entry in &partial {
        let after = complete.iter().find(|e| e.path == entry.path).unwrap();
        assert_eq!(after.file_hash, entry.file_hash);
    }
}
```

---

### 2. Chunker Tests

Located: `crates/mccp-indexer/src/chunker/tests.rs`

```rust
// test: single small method produces one chunk
#[test]
fn test_small_method_single_chunk() {
    let source = r#"fn greet(name: &str) -> String { format!("Hello, {}", name) }"#;
    let chunks = chunk_source(source, Language::Rust, ChunkConfig::default());
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].scope, ChunkScope::Method("greet".into()));
}

// test: two distinct methods produce two chunks
#[test]
fn test_two_methods_two_chunks() {
    let source = r#"
        fn alpha() { println!("a"); }
        fn beta()  { println!("b"); }
    "#;
    let chunks = chunk_source(source, Language::Rust, ChunkConfig::default());
    assert_eq!(chunks.len(), 2);
    assert!(chunks.iter().any(|c| c.scope == ChunkScope::Method("alpha".into())));
    assert!(chunks.iter().any(|c| c.scope == ChunkScope::Method("beta".into())));
}

// test: no chunk exceeds 512 tokens
#[test]
fn test_no_chunk_exceeds_max_tokens() {
    let source = generate_large_source(5000);
    let chunks = chunk_source(&source, Language::Rust, ChunkConfig::default());
    for chunk in &chunks {
        assert!(chunk.token_count <= 512, "chunk exceeded 512 tokens: {}", chunk.token_count);
    }
}

// test: method body is never split across chunk boundaries
#[test]
fn test_method_never_split_across_chunks() {
    let source = generate_large_method("giant_fn", 600);
    let chunks = chunk_source(&source, Language::Rust, ChunkConfig::default());
    let method_chunks: Vec<_> = chunks.iter()
        .filter(|c| c.scope == ChunkScope::Method("giant_fn".into()))
        .collect();
    // all sub-chunks must be logically adjacent — no gap in byte offsets
    for window in method_chunks.windows(2) {
        assert_eq!(window[0].end_byte, window[1].start_byte,
            "method body split with a gap between chunks");
    }
}

// test: overlap between adjacent chunks is within configured range
#[test]
fn test_chunk_overlap_within_bounds() {
    let source = generate_large_source(2000);
    let config = ChunkConfig { max_tokens: 512, overlap_tokens: 64 };
    let chunks = chunk_source(&source, Language::Rust, config);
    for window in chunks.windows(2) {
        let overlap = compute_token_overlap(&window[0], &window[1]);
        assert!(overlap <= 64, "overlap {} exceeds max 64", overlap);
        assert!(overlap > 0, "adjacent chunks have no overlap");
    }
}

// test: Python, TypeScript, Go produce valid chunks (multi-language support)
#[test]
fn test_multi_language_chunking() {
    for (source, lang) in [
        ("def foo():\n    return 1\ndef bar():\n    return 2", Language::Python),
        ("function foo() { return 1; }\nfunction bar() { return 2; }", Language::TypeScript),
        ("func Foo() int { return 1 }\nfunc Bar() int { return 2 }", Language::Go),
    ] {
        let chunks = chunk_source(source, lang, ChunkConfig::default());
        assert_eq!(chunks.len(), 2, "expected 2 chunks for {:?}", lang);
    }
}

// test: empty file produces zero chunks without panic
#[test]
fn test_empty_file_no_chunks_no_panic() {
    let chunks = chunk_source("", Language::Rust, ChunkConfig::default());
    assert_eq!(chunks.len(), 0);
}

// test: file with only comments produces zero method chunks
#[test]
fn test_comment_only_file() {
    let source = "// this is a comment\n// another comment";
    let chunks = chunk_source(source, Language::Rust, ChunkConfig::default());
    assert!(chunks.iter().all(|c| c.scope == ChunkScope::File));
}
```

---

### 3. Graph Engine Tests

Located: `crates/mccp-graph/src/tests.rs`

```rust
// test: direct call edge is recorded
#[test]
fn test_direct_call_edge_recorded() {
    let mut graph = GraphStore::new();
    graph.add_edge("UserController::login", "AuthService::validate", EdgeKind::Call);
    assert!(graph.has_edge("UserController::login", "AuthService::validate"));
}

// test: BFS traversal returns all reachable nodes within depth
#[test]
fn test_bfs_respects_max_depth() {
    let mut graph = GraphStore::new();
    // A → B → C → D (chain of depth 3)
    graph.add_edge("A", "B", EdgeKind::Call);
    graph.add_edge("B", "C", EdgeKind::Call);
    graph.add_edge("C", "D", EdgeKind::Call);
    let reachable = graph.traverse("A", TraversalConfig { max_depth: 2, ..Default::default() });
    assert!(reachable.contains("B"));
    assert!(reachable.contains("C"));
    assert!(!reachable.contains("D"), "D is at depth 3, should not be returned");
}

// test: cycle in graph does not cause infinite loop
#[test]
fn test_cycle_does_not_infinite_loop() {
    let mut graph = GraphStore::new();
    graph.add_edge("A", "B", EdgeKind::Call);
    graph.add_edge("B", "C", EdgeKind::Call);
    graph.add_edge("C", "A", EdgeKind::Call); // cycle back to A
    let result = std::panic::catch_unwind(|| {
        graph.traverse("A", TraversalConfig::default())
    });
    assert!(result.is_ok(), "graph traversal panicked on cycle");
}

// test: mutual import cycle does not infinite loop
#[test]
fn test_mutual_import_cycle_safe() {
    let mut graph = GraphStore::new();
    graph.add_edge("module_a", "module_b", EdgeKind::Import);
    graph.add_edge("module_b", "module_a", EdgeKind::Import);
    let visited = graph.traverse("module_a", TraversalConfig::default());
    assert_eq!(visited.len(), 2); // both nodes, visited once each
}

// test: node with no edges returns empty neighbour set
#[test]
fn test_isolated_node_returns_empty() {
    let mut graph = GraphStore::new();
    graph.register_node("orphan");
    let neighbours = graph.neighbours("orphan");
    assert!(neighbours.is_empty());
}

// test: project namespace isolation — edges from project A not visible from project B
#[test]
fn test_project_namespace_isolation() {
    let mut store = MultiProjectGraphStore::new();
    store.add_edge("proj_a", "A::foo", "A::bar", EdgeKind::Call);
    store.add_edge("proj_b", "B::foo", "B::bar", EdgeKind::Call);
    assert!(store.has_edge("proj_a", "A::foo", "A::bar"));
    assert!(!store.has_edge("proj_b", "A::foo", "A::bar"),
        "proj_a edges must not be visible from proj_b");
}

// test: entry points detected — methods with in-degree 0
#[test]
fn test_entry_points_detected() {
    let mut graph = GraphStore::new();
    graph.add_edge("HttpHandler::post", "UserService::create", EdgeKind::Call);
    graph.add_edge("UserService::create", "UserRepo::insert", EdgeKind::Call);
    let entries = graph.entry_points();
    assert!(entries.contains("HttpHandler::post"));
    assert!(!entries.contains("UserService::create"));
    assert!(!entries.contains("UserRepo::insert"));
}

// test: bottleneck detection — high betweenness centrality node identified
#[test]
fn test_bottleneck_detection() {
    let mut graph = GraphStore::new();
    // all handlers route through AuthService
    for handler in ["login_handler", "register_handler", "refresh_handler"] {
        graph.add_edge(handler, "AuthService::validate", EdgeKind::Call);
        graph.add_edge("AuthService::validate", "TokenStore::get", EdgeKind::Call);
    }
    let bottlenecks = graph.bottlenecks(1);
    assert!(bottlenecks.contains("AuthService::validate"));
}

// test: leaf nodes detected — methods with out-degree 0
#[test]
fn test_leaf_nodes_detected() {
    let mut graph = GraphStore::new();
    graph.add_edge("A", "B", EdgeKind::Call);
    graph.add_edge("A", "C", EdgeKind::Call);
    // B and C have no outgoing edges → leaves
    let leaves = graph.leaf_nodes();
    assert!(leaves.contains("B"));
    assert!(leaves.contains("C"));
    assert!(!leaves.contains("A"));
}
```

---

### 4. Query Engine Tests

Located: `crates/mccp-query/src/tests.rs`

#### Unit Tests

```rust
// test: query cache returns hit for identical query + project
#[test]
fn test_query_cache_hit_same_query() {
    let mut cache = QueryCache::new(1000);
    let key = QueryCacheKey { project_id: "proj_a".into(), query_hash: hash("how is auth done"), model_version: "v1".into() };
    let result = fake_query_result(5);
    cache.insert(key.clone(), result.clone());
    assert_eq!(cache.get(&key), Some(&result));
}

// test: query cache miss for different project, same query text
#[test]
fn test_query_cache_miss_different_project() {
    let mut cache = QueryCache::new(1000);
    let key_a = QueryCacheKey { project_id: "proj_a".into(), query_hash: hash("auth"), model_version: "v1".into() };
    let key_b = QueryCacheKey { project_id: "proj_b".into(), query_hash: hash("auth"), model_version: "v1".into() };
    cache.insert(key_a, fake_query_result(3));
    assert!(cache.get(&key_b).is_none(), "different project must not share cache entry");
}

// test: query cache evicts LRU entry at capacity
#[test]
fn test_query_cache_lru_eviction() {
    let mut cache = QueryCache::new(2);
    let k1 = make_key("q1");
    let k2 = make_key("q2");
    let k3 = make_key("q3");
    cache.insert(k1.clone(), fake_query_result(1));
    cache.insert(k2.clone(), fake_query_result(1));
    cache.insert(k3.clone(), fake_query_result(1)); // k1 should be evicted
    assert!(cache.get(&k1).is_none(), "k1 should have been evicted");
    assert!(cache.get(&k2).is_some());
    assert!(cache.get(&k3).is_some());
}

// test: cache is invalidated for project on re-index
#[test]
fn test_cache_invalidated_on_reindex() {
    let mut cache = QueryCache::new(1000);
    cache.insert(make_key_for("proj_a", "auth"), fake_query_result(3));
    cache.invalidate_project("proj_a");
    assert!(cache.get(&make_key_for("proj_a", "auth")).is_none());
}

// test: ranker applies configurable weights correctly
#[test]
fn test_ranker_applies_weights() {
    let config = RankerConfig { similarity: 0.6, graph: 0.25, metadata: 0.15 };
    let candidate = RankCandidate { similarity: 1.0, graph_score: 0.0, metadata_score: 0.0 };
    let score = rank(&candidate, &config);
    assert!((score - 0.6).abs() < 1e-6);
}

// test: ranker weights sum to approximately 1.0 — misconfigured weights rejected
#[test]
fn test_ranker_rejects_weights_not_summing_to_one() {
    let result = RankerConfig::new(0.5, 0.5, 0.5); // sum = 1.5
    assert!(result.is_err(), "weights that don't sum to ~1.0 should be rejected");
}

// test: result with higher graph score wins over pure similarity when weights favor graph
#[test]
fn test_graph_weight_can_outrank_similarity() {
    let config = RankerConfig { similarity: 0.3, graph: 0.6, metadata: 0.1 };
    let high_sim = RankCandidate { similarity: 0.9, graph_score: 0.1, metadata_score: 0.5 };
    let high_graph = RankCandidate { similarity: 0.4, graph_score: 0.95, metadata_score: 0.5 };
    let score_sim   = rank(&high_sim, &config);
    let score_graph = rank(&high_graph, &config);
    assert!(score_graph > score_sim, "graph-heavy candidate should win when graph weight is 0.6");
}
```

#### Integration Tests

```rust
// test: query returns only results from the queried project
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_query_scoped_to_project() {
    let ctx = TestContext::start().await;
    ctx.index_fixture("proj_a", "small_rust").await;
    ctx.index_fixture("proj_b", "small_python").await;
    let results = ctx.query_engine
        .query(QueryRequest { project: "proj_a".into(), text: "main entry point".into(), top_k: 10 })
        .await.unwrap();
    for r in &results {
        assert_eq!(r.project_id, "proj_a", "result from wrong project: {:?}", r.path);
    }
}

// test: semantic query returns relevant results in top-3
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_semantic_query_relevant_results() {
    let ctx = TestContext::start().await;
    ctx.index_fixture("proj_a", "auth_service").await;
    let results = ctx.query_engine
        .query(QueryRequest { project: "proj_a".into(), text: "validate JWT token".into(), top_k: 5 })
        .await.unwrap();
    let top3_paths: Vec<_> = results.iter().take(3).map(|r| r.path.as_str()).collect();
    assert!(
        top3_paths.iter().any(|p| p.contains("auth") || p.contains("token") || p.contains("jwt")),
        "expected auth-related file in top 3, got: {:?}", top3_paths
    );
}

// test: stale results are flagged when model has changed since index
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_stale_results_flagged_on_model_change() {
    let ctx = TestContext::start().await;
    ctx.index_fixture("proj_a", "small_rust").await;
    ctx.model_manager.switch_chat_model("codellama:7b").await.unwrap();
    let results = ctx.query_engine
        .query(QueryRequest { project: "proj_a".into(), text: "any query".into(), top_k: 5 })
        .await.unwrap();
    assert!(results.iter().any(|r| r.stale), "at least one result should be flagged stale after model change");
}
```

---

### 5. LLM Summary Tests

Located: `crates/mccp-indexer/src/summarizer/tests.rs`

```rust
// test: summary output conforms to strict JSON schema
#[tokio::test]
async fn test_summary_schema_valid() {
    let source = r#"
        pub struct UserService { db: Arc<Db> }
        impl UserService {
            pub async fn create(&self, dto: CreateUserDto) -> Result<User> {
                self.db.insert(dto).await
            }
        }
    "#;
    let mock_llm = MockLlm::returning(VALID_SUMMARY_JSON);
    let summary = summarize(source, Language::Rust, &mock_llm).await.unwrap();
    assert!(!summary.purpose.is_empty());
    assert!(!summary.responsibilities.is_empty());
    assert!(summary.methods.iter().all(|m| !m.name.is_empty()));
}

// test: summary includes _meta with model_id and schema_version
#[tokio::test]
async fn test_summary_meta_populated() {
    let mock_llm = MockLlm::returning(VALID_SUMMARY_JSON).with_model("codellama:13b");
    let summary = summarize("fn foo() {}", Language::Rust, &mock_llm).await.unwrap();
    assert_eq!(summary.meta.schema_version, CURRENT_SCHEMA_VERSION);
    assert_eq!(summary.meta.model_id, "codellama:13b");
    assert!(!summary.meta.file_hash.is_empty());
}

// test: summary is invalidated when model changes
#[test]
fn test_summary_invalidated_on_model_change() {
    let mut store = SummaryCacheStore::in_memory();
    let summary = fake_summary("codellama:7b");
    store.put("file_hash_abc", summary);
    let is_valid = store.is_valid("file_hash_abc", "codellama:13b");
    assert!(!is_valid, "summary should be invalid when model changed");
}

// test: summary is invalidated when file hash changes
#[test]
fn test_summary_invalidated_on_file_change() {
    let mut store = SummaryCacheStore::in_memory();
    store.put("hash_v1", fake_summary("codellama:13b"));
    let is_valid = store.is_valid("hash_v2", "codellama:13b");
    assert!(!is_valid, "summary should be invalid when file content changed");
}

// test: LLM returns invalid JSON — fallback to partial summary, no panic
#[tokio::test]
async fn test_malformed_llm_response_handled() {
    let mock_llm = MockLlm::returning("not valid json at all {{{");
    let result = summarize("fn foo() {}", Language::Rust, &mock_llm).await;
    assert!(result.is_ok(), "malformed LLM response should not panic or hard-fail");
    let summary = result.unwrap();
    assert!(summary.is_partial(), "malformed response should produce a partial summary");
}

// test: LLM timeout — fallback used, query still served
#[tokio::test]
async fn test_llm_timeout_fallback() {
    let mock_llm = MockLlm::that_times_out_after(Duration::from_millis(50));
    let result = summarize("fn foo() {}", Language::Rust, &mock_llm).await;
    assert!(result.is_ok(), "LLM timeout should not crash the indexer");
}
```

---

### 6. Model Manager Tests

Located: `crates/mccp-models/src/tests.rs`

```rust
// test: switching embedding model flags all projects for re-index
#[tokio::test]
async fn test_model_switch_flags_reindex() {
    let mut manager = ModelManager::with_mock_ollama();
    manager.register_project("proj_a", "nomic-embed-text", "codellama:13b");
    manager.register_project("proj_b", "nomic-embed-text", "codellama:13b");
    manager.switch_embedding_model("all-minilm").await.unwrap();
    assert!(manager.needs_reindex("proj_a"));
    assert!(manager.needs_reindex("proj_b"));
}

// test: switching chat model only flags projects indexed with old model
#[tokio::test]
async fn test_chat_model_switch_partial_reindex() {
    let mut manager = ModelManager::with_mock_ollama();
    manager.register_project("proj_a", "nomic-embed-text", "codellama:7b");
    manager.register_project("proj_b", "nomic-embed-text", "codellama:13b");
    manager.switch_chat_model("mistral:7b").await.unwrap();
    // both used a different chat model → both need reindex
    assert!(manager.needs_reindex("proj_a"));
    assert!(manager.needs_reindex("proj_b"));
}

// test: pull non-existent model returns clear error
#[tokio::test]
async fn test_pull_nonexistent_model_returns_error() {
    let manager = ModelManager::with_mock_ollama();
    let result = manager.pull("totally-fake-model:99b").await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("not found") || err.to_string().contains("does not exist"));
}

// test: model registry persists across restarts
#[test]
fn test_model_registry_persists() {
    let dir = tempdir().unwrap();
    {
        let mut registry = ModelRegistry::open(dir.path()).unwrap();
        registry.set_active_embedding("nomic-embed-text");
        registry.set_active_chat("codellama:13b");
        registry.save().unwrap();
    }
    {
        let registry = ModelRegistry::open(dir.path()).unwrap();
        assert_eq!(registry.active_embedding(), "nomic-embed-text");
        assert_eq!(registry.active_chat(), "codellama:13b");
    }
}

// test: recommended model selection based on available RAM
#[test]
fn test_recommended_models_by_ram() {
    let low_ram  = recommended_models(8 * 1024);   // 8 GB
    let high_ram = recommended_models(32 * 1024);  // 32 GB
    assert!(low_ram.chat.contains("7b"),  "low-RAM systems should get 7b model");
    assert!(high_ram.chat.contains("13b"), "high-RAM systems should get 13b model");
}

// test: Ollama health check fails gracefully when Ollama is down
#[tokio::test]
async fn test_ollama_health_check_graceful_failure() {
    let manager = ModelManager::with_ollama_at("http://localhost:19999"); // nothing there
    let health = manager.health_check().await;
    assert!(!health.is_healthy);
    assert!(!health.error.is_empty());
}
```

---

### 7. Docker Manager Tests

Located: `crates/mccp-docker/src/tests.rs`

```rust
// test: compose file is written with correct service definitions
#[test]
fn test_compose_file_written_correctly() {
    let dir = tempdir().unwrap();
    let config = DockerConfig { data_dir: dir.path().to_path_buf(), ..Default::default() };
    write_compose_file(&config).unwrap();
    let content = std::fs::read_to_string(dir.path().join("docker-compose.yml")).unwrap();
    assert!(content.contains("qdrant/qdrant"));
    assert!(content.contains("ollama/ollama"));
    assert!(content.contains(dir.path().to_str().unwrap()));
}

// test: set-data-dir updates compose file and preserves other settings
#[test]
fn test_set_data_dir_updates_compose() {
    let dir = tempdir().unwrap();
    let original = DockerConfig { data_dir: PathBuf::from("/old/path"), ..Default::default() };
    write_compose_file(&original).unwrap();
    let updated = DockerConfig { data_dir: dir.path().to_path_buf(), ..Default::default() };
    write_compose_file(&updated).unwrap();
    let content = std::fs::read_to_string(dir.path().join("docker-compose.yml")).unwrap();
    assert!(!content.contains("/old/path"));
    assert!(content.contains(dir.path().to_str().unwrap()));
}

// test: install detection skips install when Docker is already present
#[tokio::test]
async fn test_install_skipped_when_docker_present() {
    let mock_os = MockOs::with_docker_installed();
    let mut log = vec![];
    docker_install(&mock_os, &mut log).await.unwrap();
    assert!(!log.iter().any(|l: &String| l.contains("installing Docker")),
        "should not attempt install when Docker is already present");
}

// test: Qdrant health check returns healthy when container is running
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_qdrant_health_check_running() {
    let ctx = DockerTestContext::start().await;
    let health = ctx.check_service_health("qdrant").await;
    assert!(health.is_healthy);
}

// test: data directory is mounted correctly in container
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_data_dir_mounted_in_container() {
    let dir = tempdir().unwrap();
    let ctx = DockerTestContext::with_data_dir(dir.path()).await;
    ctx.write_to_service("qdrant", "testfile", b"hello").await.unwrap();
    assert!(dir.path().join("qdrant/testfile").exists(),
        "file written inside container should appear on host at the mounted path");
}
```

---

### 8. Project Isolation Tests

Located: `crates/mccp-core/src/isolation/tests.rs`

```rust
// test: vectors from project A are never returned for project B queries
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_vectors_isolated_between_projects() {
    let ctx = TestContext::start().await;
    ctx.index_fixture("proj_a", "auth_service").await;
    ctx.index_fixture("proj_b", "unrelated_service").await;

    // query proj_b with a phrase that only exists in proj_a
    let results = ctx.query_engine.query(QueryRequest {
        project: "proj_b".into(),
        text: "JWT token validation".into(),
        top_k: 10,
    }).await.unwrap();

    for r in &results {
        assert_eq!(r.project_id, "proj_b",
            "result from proj_a leaked into proj_b query: {:?}", r.path);
    }
}

// test: deleting project A does not affect project B's data
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_project_delete_does_not_affect_other_projects() {
    let ctx = TestContext::start().await;
    ctx.index_fixture("proj_a", "small_rust").await;
    ctx.index_fixture("proj_b", "small_rust").await;
    let count_before = ctx.metadata_store.count("proj_b").await.unwrap();
    ctx.project_manager.remove("proj_a", RemoveOptions::purge()).await.unwrap();
    let count_after = ctx.metadata_store.count("proj_b").await.unwrap();
    assert_eq!(count_before, count_after, "proj_b data changed after proj_a was deleted");
}

// test: project_id is derived deterministically from canonical path
#[test]
fn test_project_id_deterministic() {
    let id_a = ProjectId::from_path("/home/user/my-project");
    let id_b = ProjectId::from_path("/home/user/my-project");
    assert_eq!(id_a, id_b);
}

// test: two different paths produce different project IDs
#[test]
fn test_different_paths_different_ids() {
    let id_a = ProjectId::from_path("/home/user/project-a");
    let id_b = ProjectId::from_path("/home/user/project-b");
    assert_ne!(id_a, id_b);
}

// test: graph edges do not cross project namespace boundary
#[test]
fn test_graph_no_cross_project_edges() {
    let mut store = MultiProjectGraphStore::new();
    store.add_edge("proj_a", "A::foo", "A::bar", EdgeKind::Call);
    let edges_b = store.all_edges("proj_b");
    assert!(edges_b.is_empty(), "proj_b should have no edges after only proj_a was populated");
}
```

---

### 9. Daemon & CLI Tests

Located: `crates/mccp-cli/src/tests.rs`

```rust
// test: `mccp start` writes PID file
#[tokio::test]
async fn test_start_writes_pid_file() {
    let dir = tempdir().unwrap();
    let runtime = TestRuntime::with_home(dir.path());
    runtime.run_cmd(&["start"]).await.unwrap();
    assert!(dir.path().join(".mccp/daemon.pid").exists());
}

// test: `mccp stop` removes PID file and daemon is no longer running
#[tokio::test]
async fn test_stop_removes_pid_and_daemon_exits() {
    let dir = tempdir().unwrap();
    let runtime = TestRuntime::with_home(dir.path());
    runtime.run_cmd(&["start"]).await.unwrap();
    runtime.run_cmd(&["stop"]).await.unwrap();
    assert!(!dir.path().join(".mccp/daemon.pid").exists());
}

// test: `mccp status` exits 0 when daemon is running
#[tokio::test]
async fn test_status_exit_code_running() {
    let runtime = TestRuntime::with_running_daemon().await;
    let result = runtime.run_cmd(&["status"]).await;
    assert_eq!(result.exit_code, 0);
}

// test: `mccp status` exits non-zero when daemon is not running
#[tokio::test]
async fn test_status_exit_code_not_running() {
    let runtime = TestRuntime::with_no_daemon();
    let result = runtime.run_cmd(&["status"]).await;
    assert_ne!(result.exit_code, 0);
}

// test: `mccp index --reset` requires confirmation, aborts on 'n'
#[tokio::test]
async fn test_reset_aborts_on_no_confirmation() {
    let runtime = TestRuntime::with_project("proj_a");
    let result = runtime.run_cmd_with_input(&["index", "--reset", "--project", "proj_a"], "n\n").await;
    assert!(result.stderr.contains("aborted") || result.stdout.contains("aborted"));
    assert!(runtime.project_data_exists("proj_a"), "data should not be wiped when user declined");
}

// test: destructive commands fail without --project when no default project is set
#[tokio::test]
async fn test_destructive_requires_project_flag() {
    let runtime = TestRuntime::with_no_default_project();
    let result = runtime.run_cmd(&["index", "--reset"]).await;
    assert_ne!(result.exit_code, 0);
    assert!(result.stderr.contains("--project") || result.stderr.contains("no default project"));
}

// test: `--json` flag on query outputs valid JSON to stdout
#[tokio::test]
async fn test_query_json_output_valid() {
    let runtime = TestRuntime::with_indexed_project("proj_a", "small_rust").await;
    let result = runtime.run_cmd(&["query", "main function", "--project", "proj_a", "--json"]).await;
    assert_eq!(result.exit_code, 0);
    let parsed: serde_json::Value = serde_json::from_str(&result.stdout)
        .expect("--json flag should produce valid JSON stdout");
    assert!(parsed.is_array() || parsed.get("results").is_some());
}

// test: daemon survives terminal detach (stdout/stderr closed)
#[tokio::test]
async fn test_daemon_survives_terminal_detach() {
    let dir = tempdir().unwrap();
    let runtime = TestRuntime::with_home(dir.path());
    runtime.run_cmd(&["start"]).await.unwrap();
    // simulate terminal close
    runtime.close_stdio();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let pid = std::fs::read_to_string(dir.path().join(".mccp/daemon.pid")).unwrap();
    let pid: u32 = pid.trim().parse().unwrap();
    assert!(process_is_running(pid), "daemon should still be running after stdio was closed");
}
```

---

### 10. Logging & Observability Tests

Located: `crates/mccp-logging/src/tests.rs`

```rust
// test: log entry has all required fields
#[test]
fn test_log_entry_has_required_fields() {
    let entry = LogEntry::new(LogLevel::Info, "query_engine", "query completed");
    assert!(entry.ts.is_some());
    assert!(!entry.component.is_empty());
    assert!(!entry.msg.is_empty());
}

// test: log filter by level excludes lower-priority entries
#[test]
fn test_log_filter_by_level() {
    let logs = vec![
        LogEntry::new(LogLevel::Debug, "indexer", "chunk processed"),
        LogEntry::new(LogLevel::Info,  "query_engine", "query done"),
        LogEntry::new(LogLevel::Error, "graph", "cycle detected"),
    ];
    let filtered: Vec<_> = logs.iter().filter(|e| e.level >= LogLevel::Info).collect();
    assert_eq!(filtered.len(), 2);
    assert!(filtered.iter().none(|e| e.level == LogLevel::Debug));
}

// test: log filter by project only returns entries for that project
#[test]
fn test_log_filter_by_project() {
    let logs = vec![
        make_log_entry("proj_a", "query_engine", "done"),
        make_log_entry("proj_b", "indexer", "indexed"),
        make_log_entry("proj_a", "graph", "built"),
    ];
    let filtered: Vec<_> = logs.iter().filter(|e| e.project.as_deref() == Some("proj_a")).collect();
    assert_eq!(filtered.len(), 2);
}

// test: `--json` flag produces one JSON object per line (NDJSON)
#[test]
fn test_json_flag_produces_ndjson() {
    let output = capture_logs_with_flag("--json");
    for line in output.trim().lines() {
        serde_json::from_str::<serde_json::Value>(line)
            .expect("each line should be valid JSON");
    }
}

// test: metrics endpoint returns latency percentiles
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_metrics_returns_latency_percentiles() {
    let ctx = TestContext::start().await;
    ctx.run_several_queries("proj_a", 20).await;
    let metrics = ctx.fetch_metrics().await.unwrap();
    assert!(metrics.query_latency_p50 > 0.0);
    assert!(metrics.query_latency_p95 >= metrics.query_latency_p50);
    assert!(metrics.query_latency_p99 >= metrics.query_latency_p95);
}

// test: feedback store records bad signals correctly
#[test]
fn test_feedback_store_records_bad_signal() {
    let mut store = FeedbackStore::in_memory();
    store.record("q_abc", "proj_a", FeedbackSignal::Bad);
    let entries = store.get_by_project("proj_a");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].signal, FeedbackSignal::Bad);
}

// test: ranker re-tune triggered after 50 bad signals
#[test]
fn test_retuning_triggered_after_threshold() {
    let mut tracker = FeedbackTracker::new(50);
    for _ in 0..49 {
        tracker.record(FeedbackSignal::Bad);
    }
    assert!(!tracker.should_retune());
    tracker.record(FeedbackSignal::Bad);
    assert!(tracker.should_retune(), "retuning should trigger at exactly 50 signals");
}

// test: query log export writes valid file with correct entries
#[tokio::test]
async fn test_query_log_export() {
    let dir = tempdir().unwrap();
    let ctx = TestContext::start().await;
    ctx.run_several_queries("proj_a", 5).await;
    let path = ctx.export_logs(dir.path(), Duration::from_secs(3600)).await.unwrap();
    let content = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<_> = content.lines().collect();
    assert_eq!(lines.len(), 5);
    for line in lines {
        let entry: LogEntry = serde_json::from_str(line).expect("export must produce valid NDJSON");
        assert_eq!(entry.component, "query_engine");
    }
}
```

---

### 12. Provider Abstraction Tests

Located: `crates/mccp-providers/src/tests.rs`

#### Unit Tests (Mock Provider)

```rust
// test: mock embedding provider implements the trait and returns correct dimensions
#[tokio::test]
async fn test_mock_embedding_provider_dimensions() {
    let provider = MockEmbeddingProvider::with_dimensions(768);
    let vecs = provider.embed(&["hello".into(), "world".into()]).await.unwrap();
    assert_eq!(vecs.len(), 2);
    assert!(vecs.iter().all(|v| v.len() == 768));
}

// test: provider fingerprint changes when model changes
#[test]
fn test_fingerprint_changes_on_model_change() {
    let fp_a = OllamaEmbeddingConfig { model: "nomic-embed-text".into(), url: "http://localhost:11434".into() }.fingerprint();
    let fp_b = OllamaEmbeddingConfig { model: "all-minilm".into(),        url: "http://localhost:11434".into() }.fingerprint();
    assert_ne!(fp_a, fp_b);
}

// test: provider fingerprint changes when endpoint changes
#[test]
fn test_fingerprint_changes_on_endpoint_change() {
    let fp_a = OllamaEmbeddingConfig { model: "nomic-embed-text".into(), url: "http://localhost:11434".into() }.fingerprint();
    let fp_b = OllamaEmbeddingConfig { model: "nomic-embed-text".into(), url: "http://remotehost:11434".into() }.fingerprint();
    assert_ne!(fp_a, fp_b);
}

// test: provider fingerprint is stable across restarts (same input → same output)
#[test]
fn test_fingerprint_is_deterministic() {
    let config = OllamaEmbeddingConfig { model: "nomic-embed-text".into(), url: "http://localhost:11434".into() };
    assert_eq!(config.fingerprint(), config.fingerprint());
}

// test: fallback chain tries secondary provider when primary health check fails
#[tokio::test]
async fn test_fallback_chain_on_primary_failure() {
    let primary   = MockEmbeddingProvider::that_is_unhealthy();
    let secondary = MockEmbeddingProvider::with_dimensions(768);
    let chain = FallbackEmbeddingProvider::new(vec![
        Box::new(primary),
        Box::new(secondary),
    ]);
    let health = chain.resolve_active().await;
    assert!(health.is_ok(), "should resolve to secondary when primary is down");
}

// test: fallback does not silently switch during an active query — returns error
#[tokio::test]
async fn test_fallback_does_not_switch_mid_query() {
    let active = MockEmbeddingProvider::that_fails_after(1); // fails on 2nd call
    let chain = FallbackEmbeddingProvider::new(vec![Box::new(active)]);
    chain.embed(&["first call".into()]).await.unwrap();
    let result = chain.embed(&["second call — fails".into()]).await;
    assert!(result.is_err(), "mid-query failure should return error, not silently switch provider");
}

// test: env var MCCP_EMBEDDING_PROVIDER overrides config file
#[test]
fn test_env_var_overrides_config() {
    std::env::set_var("MCCP_EMBEDDING_PROVIDER", "openai");
    std::env::set_var("MCCP_EMBEDDING_MODEL", "text-embedding-3-small");
    let config = ProviderConfig::resolve_from_env_and_file(&dummy_config_file());
    assert_eq!(config.embedding.driver, "openai");
    assert_eq!(config.embedding.model, "text-embedding-3-small");
    std::env::remove_var("MCCP_EMBEDDING_PROVIDER");
    std::env::remove_var("MCCP_EMBEDDING_MODEL");
}

// test: MCCP_LLM_PROVIDER env var overrides config file
#[test]
fn test_env_var_overrides_llm_config() {
    std::env::set_var("MCCP_LLM_PROVIDER", "anthropic");
    std::env::set_var("MCCP_LLM_MODEL", "claude-3-5-haiku-20241022");
    let config = ProviderConfig::resolve_from_env_and_file(&dummy_config_file());
    assert_eq!(config.llm.driver, "anthropic");
    std::env::remove_var("MCCP_LLM_PROVIDER");
    std::env::remove_var("MCCP_LLM_MODEL");
}

// test: MCCP_VECTOR_PROVIDER env var selects correct store driver
#[test]
fn test_env_var_selects_vector_driver() {
    std::env::set_var("MCCP_VECTOR_PROVIDER", "pgvector");
    std::env::set_var("MCCP_VECTOR_URL", "postgresql://user:pass@localhost/mccp");
    let config = ProviderConfig::resolve_from_env_and_file(&dummy_config_file());
    assert_eq!(config.vector.driver, "pgvector");
    std::env::remove_var("MCCP_VECTOR_PROVIDER");
    std::env::remove_var("MCCP_VECTOR_URL");
}

// test: missing API key for cloud provider returns actionable error at startup
#[tokio::test]
async fn test_missing_api_key_returns_clear_error() {
    let config = OpenAIEmbeddingConfig { model: "text-embedding-3-small".into(), api_key: "".into() };
    let provider = OpenAIEmbeddingProvider::new(config);
    let health = provider.health().await;
    assert!(!health.is_healthy);
    assert!(health.error.contains("API key") || health.error.contains("api_key"),
        "error should mention API key, got: {}", health.error);
}

// test: dimensions mismatch between config and provider response is caught at startup
#[tokio::test]
async fn test_dimensions_mismatch_caught_at_startup() {
    let mock = MockEmbeddingProvider::with_dimensions(1536); // returns 1536-dim vectors
    let config = EmbeddingSlotConfig { expected_dimensions: 768, ..Default::default() };
    let result = validate_embedding_provider(&mock, &config).await;
    assert!(result.is_err(), "dimensions mismatch should fail validation");
    assert!(result.unwrap_err().to_string().contains("768") || result.unwrap_err().to_string().contains("1536"));
}

// test: custom-http driver sends correct OpenAI-compatible request format
#[tokio::test]
async fn test_custom_http_embedding_request_format() {
    let server = MockHttpServer::record_requests();
    let provider = CustomHttpEmbeddingProvider::new(CustomHttpConfig {
        url: server.url(),
        model: "my-model".into(),
        api_key: "test-key".into(),
    });
    let _ = provider.embed(&["test input".into()]).await;
    let request = server.last_request().await;
    let body: serde_json::Value = serde_json::from_str(&request.body).unwrap();
    assert_eq!(body["model"], "my-model");
    assert!(body["input"].is_array());
    assert_eq!(request.headers["authorization"], "Bearer test-key");
}

// test: custom-http LLM driver sends correct OpenAI chat completion format
#[tokio::test]
async fn test_custom_http_llm_request_format() {
    let server = MockHttpServer::record_requests();
    let provider = CustomHttpLlmProvider::new(CustomHttpLlmConfig {
        url: server.url(),
        model: "my-llm".into(),
        api_key: "test-key".into(),
    });
    let _ = provider.complete("Summarize this function", None).await;
    let request = server.last_request().await;
    let body: serde_json::Value = serde_json::from_str(&request.body).unwrap();
    assert_eq!(body["model"], "my-llm");
    assert!(body["messages"].is_array());
}
```

#### Integration Tests

```rust
// test: Ollama embedding provider returns vectors of correct dimension
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_ollama_embedding_returns_correct_dims() {
    let provider = OllamaEmbeddingProvider::from_env_or_default().await.unwrap();
    let vecs = provider.embed(&["fn main() {}".into()]).await.unwrap();
    assert_eq!(vecs.len(), 1);
    assert_eq!(vecs[0].len(), provider.dimensions());
}

// test: Ollama LLM provider returns valid JSON summary
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_ollama_llm_returns_valid_json_summary() {
    let provider = OllamaLlmProvider::from_env_or_default().await.unwrap();
    let prompt = summary_prompt_for("fn add(a: i32, b: i32) -> i32 { a + b }");
    let result = provider.complete(&prompt, Some(&SUMMARY_SCHEMA)).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&result)
        .expect("LLM must return parseable JSON for summary prompt");
    assert!(parsed.get("purpose").is_some());
}

// test: switching from Ollama to memory vector store preserves query correctness
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_vector_store_swap_preserves_results() {
    let qdrant_ctx = TestContext::with_vector_driver("qdrant").await;
    qdrant_ctx.index_fixture("proj_a", "auth_service").await;
    let qdrant_results = qdrant_ctx.query("proj_a", "JWT validation").await.unwrap();

    let mem_ctx = TestContext::with_vector_driver("memory").await;
    mem_ctx.index_fixture("proj_a", "auth_service").await;
    let mem_results = mem_ctx.query("proj_a", "JWT validation").await.unwrap();

    // top result should be the same file regardless of backend
    assert_eq!(qdrant_results[0].path, mem_results[0].path,
        "top result should be identical between Qdrant and memory backends");
}

// test: provider change triggers re-index and old fingerprint is replaced
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_provider_change_triggers_reindex() {
    let ctx = TestContext::start().await;
    ctx.index_fixture("proj_a", "small_rust").await;
    let old_fp = ctx.metadata_store.project_fingerprint("proj_a").await.unwrap();

    ctx.set_embedding_provider("openai-mock").await; // switch provider
    ctx.daemon.wait_for_reindex("proj_a").await;

    let new_fp = ctx.metadata_store.project_fingerprint("proj_a").await.unwrap();
    assert_ne!(old_fp, new_fp, "fingerprint must change after provider switch");
}

// test: stale results served during re-index carry stale flag
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_stale_flag_during_provider_reindex() {
    let ctx = TestContext::start().await;
    ctx.index_fixture("proj_a", "small_rust").await;
    ctx.set_embedding_provider("openai-mock").await;
    // query immediately, before re-index completes
    let results = ctx.query("proj_a", "main function").await.unwrap();
    assert!(results.iter().any(|r| r.stale),
        "results must be flagged stale while re-index is in progress");
}
```

---

### 13. Platform Independence Tests

Located: `crates/mccp-platform/src/tests.rs`

```rust
// test: config home resolves to correct path on each platform
#[test]
fn test_config_home_resolves_correctly() {
    let home = config_home();
    // must be an absolute path and exist (or be creatable)
    assert!(home.is_absolute(), "config home must be absolute: {:?}", home);
    #[cfg(target_os = "windows")]
    assert!(home.to_str().unwrap().contains("AppData"), "Windows config must be under AppData");
    #[cfg(not(target_os = "windows"))]
    assert!(home.to_str().unwrap().contains(".mccp"), "Unix config must be under ~/.mccp");
}

// test: all stored paths are canonicalized to forward-slash form
#[test]
fn test_path_normalized_to_forward_slash() {
    let raw = if cfg!(target_os = "windows") {
        r"C:\Users\alice\my-project"
    } else {
        "/home/alice/my-project"
    };
    let normalized = normalize_path(raw);
    assert!(!normalized.contains('\\'), "normalized path must not contain backslashes");
}

// test: path round-trips correctly through serialize/deserialize
#[test]
fn test_path_roundtrip_serialize() {
    let original = std::env::current_dir().unwrap();
    let serialized = serialize_path(&original);
    let deserialized = deserialize_path(&serialized).unwrap();
    assert_eq!(original.canonicalize().unwrap(), deserialized.canonicalize().unwrap());
}

// test: NO_COLOR=1 disables colored output
#[test]
fn test_no_color_env_disables_color() {
    std::env::set_var("NO_COLOR", "1");
    let output = OutputConfig::from_env();
    assert!(!output.color_enabled);
    std::env::remove_var("NO_COLOR");
}

// test: CI=true disables spinner and TUI prompts
#[test]
fn test_ci_env_disables_tui() {
    std::env::set_var("CI", "true");
    let output = OutputConfig::from_env();
    assert!(!output.spinner_enabled);
    assert!(!output.interactive_prompts);
    std::env::remove_var("CI");
}

// test: non-TTY stdout disables spinner automatically
#[test]
fn test_non_tty_disables_spinner() {
    // simulate non-TTY by using a pipe
    let output = OutputConfig::from_stream(FakeStream::NonTty);
    assert!(!output.spinner_enabled);
}

// test: Docker socket path is correct per platform
#[test]
fn test_docker_socket_path_per_platform() {
    let socket = docker_socket_path();
    #[cfg(target_os = "windows")]
    assert!(socket.contains("pipe") || socket.contains("npipe"),
        "Windows Docker must use named pipe, got: {}", socket);
    #[cfg(not(target_os = "windows"))]
    assert_eq!(socket, "/var/run/docker.sock");
}

// test: PID file path is correct per platform
#[test]
fn test_pid_file_path_per_platform() {
    let pid_path = pid_file_path();
    assert!(pid_path.is_absolute());
    assert!(pid_path.to_str().unwrap().ends_with("daemon.pid"));
    #[cfg(target_os = "windows")]
    assert!(pid_path.to_str().unwrap().contains("AppData"));
}
```

---

### 14. Performance Regression Tests

Located: `crates/mccp-perf/src/regression.rs`

Run with: `cargo test --features e2e --release -- --ignored`

```rust
// test: mmap read is faster than buffered read for files over 512KB
#[test]
#[ignore]
fn perf_mmap_faster_than_bufread_large_file() {
    let path = generate_large_source_file(1024 * 1024); // 1MB
    let mmap_time   = bench_file_read(ReadStrategy::Mmap, &path, 100);
    let bufread_time = bench_file_read(ReadStrategy::BufRead, &path, 100);
    assert!(mmap_time < bufread_time,
        "mmap ({:.2}ms) should be faster than bufread ({:.2}ms) for 1MB files",
        mmap_time, bufread_time);
}

// test: parallel parse is at least 3× faster than sequential on 8-core machine
#[test]
#[ignore]
fn perf_parallel_parse_speedup() {
    let files = generate_source_files(200);
    let sequential_ms = bench_parse_sequential(&files);
    let parallel_ms   = bench_parse_parallel(&files);
    let speedup = sequential_ms / parallel_ms;
    assert!(speedup >= 3.0,
        "parallel parse speedup {:.1}× is below 3× on {} files", speedup, files.len());
}

// test: embedding pipeline keeps provider saturated — idle time under 10%
#[tokio::test]
#[ignore]
async fn perf_embedding_pipeline_provider_saturation() {
    let ctx = PerfContext::start().await;
    let chunks = generate_chunks(500);
    let stats = ctx.run_embedding_pipeline_with_stats(&chunks).await;
    let idle_pct = stats.provider_idle_ms as f64 / stats.total_ms as f64 * 100.0;
    assert!(idle_pct < 10.0, "provider idle {:.1}% exceeds 10% — pipeline has a bottleneck", idle_pct);
}

// test: query cache hit serves response under 5ms p99
#[tokio::test]
#[ignore]
async fn perf_cache_hit_under_5ms_p99() {
    let ctx = PerfContext::with_indexed_fixture("medium_monorepo").await;
    // warm the cache
    ctx.query("auth token validation").await.unwrap();
    let latencies = ctx.run_queries_timed(200, "auth token validation").await;
    let p99 = percentile(&latencies, 99.0);
    assert!(p99 < 5.0, "cache hit p99 {:.2}ms exceeds 5ms budget", p99);
}

// test: uncached query p99 under 150ms on 50k-file project
#[tokio::test]
#[ignore]
async fn perf_uncached_query_under_150ms_p99() {
    let ctx = PerfContext::with_fixture("large_monorepo_50k_files").await;
    let latencies = ctx.run_queries_with_cache_bypass(100, "user authentication flow").await;
    let p99 = percentile(&latencies, 99.0);
    assert!(p99 < 150.0, "uncached query p99 {:.2}ms exceeds 150ms budget", p99);
}

// test: daemon startup time under 500ms with 10 registered projects
#[tokio::test]
#[ignore]
async fn perf_daemon_startup_under_500ms() {
    let runtime = PerfRuntime::with_n_registered_projects(10).await;
    let start = Instant::now();
    runtime.start_daemon().await.unwrap();
    runtime.wait_for_first_query_served().await;
    let elapsed_ms = start.elapsed().as_millis();
    assert!(elapsed_ms < 500, "daemon startup took {}ms, budget is 500ms", elapsed_ms);
}

// test: incremental index of 100 changed files under 15 seconds (local Ollama)
#[tokio::test]
#[ignore]
async fn perf_incremental_index_100_files_under_15s() {
    let ctx = PerfContext::with_indexed_fixture("medium_monorepo").await;
    ctx.modify_n_files(100).await;
    let start = Instant::now();
    ctx.indexer.run_incremental().await.unwrap();
    let elapsed = start.elapsed().as_secs_f64();
    assert!(elapsed < 15.0, "incremental index of 100 files took {:.1}s, budget 15s", elapsed);
}

// test: zero-copy chunk passing — no extra allocation between parse and embed
#[test]
#[ignore]
fn perf_zero_copy_chunk_allocation() {
    let source = generate_large_source(10_000);
    let alloc_before = allocation_counter::count();
    let chunks = chunk_source_zero_copy(&source, Language::Rust, ChunkConfig::default());
    let alloc_after = allocation_counter::count();
    let allocs_per_chunk = (alloc_after - alloc_before) as f64 / chunks.len() as f64;
    assert!(allocs_per_chunk < 2.0,
        "expected < 2 allocations per chunk, got {:.1}", allocs_per_chunk);
}
```
