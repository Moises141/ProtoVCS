mod network;
mod objects;
mod status;
mod log;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::fs;
use std::path::PathBuf;
use sha2::{Sha256, Digest};
use ed25519_dalek::Signer;
use chrono::Utc;

use objects::{Tree, Commit, Index};
use status::get_status;

#[derive(Parser)]
#[command(name = "proto", about = "ProtoVCS - The Decentralized VCS")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Init,
    Add { paths: Vec<PathBuf> },
    Commit {
        /// The commit message (can be multi-word)
        #[arg(short = 'm', long = "message")]
        message: String,
    },
    Status,
    Log {
        /// Show a one-line summary per commit
        #[arg(short = '1', long = "oneline")]
        oneline: bool,
    },
    Serve {
        #[arg(default_value = "3333")]
        port: u16
    },
    Join { address: String },
    Vote {
        pub_key: String,
        #[arg(default_value = "http://127.0.0.1:3333")]
        address: String
    },
    Shutdown {
        #[arg(default_value = "http://127.0.0.1:3333")]
        address: String
    },
    Whoami,
    Permissions {
        #[arg(long)]
        allow_push: Option<bool>,
        #[arg(long)]
        allow_read: Option<bool>,
        #[arg(long)]
        allow_write: Option<bool>,
        #[arg(short, long, default_value = "http://127.0.0.1:3333")]
        address: String,
    },
    Push {
        file: PathBuf,
        #[arg(default_value = "http://127.0.0.1:3333")]
        address: String,
        #[arg(long, hide = true, default_value_t = false)]
        tamper: bool,
    },
}

fn cmd_commit(message: String) -> Result<()> {
    let root = find_repo_root()?;
    let index_path = root.join(".protovcs/index");
    
    if !index_path.exists() {
        anyhow::bail!("No index found - run 'proto add' first");
    }

    let index: Index = serde_json::from_reader(fs::File::open(&index_path)?)?;
    
    // Build and store tree
    let tree = Tree::from_index(index);
    if tree.is_empty() {
        anyhow::bail!("No changes to commit");
    }
    
    let tree_hash = tree.hash();
    tree.store(&root)?;
    println!("Created tree {}", tree_hash);

    // Get identity
    let id = network::get_or_create_identity(&root)?;
    let author = hex::encode(id.verifying_key().to_bytes());
    let timestamp = Utc::now().timestamp() as u64;

    // Get parent from HEAD
    let mut parents = Vec::new();
    let head_path = root.join(".protovcs/HEAD");
    
    if head_path.exists() {
        let head_content = fs::read_to_string(&head_path)?.trim().to_string();
        
        if head_content.starts_with("ref: ") {
            let ref_name = &head_content[5..];
            let ref_path = root.join(".protovcs").join(ref_name);
            
            if ref_path.exists() {
                let parent_hash = fs::read_to_string(&ref_path)?.trim().to_string();
                if !parent_hash.is_empty() {
                    parents.push(parent_hash);
                }
            }
        } else if !head_content.is_empty() {
            parents.push(head_content);
        }
    }

    // Create and store commit
    let commit = Commit {
        tree: tree_hash,
        parents,
        author,
        timestamp,
        message: message.clone(),
    };

    let commit_hash = commit.store(&root)?;
    println!("Created commit {}", commit_hash);

    // Update HEAD
    let main_ref = root.join(".protovcs/refs/heads/main");
    fs::create_dir_all(main_ref.parent().unwrap())?;
    fs::write(&main_ref, &commit_hash)?;
    fs::write(&head_path, "ref: refs/heads/main\n")?;

    // Clear index
    fs::remove_file(&index_path).ok();

    println!("[{}] {}", &commit_hash[..8], message);
    Ok(())
}

fn clean_path(path: PathBuf) -> PathBuf {
    let path_str = path.to_string_lossy();
    if path_str.starts_with(r"\\?\") {
        PathBuf::from(&path_str[4..])
    } else {
        path
    }
}

fn find_repo_root() -> Result<PathBuf> {
    let mut curr = std::env::current_dir()?;
    loop {
        if curr.join(".protovcs").is_dir() {
            return Ok(clean_path(fs::canonicalize(&curr)?));
        }
        if !curr.pop() {
            anyhow::bail!("fatal: Not a protovcs repository (or any parent directory)");
        }
    }
}

