use anyhow::Result;
use axum::{
    extract::{Json, State},
    routing::{get, post},
    Router,
    http::StatusCode,
    response::IntoResponse,
};
use ed25519_dalek::{Signature, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use sha2::{Sha256, Digest};
use hex;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Permissions {
    pub allow_read: bool,
    pub allow_write: bool, 
    pub allow_remote_push: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum NodeRole {
    Host,    
    Gate,    
    Member,  
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NodeInfo {
    pub pub_key: String, 
    pub role: NodeRole,
    pub address: String,
    #[serde(default)]
    pub permissions: Permissions,
}

#[derive(Serialize, Deserialize)]
pub struct JoinPayload {
    pub info: NodeInfo,
    pub sig_hex: String,
    pub nonce_hex: String,
}

#[derive(Serialize, Deserialize)]
pub struct PushPayload {
    pub pub_key: String,
    pub filename: String,
    pub content_hex: String,
    pub sig_hex: String,
    pub content_hash: String,
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct NetworkRegistry {
    pub nodes: HashMap<String, NodeInfo>,
    pub votes: HashMap<String, HashSet<String>>,
    #[serde(skip)]
    pub challenges: HashMap<String, u64>,
}

impl NetworkRegistry {
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join(".protovcs/registry.json");
        if path.exists() {
            let file = fs::File::open(path)?;
            Ok(serde_json::from_reader(file).unwrap_or_default())
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self, root: &Path) -> Result<()> {
        let path = root.join(".protovcs/registry.json");
        let file = fs::File::create(path)?;
        serde_json::to_writer_pretty(file, self)?;
        Ok(())
    }
}

pub struct AppState {
    pub root: PathBuf,
    pub registry: RwLock<NetworkRegistry>,
    pub shutdown_tx: tokio::sync::broadcast::Sender<()>,
    pub identity: SigningKey,
}

// --- Identity Management ---

pub fn get_or_create_identity(root: &Path) -> Result<SigningKey> {
    let id_path = root.join(".protovcs/identity.key");
    
    if id_path.exists() {
        let bytes = fs::read(id_path)?;
        let array: [u8; 32] = bytes.try_into().map_err(|_| anyhow::anyhow!("Invalid key length"))?;
        Ok(SigningKey::from_bytes(&array))
    } else {
        let mut rng = StdRng::from_entropy(); 
        let signing_key = SigningKey::generate(&mut rng);
        
        if let Some(parent) = id_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        fs::write(id_path, signing_key.to_bytes())?;
        Ok(signing_key)
    }
}

// --- Handshake ---

const CHALLENGE_EXPIRY_SECS: u64 = 60;

async fn handshake_handler(State(state): State<Arc<AppState>>) -> String {
    let mut rng = rand::thread_rng();
    let mut nonce = [0u8; 32];
    rng.fill(&mut nonce);
    let nonce_hex = hex::encode(nonce);
    
    let expiry = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() + CHALLENGE_EXPIRY_SECS;

    state.registry.write().unwrap().challenges.insert(nonce_hex.clone(), expiry);
    
    format!("CHALLENGE:{}", nonce_hex)
}

async fn join_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<JoinPayload>,
) -> Result<String, String> {
    // 1. Verify challenge first (needs write lock)
    {
        let mut registry = state.registry.write().unwrap();
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

        match registry.challenges.remove(&payload.nonce_hex) {
            Some(expiry) if now > expiry => return Err("Handshake challenge expired".into()),
            None => return Err("Invalid or reused handshake challenge".into()),
            _ => {}
        }
    } // Lock released here

    // 2. Verify signature (no lock needed)
    let pub_key_bytes = hex::decode(&payload.info.pub_key)
        .map_err(|_| "Invalid Public Key hex format".to_string())?;
    let pub_key_array: [u8; 32] = pub_key_bytes.try_into()
        .map_err(|_| "Public Key must be 32 bytes".to_string())?;
    let verifying_key = VerifyingKey::from_bytes(&pub_key_array)
        .map_err(|_| "Invalid Ed25519 Public Key".to_string())?;

    let sig_bytes = hex::decode(&payload.sig_hex)
        .map_err(|_| "Invalid Signature hex format".to_string())?;
    let sig_array: [u8; 64] = sig_bytes.try_into()
        .map_err(|_| "Signature must be 64 bytes".to_string())?;
    let signature = Signature::from_bytes(&sig_array);

    verifying_key.verify(payload.nonce_hex.as_bytes(), &signature)
        .map_err(|_| "Signature verification failed".to_string())?;

    // 3. Update registry (needs write lock again)
    {
        let mut registry = state.registry.write().unwrap();
        let host_exists = registry.nodes.values().any(|n| n.role == NodeRole::Host);
        let (role, msg) = if !host_exists {
            (NodeRole::Host, "Accepted: You are the Host.")
        } else {
            (NodeRole::Member, "Accepted: Joined as Member.")
        };

        let mut info = payload.info;
        info.role = role;
        registry.nodes.insert(info.pub_key.clone(), info);
        registry.save(&state.root).map_err(|e| e.to_string())?;
        
        Ok(msg.into())
    }
}

// --- Vote ---

async fn vote_handler(
    State(state): State<Arc<AppState>>,
    Json((candidate, voter, sig_hex)): Json<(String, String, String)>,
) -> Result<String, String> {
    // 1. Check voter exists and verify signature (read lock)
    {
        let registry = state.registry.read().unwrap();
        
        if !registry.nodes.contains_key(&voter) {
            return Err("Voter is not a member of the network".into());
        }
    } // Release read lock before crypto operations

    let voter_key = VerifyingKey::from_bytes(
        &hex::decode(&voter).map_err(|_| "Invalid Voter Key")?
            .try_into().map_err(|_| "Invalid Voter Key Length")?
    ).map_err(|_| "Invalid Verify Key")?;

    let signature = Signature::from_bytes(
        &hex::decode(&sig_hex).map_err(|_| "Invalid Signature")?
            .try_into().map_err(|_| "Invalid Signature Length")?
    );

    voter_key.verify(candidate.as_bytes(), &signature)
        .map_err(|_| "Invalid Vote Signature")?;

    // 2. Record vote (write lock)
    {
        let mut registry = state.registry.write().unwrap();
        let votes = registry.votes.entry(candidate.clone()).or_default();
        votes.insert(voter.clone());

        let vote_count = votes.len();
        
        if vote_count > 1 {
            if let Some(node) = registry.nodes.get_mut(&candidate) {
                if node.role == NodeRole::Member {
                    node.role = NodeRole::Gate;
                    let _ = registry.save(&state.root);
                    return Ok(format!("Vote recorded. Node promoted to Gate ({} votes)", vote_count));
                }
            }
        }
        
        let _ = registry.save(&state.root);
        Ok(format!("Vote recorded. Total votes: {}", vote_count))
    }
}

// --- Shutdown ---

async fn shutdown_handler(
    State(state): State<Arc<AppState>>,
    Json((requester, sig_hex)): Json<(String, String)>,
) -> Result<String, String> {
    // 1. Check role (read lock)
    {
        let registry = state.registry.read().unwrap();
        
        let node = registry.nodes.get(&requester)
            .ok_or("Requester not found")?;
        
        if node.role != NodeRole::Host {
            return Err("Only the Host can shut down the server".into());
        }
    } // Release lock

    // 2. Verify signature (no lock)
    let req_key = VerifyingKey::from_bytes(
        &hex::decode(&requester).map_err(|_| "Invalid Requester Key")?
            .try_into().map_err(|_| "Invalid Key Length")?
    ).map_err(|_| "Invalid Verify Key")?;

    let signature = Signature::from_bytes(
        &hex::decode(&sig_hex).map_err(|_| "Invalid Signature")?
            .try_into().map_err(|_| "Invalid Signature Length")?
    );

    req_key.verify(b"SHUTDOWN", &signature)
        .map_err(|_| "Invalid Shutdown Signature")?;

    // 3. Send shutdown (no lock needed)
    let _ = state.shutdown_tx.send(());
    Ok("Server shutting down in 5 seconds...".into())
}

// --- Permissions ---

async fn permissions_handler(
    State(state): State<Arc<AppState>>,
    Json((requester, new_perms, sig_hex)): Json<(String, Permissions, String)>,
) -> Result<String, String> {
    // 1. Verify signature (no lock needed)
    let req_key = VerifyingKey::from_bytes(
        &hex::decode(&requester).map_err(|_| "Invalid Requester Key")?
            .try_into().map_err(|_| "Invalid Key Length")?
    ).map_err(|_| "Invalid Verify Key")?;

    let signature = Signature::from_bytes(
        &hex::decode(&sig_hex).map_err(|_| "Invalid Signature")?
            .try_into().map_err(|_| "Invalid Signature Length")?
    );

    let perm_bytes = serde_json::to_vec(&new_perms).map_err(|_| "Serialization Error")?;
    req_key.verify(&perm_bytes, &signature)
        .map_err(|_| "Invalid Permission Signature")?;

    // 2. Update permissions (write lock)
    {
        let mut registry = state.registry.write().unwrap();
        
        let node = registry.nodes.get_mut(&requester)
            .ok_or("Node not found in registry".to_string())?;
        
        node.permissions = new_perms;
        registry.save(&state.root).map_err(|e| e.to_string())?;
        Ok("Permissions updated.".into())
    }
}

// --- Push ---

async fn push_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<PushPayload>,
) -> impl IntoResponse {
    // Debug logging
    println!("DEBUG [server]: Received push from {}", payload.pub_key);
    println!("DEBUG [server]: Filename: {}, Content hash: {}", payload.filename, payload.content_hash);

    // 1. Check permissions (read lock)
    {
        let registry = state.registry.read().unwrap();
        let my_pub_key = hex::encode(state.identity.verifying_key().to_bytes());

        let my_node = match registry.nodes.get(&my_pub_key) {
            Some(n) => n,
            None => return error_response(StatusCode::INTERNAL_SERVER_ERROR, "Receiver not in registry"),
        };

        if !my_node.permissions.allow_remote_push {
            return error_response(StatusCode::FORBIDDEN, "Remote push not allowed");
        }

        if !registry.nodes.contains_key(&payload.pub_key) {
            return error_response(StatusCode::FORBIDDEN, "Sender not registered");
        }
    } // Lock released

    // 2. Verify signature (no lock)
    let verifying_key = match decode_pubkey(&payload.pub_key) {
        Ok(vk) => vk,
        Err(e) => return error_response(StatusCode::BAD_REQUEST, &e),
    };

    let signature = match decode_signature(&payload.sig_hex) {
        Ok(sig) => sig,
        Err(e) => return error_response(StatusCode::BAD_REQUEST, &e),
    };

    let mut to_verify = payload.filename.as_bytes().to_vec();
    to_verify.extend_from_slice(b"|");
    to_verify.extend_from_slice(payload.content_hash.as_bytes());

    if verifying_key.verify(&to_verify, &signature).is_err() {
        return error_response(StatusCode::UNAUTHORIZED, "Invalid signature");
    }

    // 3. Verify content and save (no lock)
    let content = match hex::decode(&payload.content_hex) {
        Ok(c) => c,
        Err(e) => return error_response(StatusCode::BAD_REQUEST, &format!("Invalid content hex: {}", e)),
    };

    let actual_hash = hex::encode(Sha256::digest(&content));
    println!("DEBUG [server]: Expected hash: {}, Actual: {}", payload.content_hash, actual_hash);

    if actual_hash != payload.content_hash {
        return error_response(StatusCode::BAD_REQUEST, "Content hash mismatch - possible tampering");
    }

    let recv_dir = state.root.join("received");
    if let Err(e) = fs::create_dir_all(&recv_dir) {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, &format!("Failed to create dir: {}", e));
    }

    let safe_name = sanitize_filename(&payload.filename);
    let target = recv_dir.join(&safe_name);
    
    if let Err(e) = fs::write(&target, content) {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, &format!("Failed to write file: {}", e));
    }

    let short_key = &payload.pub_key[..16.min(payload.pub_key.len())];
    let msg = format!("File received: {} (from {})", payload.filename, short_key);
    
    (StatusCode::OK, msg).into_response()
}

