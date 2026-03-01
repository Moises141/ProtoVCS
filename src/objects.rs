use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// The staging area: maps relative file paths to their SHA-256 object hashes.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Index {
    pub entries: HashMap<String, String>,
}

/// A tree object records a snapshot of the index (path → blob hash).
#[derive(Debug, Serialize, Deserialize)]
pub struct Tree {
    pub entries: HashMap<String, String>,
}

impl Tree {
    /// Build a Tree from the current index.
    pub fn from_index(index: Index) -> Self {
        Tree {
            entries: index.entries,
        }
    }

    /// Returns true when the tree contains no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Compute the deterministic SHA-256 hash of this tree's canonical JSON.
    pub fn hash(&self) -> String {
        // Sort keys so the hash is deterministic regardless of insertion order.
        let mut sorted: Vec<(&String, &String)> = self.entries.iter().collect();
        sorted.sort_by_key(|(k, _)| *k);
        let canonical = serde_json::to_string(&sorted).expect("tree serialization failed");
        hex::encode(Sha256::digest(canonical.as_bytes()))
    }

    /// Persist the tree as a loose object under `.protovcs/objects/<hash[..2]>/<hash[2..]>`.
    pub fn store(&self, root: &PathBuf) -> Result<String> {
        let hash = self.hash();
        let obj_dir = root.join(".protovcs/objects").join(&hash[..2]);
        fs::create_dir_all(&obj_dir)?;
        fs::write(obj_dir.join(&hash[2..]), serde_json::to_vec(self)?)?;
        Ok(hash)
    }
}

/// A commit object links a tree snapshot to its history and metadata.
#[derive(Debug, Serialize, Deserialize)]
pub struct Commit {
    /// Hash of the root Tree object.
    pub tree: String,
    /// Zero or more parent commit hashes.
    pub parents: Vec<String>,
    /// Hex-encoded ed25519 public key of the author.
    pub author: String,
    /// Unix timestamp (seconds since epoch).
    pub timestamp: u64,
    /// Human-readable commit message.
    pub message: String,
}

impl Commit {
    /// Compute the deterministic SHA-256 hash of this commit's canonical JSON.
    pub fn hash(&self) -> String {
        let canonical = serde_json::to_string(self).expect("commit serialization failed");
        hex::encode(Sha256::digest(canonical.as_bytes()))
    }

    /// Persist the commit as a loose object and return its hash.
    pub fn store(&self, root: &PathBuf) -> Result<String> {
        let hash = self.hash();
        let obj_dir = root.join(".protovcs/objects").join(&hash[..2]);
        fs::create_dir_all(&obj_dir)?;
        fs::write(obj_dir.join(&hash[2..]), serde_json::to_vec(self)?)?;
        Ok(hash)
    }
}