fn cmd_add(paths: Vec<PathBuf>) -> Result<()> {
    let root = find_repo_root()?;
    let index_path = root.join(".protovcs/index");
    let mut index: Index = if index_path.exists() {
        serde_json::from_reader(fs::File::open(&index_path)?)?
    } else {
        Index::default()
    };

    for path in paths {
        let abs_path = if path.is_absolute() { 
            path 
        } else { 
            std::env::current_dir()?.join(path) 
        };
        
        let canonical = clean_path(fs::canonicalize(&abs_path)
            .with_context(|| format!("Could not find file: {:?}", abs_path))?);
        
        let rel_path = canonical.strip_prefix(&root)?
            .to_string_lossy()
            .replace('\\', "/");
        
        let content = fs::read(&canonical)?;
        let hash = hex::encode(Sha256::digest(&content));
        
        let obj_dir = root.join(".protovcs/objects").join(&hash[..2]);
        fs::create_dir_all(&obj_dir)?;
        fs::write(obj_dir.join(&hash[2..]), content)?;

        index.entries.insert(rel_path, hash);
    }

    fs::write(index_path, serde_json::to_string_pretty(&index)?)?;
    println!("Staged changes.");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => {
            let root = std::env::current_dir()?;
            let proto_dir = root.join(".protovcs");
            fs::create_dir_all(proto_dir.join("objects"))?;
            fs::create_dir_all(proto_dir.join("refs/heads"))?;
            fs::write(proto_dir.join("HEAD"), "ref: refs/heads/main\n")?;
            println!("Initialized empty ProtoVCS repository.");
            Ok(())
        }
        Commands::Add { paths } => cmd_add(paths),
        Commands::Commit { message } => cmd_commit(message),
        Commands::Log { oneline } => {
            let root = find_repo_root()?;
            let entries = log::get_log(&root)?;
            log::print_log(&entries, oneline);
            Ok(())
        }
        Commands::Status => {
            let root = find_repo_root()
                .context("Must be inside a ProtoVCS repository to run status")?;

            let status = get_status(&root)?;

            println!("On branch main");  // TODO: later read actual branch name

            let mut has_changes = false;

            // Staged changes
            if !status.staged.is_empty() {
                has_changes = true;
                println!("\nChanges to be committed:");
                println!("  (use \"proto reset <file>...\" to unstage)");
                for change in &status.staged {
                    let prefix = match change.change_type {
                        status::ChangeType::New => "new file:  ",
                        status::ChangeType::Modified => "modified: ",
                        status::ChangeType::Deleted => "deleted:  ",
                    };
                    println!("        {}{}", prefix, change.path);
                }
            }

            // Unstaged changes
            if !status.unstaged.is_empty() {
                has_changes = true;
                println!("\nChanges not staged for commit:");
                println!("  (use \"proto add <file>...\" to update what will be committed)");
                println!("  (use \"proto restore <file>...\" to discard changes)");
                for change in &status.unstaged {
                    let prefix = match change.change_type {
                        status::ChangeType::New => "new file:  ",
                        status::ChangeType::Modified => "modified: ",
                        status::ChangeType::Deleted => "deleted:  ",
                    };
                    println!("        {}{}", prefix, change.path);
                }
            }

            // Untracked files
            if !status.untracked.is_empty() {
                has_changes = true;
                println!("\nUntracked files:");
                println!("  (use \"proto add <file>...\" to include in what will be committed)");
                for path in &status.untracked {
                    println!("        {}", path);
                }
            }

            if !has_changes {
                println!("\nnothing to commit, working tree clean");
            }

            Ok(())
        }
        Commands::Serve { port } => {
            let root = find_repo_root()?;
            network::run_server(root, port).await
        }
        Commands::Join { address } => {
            let root = find_repo_root()?;
            let id = network::get_or_create_identity(&root)?;
            
            let client = reqwest::Client::new();
            
            let challenge_resp = client.get(format!("{}/handshake", address))
                .send()
                .await?
                .text()
                .await?;
            
            if !challenge_resp.starts_with("CHALLENGE:") {
                anyhow::bail!("Invalid handshake response from host");
            }
            
            let nonce_hex = &challenge_resp["CHALLENGE:".len()..];
            let _nonce_bytes = hex::decode(nonce_hex)
                .map_err(|_| anyhow::anyhow!("Invalid nonce hex from host"))?;

            let signature = id.sign(nonce_hex.as_bytes());
            
            let my_info = network::NodeInfo {
                pub_key: hex::encode(id.verifying_key().to_bytes()),
                role: network::NodeRole::Member,
                address: "127.0.0.1".into(),
                permissions: network::Permissions::default(),
            };

            let payload = network::JoinPayload {
                info: my_info.clone(),
                sig_hex: hex::encode(signature.to_bytes()),
                nonce_hex: nonce_hex.to_string(),
            };

            let res = client.post(format!("{}/join", address))
                .json(&payload)
                .send()
                .await?;
            
            let response_text = res.text().await?;
            println!("Host Response: {}", response_text);
            Ok(())
        }
        Commands::Vote { pub_key, address } => {
            let root = find_repo_root()?;
            let id = network::get_or_create_identity(&root)?;
            let client = reqwest::Client::new();
            let signature = id.sign(pub_key.as_bytes());
            let voter_pub_key = hex::encode(id.verifying_key().to_bytes());
            
            let res = client.post(format!("{}/vote", address))
                .json(&(pub_key, voter_pub_key, hex::encode(signature.to_bytes())))
                .send()
                .await?;
            
            println!("Vote Response: {}", res.text().await?);
            Ok(())
        }
        Commands::Shutdown { address } => {
            let root = find_repo_root()?;
            let id = network::get_or_create_identity(&root)?;
            let client = reqwest::Client::new();
            
            const SHUTDOWN_MSG: &[u8] = b"SHUTDOWN";
            let signature = id.sign(SHUTDOWN_MSG);
            let requester_pub_key = hex::encode(id.verifying_key().to_bytes());
            
            let res = client.post(format!("{}/shutdown", address))
                .json(&(requester_pub_key, hex::encode(signature.to_bytes())))
                .send()
                .await?;
            
            println!("Server Response: {}", res.text().await?);
            Ok(())
        }
        Commands::Whoami => {
            let root = find_repo_root()?;
            let id = network::get_or_create_identity(&root)?;
            println!("{}", hex::encode(id.verifying_key().to_bytes()));
            Ok(())
        }
        Commands::Permissions { allow_push, allow_read, allow_write, address } => {
            let root = find_repo_root()?;
            let id = network::get_or_create_identity(&root)?;
            let client = reqwest::Client::new();

            let mut perms = network::Permissions {
                allow_read: true,
                allow_write: true,
                allow_remote_push: false,
            };
            
            if let Some(v) = allow_push { perms.allow_remote_push = v; }
            if let Some(v) = allow_read { perms.allow_read = v; }
            if let Some(v) = allow_write { perms.allow_write = v; }
            
            let requester_pub_key = hex::encode(id.verifying_key().to_bytes());
            let perm_bytes = serde_json::to_vec(&perms)?;
            let signature = id.sign(&perm_bytes);
            
            let res = client.post(format!("{}/permissions", address))
                .json(&(requester_pub_key, perms, hex::encode(signature.to_bytes())))
                .send()
                .await?;
                
            println!("Permissions Response: {}", res.text().await?);
            Ok(())
        }
        Commands::Push { file, address, tamper } => {
            if !file.exists() {
                anyhow::bail!("File not found: {:?}", file);
            }

            let base_dir = find_repo_root()
                .context("Push must be run inside a ProtoVCS repository")?;
            let id = network::get_or_create_identity(&base_dir)?;
            let pub_key_hex = hex::encode(id.verifying_key().to_bytes());

            let content = fs::read(&file)?;
            let content_hash = hex::encode(Sha256::digest(&content));
            let filename = file.file_name().context("Invalid filename")?.to_string_lossy().to_string();

            println!("DEBUG [client push]: File path = {:?}", file);
            println!("DEBUG [client push]: Content length = {} bytes", content.len());
            println!("DEBUG [client push]: Computed content_hash = {}", content_hash);
            println!("DEBUG [client push]: Signing over filename|hash = {}|{}", filename, content_hash);

            if tamper {
                println!("DEBUG [client push]: TAMPER MODE ACTIVATED - sending mismatched content while keeping original hash & signature!");
            }

            let mut to_sign = filename.as_bytes().to_vec();
            to_sign.extend_from_slice(b"|");
            to_sign.extend_from_slice(content_hash.as_bytes());

            let signature = id.sign(&to_sign);
            let sig_hex = hex::encode(signature.to_bytes());

            let content_to_send = if tamper {
                b"THIS IS POST-SIGN TAMPERED CONTENT - THE HASH CHECK SHOULD REJECT THIS!".to_vec()
            } else {
                content
            };

            let payload = network::PushPayload {
                pub_key: pub_key_hex,
                filename,
                content_hex: hex::encode(&content_to_send),
                sig_hex,
                content_hash,
            };

            let client = reqwest::Client::new();
            let res = client.post(format!("{}/push", address))
                .json(&payload)
                .send()
                .await?;

            if res.status().is_success() {
                println!("File pushed successfully!");
            } else {
                anyhow::bail!("Push failed: {}", res.text().await?);
            }
            Ok(())
        }
    }
}
