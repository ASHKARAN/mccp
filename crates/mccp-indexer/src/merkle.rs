use sha2::{Sha256, Digest};
use std::collections::BTreeMap;
use serde::{Serialize, Deserialize};

/// Lightweight snapshot of a directory state used for O(1) change detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleSnapshot {
    /// sha256 of all (path:hash) pairs concatenated in sorted path order.
    pub root_hash: String,
    /// file_path → sha256(content) for every tracked file.
    pub file_hashes: BTreeMap<String, String>,
}

impl MerkleSnapshot {
    /// Build a Merkle snapshot from file hashes
    pub fn build(file_hashes: BTreeMap<String, String>) -> Self {
        let combined: String = file_hashes
            .iter()
            .map(|(p, h)| format!("{}:{}", p, h))
            .collect::<Vec<_>>()
            .join("\n");
        let root_hash = format!("{:x}", Sha256::digest(combined.as_bytes()));
        Self { root_hash, file_hashes }
    }

    /// Compare two snapshots and return diff if changed
    pub fn diff(&self, new: &MerkleSnapshot) -> Option<SnapshotDiff> {
        if self.root_hash == new.root_hash {
            return None; // identical — O(1) exit
        }
        let mut added = vec![];
        let mut removed = vec![];
        let mut modified = vec![];

        for (path, new_hash) in &new.file_hashes {
            match self.file_hashes.get(path) {
                None => added.push(path.clone()),
                Some(old_hash) if old_hash != new_hash => modified.push(path.clone()),
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

/// Difference between two snapshots
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SnapshotDiff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub modified: Vec<String>,
}

impl SnapshotDiff {
    /// Check if there are any changes
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.modified.is_empty()
    }
    
    /// Get total number of changed files
    pub fn total_changed(&self) -> usize {
        self.added.len() + self.modified.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merkle_snapshot_build() {
        let mut hashes = BTreeMap::new();
        hashes.insert("src/main.rs".to_string(), "abc123".to_string());
        hashes.insert("src/lib.rs".to_string(), "def456".to_string());
        
        let snapshot = MerkleSnapshot::build(hashes);
        
        assert!(!snapshot.root_hash.is_empty());
        assert_eq!(snapshot.file_hashes.len(), 2);
    }

    #[test]
    fn test_merkle_snapshot_diff_identical() {
        let mut hashes = BTreeMap::new();
        hashes.insert("src/main.rs".to_string(), "abc123".to_string());
        
        let snapshot1 = MerkleSnapshot::build(hashes.clone());
        let snapshot2 = MerkleSnapshot::build(hashes);
        
        let diff = snapshot1.diff(&snapshot2);
        assert!(diff.is_none());
    }

    #[test]
    fn test_merkle_snapshot_diff_changes() {
        let mut hashes1 = BTreeMap::new();
        hashes1.insert("src/main.rs".to_string(), "abc123".to_string());
        hashes1.insert("src/lib.rs".to_string(), "def456".to_string());
        
        let mut hashes2 = BTreeMap::new();
        hashes2.insert("src/main.rs".to_string(), "abc123".to_string());
        hashes2.insert("src/lib.rs".to_string(), "modified".to_string());
        hashes2.insert("src/new.rs".to_string(), "newfile".to_string());
        
        let snapshot1 = MerkleSnapshot::build(hashes1);
        let snapshot2 = MerkleSnapshot::build(hashes2);
        
        let diff = snapshot1.diff(&snapshot2).unwrap();
        
        assert_eq!(diff.modified, vec!["src/lib.rs"]);
        assert_eq!(diff.added, vec!["src/new.rs"]);
        assert!(diff.removed.is_empty());
    }

    #[test]
    fn test_snapshot_diff_empty() {
        let diff = SnapshotDiff::default();
        assert!(diff.is_empty());
        assert_eq!(diff.total_changed(), 0);
    }
}