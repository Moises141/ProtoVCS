// src/log.rs
use anyhow::Result;
use chrono::{DateTime, Local, TimeZone};
use std::path::PathBuf;

use crate::objects::Commit;

/// A resolved commit record ready for display.
pub struct LogEntry {
    pub hash: String,
    pub commit: Commit,
    /// True only for the tip commit (the one HEAD currently points to).
    pub is_head: bool,
    /// Branch name HEAD resolves through, if any (e.g. "main").
    pub branch: Option<String>,
}

/// Walk commit history from HEAD backward, following first-parent links.
/// Returns entries in reverse-chronological order (newest first).
pub fn get_log(root: &PathBuf) -> Result<Vec<LogEntry>> {
    let (start_hash, branch) = resolve_head(root)?;

    let start_hash = match start_hash {
        Some(h) => h,
        None => return Ok(Vec::new()), // no commits yet
    };

    let mut entries = Vec::new();
    let mut current_hash = start_hash;
    let mut first = true;

    loop {
        let commit = load_commit(root, &current_hash)?;

        // Peek at the first parent before we move current_hash.
        let next_hash = commit.parents.first().cloned();

        entries.push(LogEntry {
            hash: current_hash,
            is_head: first,
            branch: if first { branch.clone() } else { None },
            commit,
        });

        first = false;

        match next_hash {
            Some(h) => current_hash = h,
            None => break, // root commit — we're done
        }
    }

    Ok(entries)
}

/// Pretty-print the log to stdout in a git-like format.
pub fn print_log(entries: &[LogEntry], oneline: bool) {
    if entries.is_empty() {
        println!("No commits yet.");
        return;
    }

    if oneline {
        for entry in entries {
            let short = &entry.hash[..8];
            let msg = entry.commit.message.lines().next().unwrap_or("").trim();
            let dec = match (&entry.branch, entry.is_head) {
                (Some(branch), true) => format!(" [HEAD -> {}]", branch),
                _ => String::new(),
            };
            // No ANSI codes in oneline — output is compact and script-friendly.
            println!("{}{} {}", short, dec, msg);
        }
    } else {
        for entry in entries {
            print_entry(entry);
        }
    }
}

fn print_entry(entry: &LogEntry) {
    // ── Header line ──────────────────────────────────────────────────────────
    let decoration = match (&entry.branch, entry.is_head) {
        (Some(branch), true) => format!(" [HEAD -> {}]", branch),
        _ => String::new(),
    };

    // Yellow "commit <hash><decoration>" mirroring git's default output.
    println!(
        "\x1b[33mcommit {}{}\x1b[0m",
        entry.hash, decoration
    );

    // ── Author ───────────────────────────────────────────────────────────────
    // Full pubkey is 64 hex chars; show it in full so users can cross-reference
    // with `proto whoami` or peer lists.  We truncate only for readability in
    // the short form — keep long form here so nothing is ambiguous.
    println!("Author: {}", entry.commit.author);

    // ── Date ─────────────────────────────────────────────────────────────────
    let formatted = format_timestamp(entry.commit.timestamp);
    println!("Date:   {}", formatted);

    // ── Message ──────────────────────────────────────────────────────────────
    // Indent every line of the message by four spaces, matching git.
    println!();
    for line in entry.commit.message.lines() {
        println!("    {}", line);
    }
    println!();
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Resolve HEAD to (commit_hash, branch_name).
/// Returns (None, None) when the repo has no commits yet.
fn resolve_head(root: &PathBuf) -> Result<(Option<String>, Option<String>)> {
    let head_path = root.join(".protovcs/HEAD");

    if !head_path.exists() {
        return Ok((None, None));
    }

    let head_content = std::fs::read_to_string(&head_path)?.trim().to_string();

    if head_content.starts_with("ref: ") {
        let ref_name = &head_content[5..]; // e.g. "refs/heads/main"
        let ref_path = root.join(".protovcs").join(ref_name);

        // Extract a short branch name for the decoration ("main", not the full ref path).
        let branch = ref_name
            .strip_prefix("refs/heads/")
            .unwrap_or(ref_name)
            .to_string();

        if !ref_path.exists() {
            // Branch file doesn't exist yet — no commits on this branch.
            return Ok((None, Some(branch)));
        }

        let hash = std::fs::read_to_string(&ref_path)?.trim().to_string();
        if hash.is_empty() {
            return Ok((None, Some(branch)));
        }

        Ok((Some(hash), Some(branch)))
    } else if !head_content.is_empty() {
        // Detached HEAD — a raw commit hash with no branch name.
        Ok((Some(head_content), None))
    } else {
        Ok((None, None))
    }
}

/// Load and deserialize a commit object from the loose-object store.
fn load_commit(root: &PathBuf, hash: &str) -> Result<Commit> {
    if hash.len() < 3 {
        anyhow::bail!("Corrupt commit hash (too short): {}", hash);
    }

    let obj_path = root
        .join(".protovcs/objects")
        .join(&hash[..2])
        .join(&hash[2..]);

    if !obj_path.exists() {
        anyhow::bail!(
            "Missing commit object: {} (expected at {:?})",
            hash,
            obj_path
        );
    }

    let commit: Commit = serde_json::from_reader(std::fs::File::open(&obj_path)?)?;
    Ok(commit)
}

/// Format a Unix timestamp as a human-readable local time string.
/// Output: "2026-02-28 14:41:22 -0800"  (matches git's default date format)
fn format_timestamp(unix_secs: u64) -> String {
    match Local.timestamp_opt(unix_secs as i64, 0) {
        chrono::LocalResult::Single(dt) => {
            let dt: DateTime<Local> = dt;
            dt.format("%Y-%m-%d %H:%M:%S %z").to_string()
        }
        _ => format!("<invalid timestamp: {}>", unix_secs),
    }
}