// --- Helper functions ---

fn error_response(status: StatusCode, msg: &str) -> axum::response::Response {
    (status, msg.to_string()).into_response()
}

fn decode_pubkey(pub_key: &str) -> Result<VerifyingKey, String> {
    let bytes = hex::decode(pub_key).map_err(|_| "Invalid pubkey hex".to_string())?;
    let array: [u8; 32] = bytes.try_into().map_err(|_| "Pubkey must be 32 bytes".to_string())?;
    VerifyingKey::from_bytes(&array).map_err(|_| "Invalid Ed25519 public key".to_string())
}

fn decode_signature(sig_hex: &str) -> Result<Signature, String> {
    let bytes = hex::decode(sig_hex).map_err(|_| "Invalid signature hex")?;
    let array: [u8; 64] = bytes.try_into().map_err(|_| "Signature must be 64 bytes")?;
    Ok(Signature::from_bytes(&array))
}

fn sanitize_filename(filename: &str) -> String {
    Path::new(filename)
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.chars()
            .map(|c| if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
            .collect())
        .unwrap_or_else(|| "unnamed_file".to_string())
}

// --- Server startup ---

pub async fn run_server(root: PathBuf, port: u16) -> Result<()> {
    let mut registry = NetworkRegistry::load(&root)?;
    let id = get_or_create_identity(&root)?;
    let pub_key = hex::encode(id.verifying_key().to_bytes());
    
    if !registry.nodes.contains_key(&pub_key) {
        let my_info = NodeInfo {
            pub_key: pub_key.clone(),
            role: NodeRole::Member, 
            address: format!("127.0.0.1:{}", port),
            permissions: Permissions::default(),
        };
        registry.nodes.insert(pub_key, my_info);
        registry.save(&root)?;
    }

    let (tx, mut rx) = tokio::sync::broadcast::channel(1);

    let shared_state = Arc::new(AppState {
        root: root.clone(),
        registry: RwLock::new(registry),
        shutdown_tx: tx,
        identity: id,
    });

    let app = Router::new()
        .route("/handshake", get(handshake_handler))
        .route("/join", post(join_handler))
        .route("/vote", post(vote_handler))
        .route("/shutdown", post(shutdown_handler))
        .route("/permissions", post(permissions_handler))
        .route("/push", post(push_handler))
        .with_state(shared_state);

    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("ProtoVCS Node online at {}", addr);
    
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            tokio::select! {
                _ = rx.recv() => println!("Remote shutdown signal received."),
                _ = tokio::signal::ctrl_c() => println!("Ctrl+C received."),
            }
            println!("Stopping server...");
        })
        .await?;
    Ok(())
}