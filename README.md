# ProtoVCS

ProtoVCS is an experimental decentralized version control system (VCS) built in Rust. Inspired by Git but designed for privacy and decentralization, it uses cryptographic identities (Ed25519) for secure, pseudonymous collaboration. Nodes form a network with roles (Host, Gate, Member) and support basic VCS operations alongside peer-to-peer features like signed pushes and permission controls. It's still in early development—think of it as a prototype for a more sovereign future of code sharing.

## Key Features
- **Cryptographic Security**: Every node has a unique Ed25519 keypair for signing commits, pushes, and network actions.
- **Decentralized Networking**: Nodes join via handshake, vote on roles, and exchange data over HTTP (with plans for gossip and anonymity layers).
- **Git-Like Workflow**: Basic `init`, `add`, `commit`, and `status` commands, with object storage for blobs, trees, and commits.
- **Permissions & Push**: Control read/write/push access; secure file pushes with signature and hash verification.
- **Testing Focus**: Comes with PowerShell scripts for verifying security (e.g., tamper detection, authorization).
- **Roadmap-Driven**: See the [Roadmap](#roadmap) for upcoming features like full decentralization and anonymity.

## Project Structure
- **`src/main.rs`**: CLI entry point; handles commands like init, add, commit, and network ops.
- **`src/network.rs`**: Networking logic, including Axum server, handshake, join/vote handlers, permissions, and push endpoint.
- **`src/objects.rs`**: Defines core VCS objects (Index, Tree, Commit) with serialization and hashing.
- **`src/status.rs`**: Implements `status` command to show staged/unstaged/untracked changes.
- **`.protovcs/`** (runtime dir): Stores objects, refs, identity.key, index, and registry.json.
- **Tests**: PowerShell scripts in root (e.g., `test-push-tamper.ps1`) for automated verification.

## Installation
ProtoVCS is a Rust binary. Clone the repo and build with Cargo:

