/// Indexing engine: file walking, MerkleDAG incremental sync, chunking,
/// embedding, and vector store upsert.
///
/// Intentionally self-contained — does not depend on the mccp-core crate.
use crate::config::ProjectConfig;
use crate::embeddings::EmbeddingClient;
use crate::system_config::SystemConfig;
use crate::vector_store::{ChunkPayload, ChunkPoint, VectorStoreClient};
use anyhow::Context;
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;
use uuid::Uuid;

// ─── MerkleDAG snapshot ───────────────────────────────────────────────────────

/// Snapshot of a project's file state.
/// The root hash provides O(1) change detection across the entire project.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MerkleSnapshot {
    pub root_hash: String,
    pub file_hashes: BTreeMap<String, String>,
}

impl MerkleSnapshot {
    /// Build a snapshot from a path→sha256 map.
    pub fn build(file_hashes: BTreeMap<String, String>) -> Self {
        let combined = file_hashes
            .iter()
            .map(|(p, h)| format!("{}:{}", p, h))
            .collect::<Vec<_>>()
            .join("\n");
        let root_hash = format!("{:x}", Sha256::digest(combined.as_bytes()));
        Self { root_hash, file_hashes }
    }

    /// Compare self (old) with `new`.  Returns `None` when nothing changed.
    pub fn diff(&self, new: &MerkleSnapshot) -> Option<SnapshotDiff> {
        if !self.root_hash.is_empty() && self.root_hash == new.root_hash {
            return None; // O(1) identical check
        }
        let mut added = Vec::new();
        let mut removed = Vec::new();
        let mut modified = Vec::new();

        for (path, new_hash) in &new.file_hashes {
            match self.file_hashes.get(path) {
                None => added.push(path.clone()),
                Some(old) if old != new_hash => modified.push(path.clone()),
                _ => {}
            }
        }
        for path in self.file_hashes.keys() {
            if !new.file_hashes.contains_key(path) {
                removed.push(path.clone());
            }
        }
        Some(SnapshotDiff { added, removed, modified })
    }
}

/// What changed between two snapshots.
#[derive(Debug, Default)]
pub struct SnapshotDiff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub modified: Vec<String>,
}

impl SnapshotDiff {
    pub fn to_index(&self) -> Vec<String> {
        self.added.iter().chain(self.modified.iter()).cloned().collect()
    }

    pub fn total_changed(&self) -> usize {
        self.added.len() + self.modified.len() + self.removed.len()
    }
}

// ─── Text chunk ───────────────────────────────────────────────────────────────

struct TextChunk {
    content: String,
    path: String,
    start_line: usize,
    end_line: usize,
}

// ─── Progress ─────────────────────────────────────────────────────────────────

pub enum IndexProgress<'a> {
    Scanning,
    FilesFound { total: usize, changed: usize },
    IndexingFile { path: &'a str, current: usize, total: usize },
    FileError { path: &'a str, error: &'a str },
    Done(IndexStats),
}

// ─── Stats ────────────────────────────────────────────────────────────────────

#[derive(Debug, Default, Clone)]
pub struct IndexStats {
    pub files_indexed: usize,
    pub files_unchanged: usize,
    pub files_removed: usize,
    pub chunks_created: usize,
    pub duration_secs: f64,
}

// ─── Indexer ──────────────────────────────────────────────────────────────────

pub struct Indexer {
    embedding: EmbeddingClient,
    store: VectorStoreClient,
    data_dir: PathBuf,
    chunk_size_lines: usize,
    overlap_lines: usize,
}

