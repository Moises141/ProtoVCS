ProtoVCS Roadmap – February 2026
Vision
A decentralized, cryptographically-secure, privacy-first version control system — Git reimagined for a world that doesn’t trust central servers or metadata leaks.
Core Principles (unchanged)

Strong Ed25519 identities
No single point of failure (eventual gossip / DHT target)
Permissioned & verifiable everything
Minimal metadata leakage (Tor/I2P compatibility later)
Git-compatible object model where it makes sense

Phase 0 – Foundation & Security
Status: Mostly complete / hardening in progress

 Secure identity generation & persistence (identity.key)
 Handshake + join flow with nonce replay protection
 Signed push command (client signs filename|content_hash)
 Server-side push verification (signature + hash check)
 Node-level allow_remote_push permission flag
 Receiver enforces permissions (not sender)
 Unauthorized push rejection (403 / "not allowed")
 Authorized push success + file written to received/
 Tamper test green (test-push-tamper.ps1 — post-sign content change → rejected)
 Non-registered sender push rejection test (push without ever joining → 403)
 Per-sender allowlist (optional nice-to-have for Phase 0 polish)

Phase 1 – Core Git Parity
Status: In progress / next 2–6 weeks focus

 Basic object model: blobs, trees, commits
status command (staged / unstaged / untracked — comparing working ↔ index ↔ HEAD)
commit command skeleton (tree from index, parent from HEAD, store commit, update ref)
 Full commit polish: proper error messages, empty commit guard, better output
log command (simple linear history walk from HEAD)
clone / fetch basics (pull objects + refs over HTTP from another node)
 Signed commits (optional extra signature over commit JSON for stronger authorship proof)
 Basic conflict detection on fetch (simple tree vs tree diff)

Phase 2 – Decentralization & Replication
Status: Design & early experiments

Gossip protocol (periodic signed delta pushes of registry + new objects)
Automatic object replication (push new commits/trees to 2–3 Gates or random peers)
Multi-node merge resolution (last-write-wins initially, manual conflict UI later)
Bootstrap / seed node discovery
Remove permanent Host dependency after initial network formation

Phase 3 – Anonymity & Privacy Layer
Status: Planning

Integrate Tor / Arti (onion services for node addresses)
Replace clearnet IPs in registry with .onion addresses
Optional blinded / anonymous push (hide sender pubkey from passive observers)
Nostr-style relay support for metadata hiding

Phase 4 – Polish & Usability
Status: Future

Proper branching & merge support
Text-based conflict resolution helper
Web-of-trust style key verification (optional)
Better CLI UX (colored output, progress bars, interactive prompts)
Documentation, example workflows, contribution guide
Packaging (cargo install, standalone binaries)

Nice-to-haves (whenever we feel playful)

Per-sender permissions (allow_push only from specific pubkeys)
Rate limiting / anti-DoS
Wire compression (zstd)
Simple Tauri GUI companion
Git import/export bridge
