// src/status.rs
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use sha2::{Digest, Sha256};
use hex;
use walkdir::WalkDir;
use crate::objects::{Index, Commit};

// ── Public API ───────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct Status {
    pub staged: Vec<Change>,
    pub unstaged: Vec<Change>,
    pub untracked: Vec<String>,
}

#[derive(Debug)]
pub enum ChangeType {
    New,
    Modified,
    Deleted,
}

#[derive(Debug)]
pub struct Change {
    pub path: String,
    pub change_type: ChangeType,
}

pub fn get_status(root: &PathBuf) -> Result<Status> {
    let head_tree = get_head_tree(root)?;
    let index = get_index(root)?;
    let working = scan_working_tree(root)?;

    let mut status = Status::default();

    // 1. Staged changes (index vs HEAD)
    compute_staged(&head_tree, &index, &mut status.staged);

    // 2. Unstaged changes (working vs index)
    compute_unstaged(&working, &index, &mut status.unstaged);

    // 3. Untracked (in working but not in index or HEAD)
    compute_untracked(&working, &index, &head_tree, &mut status.untracked);

    Ok(status)
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn get_head_tree(root: &PathBuf) -> Result<HashMap<String, String>> {
    let head_path = root.join(".protovcs/HEAD");
    if !head_path.exists() {
        return Ok(HashMap::new());
    }

    let head_content = std::fs::read_to_string(&head_path)?.trim().to_string();

    // Resolve the commit hash from HEAD (either a ref or a direct hash)
    let commit_hash = if head_content.starts_with("ref: ") {
        let ref_name = &head_content[5..];
        let ref_path = root.join(".protovcs").join(ref_name);
        if !ref_path.exists() {
            return Ok(HashMap::new()); // No commits yet on this branch
        }
        std::fs::read_to_string(&ref_path)?.trim().to_string()
    } else {
        head_content
    };

    if commit_hash.is_empty() {
        return Ok(HashMap::new());
    }

    // Load the commit object
    let commit_obj_path = root
        .join(".protovcs/objects")
        .join(&commit_hash[..2])
        .join(&commit_hash[2..]);

    if !commit_obj_path.exists() {
        return Ok(HashMap::new());
    }

    let commit: Commit = serde_json::from_reader(std::fs::File::open(&commit_obj_path)?)?;

    // Load the tree object
    let tree_hash = &commit.tree;
    let tree_obj_path = root
        .join(".protovcs/objects")
        .join(&tree_hash[..2])
        .join(&tree_hash[2..]);

    if !tree_obj_path.exists() {
        return Ok(HashMap::new());
    }

    // Tree is stored as a Tree struct with an `entries` field, not a bare HashMap
    let tree: crate::objects::Tree =
        serde_json::from_reader(std::fs::File::open(&tree_obj_path)?)?;

    Ok(tree.entries)
}

fn get_index(root: &PathBuf) -> Result<HashMap<String, String>> {
    let index_path = root.join(".protovcs/index");
    if !index_path.exists() {
        return Ok(HashMap::new());
    }
    let index: Index = serde_json::from_reader(std::fs::File::open(index_path)?)?;
    Ok(index.entries)
}

fn scan_working_tree(root: &PathBuf) -> Result<HashMap<String, String>> {
    let mut map = HashMap::new();
    let protovcs_dir = root.join(".protovcs");
    
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let entry: walkdir::DirEntry = entry;
        
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.path().starts_with(&protovcs_dir) {
            continue;
        }
        
        let rel_path = entry
            .path()
            .strip_prefix(root)?
            .to_string_lossy()
            .replace('\\', "/");
        let content = std::fs::read(entry.path())?;
        let hash = hex::encode(Sha256::digest(&content));
        map.insert(rel_path, hash);
    }
    Ok(map)
}

fn compute_staged(
    head: &HashMap<String, String>,
    index: &HashMap<String, String>,
    staged: &mut Vec<Change>,
) {
    // New: in index but not in HEAD
    for (path, _) in index {
        if !head.contains_key(path) {
            staged.push(Change {
                path: path.clone(),
                change_type: ChangeType::New,
            });
        }
    }

    // Modified: in both index and HEAD but with different hash
    for (path, index_hash) in index {
        if let Some(head_hash) = head.get(path) {
            if index_hash != head_hash {
                staged.push(Change {
                    path: path.clone(),
                    change_type: ChangeType::Modified,
                });
            }
        }
    }

    // Deleted: in HEAD but not in index
    for (path, _) in head {
        if !index.contains_key(path) {
            staged.push(Change {
                path: path.clone(),
                change_type: ChangeType::Deleted,
            });
        }
    }
}

fn compute_unstaged(
    working: &HashMap<String, String>,
    index: &HashMap<String, String>,
    unstaged: &mut Vec<Change>,
) {
    // Modified: in both working tree and index but with different hash
    for (path, index_hash) in index {
        if let Some(working_hash) = working.get(path) {
            if working_hash != index_hash {
                unstaged.push(Change {
                    path: path.clone(),
                    change_type: ChangeType::Modified,
                });
            }
        } else {
            // Deleted: in index but not in working tree
            unstaged.push(Change {
                path: path.clone(),
                change_type: ChangeType::Deleted,
            });
        }
    }
}

fn compute_untracked(
    working: &HashMap<String, String>,
    index: &HashMap<String, String>,
    head: &HashMap<String, String>,
    untracked: &mut Vec<String>,
) {
    for path in working.keys() {
        if !index.contains_key(path) && !head.contains_key(path) {
            untracked.push(path.clone());
        }
    }
}