impl Indexer {
    pub fn new(cfg: &SystemConfig) -> anyhow::Result<Self> {
        let embedding = EmbeddingClient::from_config(&cfg.embedding)?;
        let vc = &cfg.vector;
        let store = VectorStoreClient::new(
            &vc.url,
            if vc.api_key.is_empty() { None } else { Some(vc.api_key.clone()) },
        );
        let data_dir = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("cannot determine home directory"))?
            .join(".mccp")
            .join("data");
        std::fs::create_dir_all(&data_dir)?;
        Ok(Self { embedding, store, data_dir, chunk_size_lines: 60, overlap_lines: 10 })
    }

    /// Full index/re-index a project, calling `on_progress` as work proceeds.
    pub async fn index(
        &self,
        project: &ProjectConfig,
        force_full: bool,
        mut on_progress: impl FnMut(IndexProgress<'_>),
    ) -> anyhow::Result<IndexStats> {
        let start = Instant::now();
        let collection = sanitize_name(&project.name);

        on_progress(IndexProgress::Scanning);

        // 1. Walk and hash every tracked file
        let new_hashes = self.scan_hashes(&project.path)?;
        let new_snapshot = MerkleSnapshot::build(new_hashes);
        let total_files = new_snapshot.file_hashes.len();

        // 2. Load previous snapshot
        let snap_path = self.snapshot_path(&project.name);
        let old_snapshot = self.load_snapshot(&snap_path);

        // 3. Determine what needs work
        let (files_to_index, files_to_remove): (Vec<String>, Vec<String>) = if force_full {
            (new_snapshot.file_hashes.keys().cloned().collect(), vec![])
        } else {
            match old_snapshot.diff(&new_snapshot) {
                None => {
                    // Nothing changed — fast exit
                    let stats = IndexStats {
                        files_unchanged: total_files,
                        duration_secs: start.elapsed().as_secs_f64(),
                        ..Default::default()
                    };
                    on_progress(IndexProgress::FilesFound { total: total_files, changed: 0 });
                    on_progress(IndexProgress::Done(stats.clone()));
                    return Ok(stats);
                }
                Some(diff) => {
                    let to_index = diff.to_index();
                    let to_remove = diff.removed.clone();
                    (to_index, to_remove)
                }
            }
        };

        on_progress(IndexProgress::FilesFound {
            total: total_files,
            changed: files_to_index.len(),
        });

        // 4. Ensure the vector collection exists
        let dims = if self.embedding.dimensions > 0 { self.embedding.dimensions } else { 768 };
        self.store
            .ensure_collection(&collection, dims)
            .await
            .context("cannot connect to vector store — is Qdrant running?")?;

        // 5. Delete removed/modified files from store (before re-indexing)
        let mut files_removed = 0;
        for path in &files_to_remove {
            if let Err(e) = self.store.delete_by_path(&collection, path).await {
                eprintln!("  Warning: could not delete {}: {}", path, e);
            } else {
                files_removed += 1;
            }
        }
        // Also delete modified files so they get fresh chunks
        for path in files_to_index.iter().filter(|p| {
            old_snapshot.file_hashes.contains_key(*p)
        }) {
            let _ = self.store.delete_by_path(&collection, path).await;
        }

        // 6. Index files
        let total_to_index = files_to_index.len();
        let mut files_indexed = 0;
        let mut chunks_created = 0;

        for (i, file_path) in files_to_index.iter().enumerate() {
            on_progress(IndexProgress::IndexingFile {
                path: file_path,
                current: i + 1,
                total: total_to_index,
            });

            match self.index_file(file_path, &project.name, &collection).await {
                Ok(n) => {
                    files_indexed += 1;
                    chunks_created += n;
                }
                Err(e) => {
                    let msg = e.to_string();
                    on_progress(IndexProgress::FileError { path: file_path, error: &msg });
                }
            }
        }

        // 7. Save new snapshot
        if let Err(e) = self.save_snapshot(&snap_path, &new_snapshot) {
            eprintln!("  Warning: failed to save snapshot: {}", e);
        }

        let stats = IndexStats {
            files_indexed,
            files_unchanged: total_files.saturating_sub(files_indexed),
            files_removed,
            chunks_created,
            duration_secs: start.elapsed().as_secs_f64(),
        };
        on_progress(IndexProgress::Done(stats.clone()));
        Ok(stats)
    }

    /// Embed a query string and search the vector store.
    pub async fn search(
        &self,
        project_name: &str,
        query: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<crate::vector_store::SearchHit>> {
        let collection = sanitize_name(project_name);
        let vector = self.embedding.embed(query).await.context("embedding query")?;
        self.store.search(&collection, &vector, limit).await
    }

    // ── internals ─────────────────────────────────────────────────────────────

    async fn index_file(
        &self,
        file_path: &str,
        project_name: &str,
        collection: &str,
    ) -> anyhow::Result<usize> {
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("reading {}", file_path))?;
        if content.trim().is_empty() {
            return Ok(0);
        }

        let chunks = self.chunk_file(file_path, &content);
        if chunks.is_empty() {
            return Ok(0);
        }

        let mut points = Vec::with_capacity(chunks.len());
        for chunk in &chunks {
            let vector = self
                .embedding
                .embed(&chunk.content)
                .await
                .with_context(|| format!("embedding chunk from {}", file_path))?;
            points.push(ChunkPoint {
                id: Uuid::new_v4().to_string(),
                vector,
                payload: ChunkPayload {
                    path: file_path.to_string(),
                    content: chunk.content.clone(),
                    start_line: chunk.start_line,
                    end_line: chunk.end_line,
                    project: project_name.to_string(),
                },
            });
        }

        let n = points.len();
        // Upsert in batches of 64 to avoid request-size limits
        for batch in points.chunks(64) {
            self.store
                .upsert_points(collection, batch.to_vec())
                .await
                .with_context(|| format!("upserting chunks for {}", file_path))?;
        }
        Ok(n)
    }

    /// Walk `root` respecting `.gitignore` and return a path→sha256 map.
    fn scan_hashes(&self, root: &Path) -> anyhow::Result<BTreeMap<String, String>> {
        let mut hashes = BTreeMap::new();

        for entry in WalkBuilder::new(root)
            .hidden(false)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .build()
        {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                continue;
            }
            let path = entry.path();
            if is_binary_path(path) {
                continue;
            }
            if std::fs::metadata(path).map(|m| m.len()).unwrap_or(0) > MAX_FILE_BYTES {
                continue;
            }
            let bytes = match std::fs::read(path) {
                Ok(b) => b,
                Err(_) => continue,
            };
            if bytes.contains(&0u8) {
                continue; // binary content
            }
            let hash = format!("{:x}", Sha256::digest(&bytes));
            hashes.insert(path.to_string_lossy().to_string(), hash);
        }
        Ok(hashes)
    }

    /// Line-based chunker with overlap.
    fn chunk_file(&self, path: &str, content: &str) -> Vec<TextChunk> {
        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() {
            return vec![];
        }
        let step = self.chunk_size_lines.saturating_sub(self.overlap_lines).max(1);
        let mut chunks = Vec::new();
        let mut start = 0;

        while start < lines.len() {
            let end = (start + self.chunk_size_lines).min(lines.len());
            let text = lines[start..end].join("\n");
            if !text.trim().is_empty() {
                // Prepend the file path so the chunk is self-describing for the LLM
                chunks.push(TextChunk {
                    content: format!("// {}\n{}", path, text),
                    path: path.to_string(),
                    start_line: start + 1,
                    end_line: end,
                });
            }
            if end == lines.len() {
                break;
            }
            start += step;
        }
        chunks
    }

    fn snapshot_path(&self, project_name: &str) -> PathBuf {
        let dir = self.data_dir.join(sanitize_name(project_name));
        let _ = std::fs::create_dir_all(&dir);
        dir.join("snapshot.json")
    }

    fn load_snapshot(&self, path: &Path) -> MerkleSnapshot {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save_snapshot(&self, path: &Path, snapshot: &MerkleSnapshot) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(snapshot)?;
        std::fs::write(path, json)?;
        Ok(())
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

const MAX_FILE_BYTES: u64 = 512 * 1024; // 512 KB

const BINARY_EXTENSIONS: &[&str] = &[
    // images
    "png", "jpg", "jpeg", "gif", "bmp", "ico", "svg", "webp", "tiff",
    // documents
    "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx",
    // archives
    "zip", "tar", "gz", "bz2", "xz", "7z", "rar",
    // binaries / object files
    "exe", "dll", "so", "dylib", "lib", "a", "o", "wasm",
    // jvm
    "class", "jar", "war",
    // media
    "mp3", "mp4", "wav", "avi", "mov", "mkv", "flac",
    // fonts
    "ttf", "otf", "woff", "woff2", "eot",
    // package locks (too noisy, index-irrelevant)
    "lock",
];

fn is_binary_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| BINARY_EXTENSIONS.contains(&ext))
        .unwrap_or(false)
}

fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect::<String>()
        .to_lowercase()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use tempfile::tempdir;

    // ── MerkleSnapshot ────────────────────────────────────────────────────────

    #[test]
    fn snapshot_identical_returns_none_diff() {
        let mut hashes = BTreeMap::new();
        hashes.insert("a.rs".to_string(), "hash1".to_string());
        hashes.insert("b.rs".to_string(), "hash2".to_string());
        let snap = MerkleSnapshot::build(hashes);
        assert!(snap.diff(&snap.clone()).is_none());
    }

    #[test]
    fn snapshot_detects_added_file() {
        let mut old = BTreeMap::new();
        old.insert("a.rs".to_string(), "h1".to_string());
        let old_snap = MerkleSnapshot::build(old);

        let mut new = BTreeMap::new();
        new.insert("a.rs".to_string(), "h1".to_string());
        new.insert("b.rs".to_string(), "h2".to_string()); // added
        let new_snap = MerkleSnapshot::build(new);

        let diff = old_snap.diff(&new_snap).expect("should have diff");
        assert_eq!(diff.added, vec!["b.rs"]);
        assert!(diff.removed.is_empty());
        assert!(diff.modified.is_empty());
    }

    #[test]
    fn snapshot_detects_modified_file() {
        let mut old = BTreeMap::new();
        old.insert("a.rs".to_string(), "old-hash".to_string());
        let old_snap = MerkleSnapshot::build(old);

        let mut new = BTreeMap::new();
        new.insert("a.rs".to_string(), "new-hash".to_string()); // modified
        let new_snap = MerkleSnapshot::build(new);

        let diff = old_snap.diff(&new_snap).expect("should have diff");
        assert_eq!(diff.modified, vec!["a.rs"]);
        assert!(diff.added.is_empty());
        assert!(diff.removed.is_empty());
    }

    #[test]
    fn snapshot_detects_removed_file() {
        let mut old = BTreeMap::new();
        old.insert("a.rs".to_string(), "h1".to_string());
        old.insert("b.rs".to_string(), "h2".to_string());
        let old_snap = MerkleSnapshot::build(old);

        let mut new = BTreeMap::new();
        new.insert("a.rs".to_string(), "h1".to_string());
        // b.rs removed
        let new_snap = MerkleSnapshot::build(new);

        let diff = old_snap.diff(&new_snap).expect("should have diff");
        assert_eq!(diff.removed, vec!["b.rs"]);
        assert!(diff.added.is_empty());
    }

    #[test]
    fn snapshot_root_hash_changes_on_modification() {
        let mut m1 = BTreeMap::new();
        m1.insert("a.rs".to_string(), "h1".to_string());
        let s1 = MerkleSnapshot::build(m1);

        let mut m2 = BTreeMap::new();
        m2.insert("a.rs".to_string(), "h2".to_string());
        let s2 = MerkleSnapshot::build(m2);

        assert_ne!(s1.root_hash, s2.root_hash);
    }

    #[test]
    fn snapshot_roundtrip_via_json() {
        let mut m = BTreeMap::new();
        m.insert("src/lib.rs".to_string(), "abc123".to_string());
        let snap = MerkleSnapshot::build(m);

        let json = serde_json::to_string(&snap).unwrap();
        let back: MerkleSnapshot = serde_json::from_str(&json).unwrap();

        assert_eq!(snap.root_hash, back.root_hash);
        assert_eq!(snap.file_hashes, back.file_hashes);
    }

    #[test]
    fn empty_snapshot_diff_against_new_files_is_all_added() {
        let old = MerkleSnapshot::default();
        let mut new_hashes = BTreeMap::new();
        new_hashes.insert("x.rs".to_string(), "h".to_string());
        let new_snap = MerkleSnapshot::build(new_hashes);

        let diff = old.diff(&new_snap).expect("should differ");
        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.added[0], "x.rs");
    }

    // ── Chunker ───────────────────────────────────────────────────────────────

    fn make_indexer_stub() -> Indexer {
        use crate::system_config::SystemConfig;
        // We only test the pure helper methods — no network calls
        let cfg = SystemConfig::default();
        let embedding = EmbeddingClient::from_config(&cfg.embedding).unwrap();
        let store = VectorStoreClient::new("http://localhost:6333", None);
        Indexer {
            embedding,
            store,
            data_dir: std::env::temp_dir(),
            chunk_size_lines: 5,
            overlap_lines: 1,
        }
    }

    #[test]
    fn chunk_file_produces_overlapping_chunks() {
        let idx = make_indexer_stub();
        let content = (1..=12).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
        let chunks = idx.chunk_file("test.rs", &content);
        // With size=5 and overlap=1, step=4: starts at 0, 4, 8 → 3 chunks
        assert!(!chunks.is_empty());
        // Each chunk should include the file path comment
        assert!(chunks[0].content.starts_with("// test.rs\n"));
        // Line numbers should be consistent
        for w in chunks.windows(2) {
            assert!(w[1].start_line > w[0].start_line);
        }
    }

    #[test]
    fn chunk_file_empty_content_returns_empty() {
        let idx = make_indexer_stub();
        let chunks = idx.chunk_file("empty.rs", "   \n  \n ");
        assert!(chunks.is_empty());
    }

    #[test]
    fn chunk_file_single_chunk_for_short_file() {
        let idx = make_indexer_stub();
        let content = "fn main() {}\n";
        let chunks = idx.chunk_file("main.rs", content);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].start_line, 1);
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    #[test]
    fn sanitize_name_lowercases_and_replaces_special_chars() {
        assert_eq!(sanitize_name("My Project!"), "my_project_");
        assert_eq!(sanitize_name("mccp"), "mccp");
        assert_eq!(sanitize_name("my-project"), "my-project");
    }

    #[test]
    fn is_binary_path_detects_known_extensions() {
        assert!(is_binary_path(Path::new("image.png")));
        assert!(is_binary_path(Path::new("archive.tar")));
        assert!(!is_binary_path(Path::new("main.rs")));
        assert!(!is_binary_path(Path::new("README.md")));
    }

    // ── scan_hashes ───────────────────────────────────────────────────────────

    #[test]
    fn scan_hashes_indexes_text_files_and_skips_binary() {
        let tmp = tempdir().unwrap();
        std::fs::write(tmp.path().join("hello.rs"), "fn main() {}").unwrap();
        std::fs::write(tmp.path().join("data.png"), b"\x89PNG\r\n\x1a\n" as &[u8]).unwrap();

        let idx = make_indexer_stub();
        let hashes = idx.scan_hashes(tmp.path()).unwrap();

        // hello.rs should be indexed
        let has_rs = hashes.keys().any(|k| k.ends_with("hello.rs"));
        assert!(has_rs, "expected hello.rs in hashes");

        // data.png should be skipped
        let has_png = hashes.keys().any(|k| k.ends_with("data.png"));
        assert!(!has_png, "png should be excluded");
    }

    #[test]
    fn scan_hashes_skips_null_byte_files() {
        let tmp = tempdir().unwrap();
        std::fs::write(tmp.path().join("binary.dat"), b"data\x00more").unwrap();
        std::fs::write(tmp.path().join("text.txt"), "hello world").unwrap();

        let idx = make_indexer_stub();
        let hashes = idx.scan_hashes(tmp.path()).unwrap();

        assert!(!hashes.keys().any(|k| k.ends_with("binary.dat")));
        assert!(hashes.keys().any(|k| k.ends_with("text.txt")));
    }

    #[test]
    fn snapshot_diff_to_index_combines_added_and_modified() {
        let diff = SnapshotDiff {
            added: vec!["a.rs".into()],
            modified: vec!["b.rs".into()],
            removed: vec!["c.rs".into()],
        };
        let to_index = diff.to_index();
        assert_eq!(to_index.len(), 2);
        assert!(to_index.contains(&"a.rs".to_string()));
        assert!(to_index.contains(&"b.rs".to_string()));
    }
}
